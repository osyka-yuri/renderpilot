//! Update detection for installed RenoDX add-ons and shared ReShade Vulkan layer.

use renderpilot_application::{InstalledAddonRepository, SharedArtifactRepository};
use renderpilot_domain::{
    Architecture, GameId, InstalledAddon, InstalledAddonHostKind, TrackedSourceRole,
};

use crate::addons::renodx::dto::update::RenoDxUpdateReport;
use crate::addons::renodx::dto::vulkan::{LayerDiagnosticReason, VulkanLayerDetection};
use crate::addons::renodx::platform::vulkan::validation::{
    LayerUpdateVerdict, resolve_digest_verdict,
};
use crate::addons::renodx::types::{RenoDxManifest, ReshadeChannel, ReshadeConfig};
use crate::addons::renodx::use_cases::reshade_update::{
    addon_label, recorded_reshade_channel, resolve_host_update_target, source_with_role,
};
use crate::addons::renodx::{channel, fetch, source, vulkan};
use crate::addons::update::{UpdateStatus, digest_verdict, validator_fast_path};
use crate::net::head_validators;
use crate::{Context, ServiceError};
/// Checks whether the installed add-on for `game_id` has an upstream update.
pub async fn check_update(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
) -> Result<RenoDxUpdateReport, ServiceError> {
    match context.storage().get_installed_addon(game_id)? {
        Some(record) => Ok(check_record(context, manifest, &record).await),
        None => Ok(RenoDxUpdateReport::new(None, None, None)),
    }
}

/// Bulk update check over every installed RenoDX add-on.
pub async fn check_updates(
    context: &Context,
    manifest: &RenoDxManifest,
) -> Result<Vec<(GameId, UpdateStatus)>, ServiceError> {
    let records = context.storage().list_installed_addons()?;
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let report = check_record(context, manifest, &record).await;
        out.push((record.game_id().clone(), report.overall));
    }
    Ok(out)
}

async fn check_record(
    context: &Context,
    manifest: &RenoDxManifest,
    record: &InstalledAddon,
) -> RenoDxUpdateReport {
    let addon = check_addon(record).await;
    let host_check = check_host(context, manifest, record).await;
    let dlss_fix = check_dlss_fix(record).await;
    RenoDxUpdateReport::with_vulkan_diagnostics(
        addon,
        host_check.status,
        dlss_fix,
        host_check.vulkan_diagnostics,
    )
}

/// Result of checking the ReShade host for updates, carrying both the update
/// status and any Vulkan-layer digest-mismatch diagnostics.
struct HostCheckResult {
    status: Option<UpdateStatus>,
    vulkan_diagnostics: Vec<LayerDiagnosticReason>,
}

impl HostCheckResult {
    fn none() -> Self {
        Self {
            status: None,
            vulkan_diagnostics: Vec::new(),
        }
    }
}

/// Update verdict for the add-on payload. A file install records no add-on source,
/// so there is nothing upstream to compare — it contributes `None`.
async fn check_addon(record: &InstalledAddon) -> Option<UpdateStatus> {
    let addon = source_with_role(record, TrackedSourceRole::AddonPayload)?;
    if addon.url().is_empty() {
        return None;
    }
    if let Ok(validators) = head_validators(addon.url(), "RenoDX update check").await {
        let current = validators.cache_validator();
        if let Some(status) = validator_fast_path(addon.etag(), current.as_deref()) {
            return Some(status);
        }
    }
    match fetch::fetch_addon(addon.url(), addon_label(record), None).await {
        Ok(download) => Some(digest_verdict(addon.digest(), &download.digest)),
        Err(_) => Some(UpdateStatus::Unknown),
    }
}

/// Update verdict for a recorded ReShade host artifact. The durable comparison is the
/// digest of the extracted DLL for the installed channel; validators are only a
/// fast path when the manifest URL did not change. If the recorded digest does not
/// match the recorded channel's upstream artifact, the other known channel is
/// checked — a match there is a channel mismatch, not an update. A nightly host
/// whose digest cannot be confirmed upstream degrades to needs-validation rather
/// than a silent "current", since a PE-version match alone is never sole proof.
async fn check_host(
    context: &Context,
    manifest: &RenoDxManifest,
    record: &InstalledAddon,
) -> HostCheckResult {
    if matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    ) {
        let channel = recorded_reshade_channel(record)
            .map(|channel| manifest.reshade.effective_install_channel(channel))
            .unwrap_or_else(|| {
                manifest
                    .reshade
                    .effective_install_channel(ReshadeChannel::Stable)
            });
        match check_layer_update(context.storage(), &manifest.reshade, channel).await {
            Some(verdict) => HostCheckResult {
                status: Some(verdict.status),
                vulkan_diagnostics: verdict.diagnostics,
            },
            None => HostCheckResult::none(),
        }
    } else {
        HostCheckResult {
            status: check_proxy_host(context, manifest, record).await,
            vulkan_diagnostics: Vec::new(),
        }
    }
}

/// Update verdict for a recorded proxy ReShade host artifact (DirectX path).
/// The durable comparison is the digest of the extracted DLL for the installed
/// channel; validators are only a fast path when the manifest URL did not
/// change. If the recorded digest does not match the recorded channel's
/// upstream artifact, the other known channel is checked — a match there is a
/// channel mismatch, not an update. A nightly host whose digest cannot be
/// confirmed upstream degrades to needs-validation rather than a silent
/// "current", since a PE-version match alone is never sole proof.
async fn check_proxy_host(
    context: &Context,
    manifest: &RenoDxManifest,
    record: &InstalledAddon,
) -> Option<UpdateStatus> {
    let host = match channel::single_host_source(record) {
        Ok(source) => source?,
        Err(channel::ChannelReadIssue::DuplicateHostSources) => return Some(UpdateStatus::Unknown),
    };
    let recorded_channel = recorded_reshade_channel(record)?;
    let target =
        match resolve_host_update_target(context, manifest, record.game_id(), recorded_channel) {
            Ok(target) => target?,
            Err(error) => {
                log::warn!(
                    "RenoDX host update check skipped for {}: {error}",
                    record.game_id()
                );
                return Some(UpdateStatus::Unknown);
            }
        };
    if target.conflict {
        return Some(UpdateStatus::Unknown);
    }
    if target.action.writes_host() {
        return Some(UpdateStatus::Available);
    }
    if target.source.url == host.url()
        && let Ok(validators) = head_validators(host.url(), "ReShade update check").await
    {
        let current = validators.cache_validator();
        if let Some(status) = validator_fast_path(host.etag(), current.as_deref())
            && status == UpdateStatus::Current
        {
            return Some(UpdateStatus::Current);
        }
    }
    let recorded_digest = host.digest().to_owned();
    match fetch::fetch_reshade_from_source(&target.source, target.arch, None).await {
        Ok(download) => {
            if download.digest == recorded_digest {
                return Some(UpdateStatus::Current);
            }
            // The recorded channel's upstream artifact does not match the installed
            // digest. Check whether the digest matches the other known channel — a
            // channel mismatch, not an update.
            if let Some(other) = other_channel_source(manifest, recorded_channel, target.arch)
                && let Ok(other_download) =
                    fetch::fetch_reshade_from_source(&other, target.arch, None).await
                && other_download.digest == recorded_digest
            {
                return Some(UpdateStatus::ChannelMismatch);
            }
            Some(UpdateStatus::Available)
        }
        Err(_) => {
            // A nightly host whose digest cannot be confirmed upstream cannot be
            // silently declared current — a PE-version match alone is never sole
            // proof, so the backend needs stronger validation.
            if recorded_channel == ReshadeChannel::Nightly {
                Some(UpdateStatus::UnknownNeedsValidation)
            } else {
                Some(UpdateStatus::Unknown)
            }
        }
    }
}

/// Resolves the ReShade source for the channel *other than* `recorded`, when the
/// manifest supports it. Used for the cross-channel digest comparison that detects
/// a channel mismatch.
fn other_channel_source(
    manifest: &RenoDxManifest,
    recorded_channel: ReshadeChannel,
    arch: Architecture,
) -> Option<source::ReshadeSource> {
    let other = match recorded_channel {
        ReshadeChannel::Stable => ReshadeChannel::Nightly,
        ReshadeChannel::Nightly => ReshadeChannel::Stable,
    };
    if !manifest.reshade.supports_channel(other) {
        return None;
    }
    source::reshade_source(&manifest.reshade, other, arch)
}

/// Update verdict for the DLSS-Fix companion add-on. Not installed (no DlssFix
/// source) contributes `None`.
async fn check_dlss_fix(record: &InstalledAddon) -> Option<UpdateStatus> {
    let dlss_fix = source_with_role(record, TrackedSourceRole::DlssFix)?;
    if let Ok(validators) = head_validators(dlss_fix.url(), "DLSS-Fix update check").await {
        let current = validators.cache_validator();
        if let Some(status) = validator_fast_path(dlss_fix.etag(), current.as_deref()) {
            return Some(status);
        }
    }
    // Fetch and compare the digest. The recorded URL already encodes the
    // architecture, so no arch derivation is needed here.
    match fetch::fetch_addon(dlss_fix.url(), "DLSS-Fix", None).await {
        Ok(download) => Some(digest_verdict(dlss_fix.digest(), &download.digest)),
        Err(_) => Some(UpdateStatus::Unknown),
    }
}

/// Checks the selected ReShade channel against the standard layer on disk.
///
/// The actual on-disk `ReShade64.dll` digest is the authoritative source of
/// truth. The advisory DB digest is consulted **only** when the DLL is
/// missing or unreadable, and even then a DB-only match never returns a
/// strong [`UpdateStatus::Current`] — it degrades to
/// [`UpdateStatus::UnknownNeedsValidation`] because the disk reality is
/// unknown.
///
/// Broken detection states (Conflict, InstalledDisabled, ExternalReadOnly,
/// Unsupported) never return `Current`.
///
/// Returns the full [`LayerUpdateVerdict`] (status + digest-mismatch
/// diagnostics) so callers can thread precise reasons into the update report.
pub(crate) async fn check_layer_update(
    storage: &impl SharedArtifactRepository,
    reshade_config: &ReshadeConfig,
    channel: ReshadeChannel,
) -> Option<LayerUpdateVerdict> {
    let report = vulkan::layer_report();
    let detection = report.detection();
    if matches!(
        detection,
        VulkanLayerDetection::ExternalReadOnly
            | VulkanLayerDetection::Unsupported
            | VulkanLayerDetection::InstalledDisabled
    ) {
        return Some(LayerUpdateVerdict {
            status: UpdateStatus::Unknown,
            diagnostics: Vec::new(),
        });
    }
    // Non-standard-mutable conflicts cannot be updated; report Unknown.
    if detection == VulkanLayerDetection::Conflict {
        if !vulkan::conflict_is_standard_mutable(&report) {
            return Some(LayerUpdateVerdict {
                status: UpdateStatus::Unknown,
                diagnostics: Vec::new(),
            });
        }
        return Some(LayerUpdateVerdict {
            status: UpdateStatus::Available,
            diagnostics: report.diagnostic_reasons,
        });
    }

    let source = source::reshade_source(reshade_config, channel, Architecture::X64)?;
    let verdict = compute_layer_verdict(storage, source, channel).await;
    Some(verdict)
}

/// Computes the update verdict by comparing the actual on-disk DLL digest
/// against the upstream artifact. The DB digest is advisory fallback only.
async fn compute_layer_verdict(
    storage: &impl SharedArtifactRepository,
    source: source::ReshadeSource,
    channel: ReshadeChannel,
) -> LayerUpdateVerdict {
    // Step 1: Actual DLL digest — the authoritative source of truth.
    let actual_digest = vulkan::current_layer_digest();

    // Step 2: Fetch upstream for comparison.
    let download = match fetch::fetch_reshade_from_source(&source, Architecture::X64, None).await {
        Ok(download) => download,
        Err(_) => {
            return LayerUpdateVerdict {
                status: if channel == ReshadeChannel::Nightly {
                    UpdateStatus::UnknownNeedsValidation
                } else {
                    UpdateStatus::Unknown
                },
                diagnostics: Vec::new(),
            };
        }
    };

    // Step 3: Decide based on digests. Actual DLL wins; DB is advisory only.
    let db_digest = vulkan::stored_layer_digest(storage);
    resolve_digest_verdict(
        actual_digest.as_deref(),
        db_digest.as_deref(),
        &download.digest,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::test_support::manifest;

    #[test]
    fn other_channel_source_resolves_the_opposite_channel() {
        let manifest = manifest(Vec::new());
        let stable = other_channel_source(&manifest, ReshadeChannel::Nightly, Architecture::X64);
        assert!(
            stable.is_some(),
            "stable should be resolvable as the other channel"
        );

        let nightly = other_channel_source(&manifest, ReshadeChannel::Stable, Architecture::X64);
        assert!(
            nightly.is_some(),
            "nightly should be resolvable as the other channel"
        );
    }

    #[test]
    fn other_channel_source_returns_none_when_other_channel_unsupported() {
        let mut manifest = manifest(Vec::new());
        manifest.reshade.stable = None;
        let other = other_channel_source(&manifest, ReshadeChannel::Nightly, Architecture::X64);
        assert!(other.is_none());
    }
}
