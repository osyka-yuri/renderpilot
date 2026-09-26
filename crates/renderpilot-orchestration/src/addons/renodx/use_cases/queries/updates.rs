//! Update detection for installed RenoDX add-ons and shared ReShade Vulkan layer.

use renderpilot_application::SharedArtifactRepository;
use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, InstalledAddonHostKind, RenoDxConfigReceipt,
    RenoDxManagedConfigKey, RenoDxSetPathValue, TrackedSourceRole,
};

use crate::addons::game_analysis::analyze_game;
use crate::addons::records::{self, addon_label, source_with_role};
use crate::addons::renodx::dto::update::RenoDxUpdateReport;
use crate::addons::renodx::dto::vulkan::{LayerDiagnosticReason, VulkanLayerDetection};
use crate::addons::renodx::platform::vulkan::validation::{
    LayerUpdateVerdict, resolve_digest_verdict,
};
use crate::addons::renodx::types::{RenoDxConfig, RenoDxManifest};
use crate::addons::renodx::use_cases::reshade_update::{
    recorded_reshade_channel, resolve_host_update_target,
};
use crate::addons::renodx::{fetch, vulkan};
use crate::addons::reshade::channel;
use crate::addons::reshade::fetch::fetch_reshade_from_source;
use crate::addons::reshade::source::{ReshadeSource, reshade_source};
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::addons::update::{UpdateStatus, digest_verdict, validator_fast_path};
use crate::net::head_validators;
use crate::{Context, ServiceError};
/// Checks whether the installed add-on for `game_id` has an upstream update. A
/// record belonging to a different addon kind (e.g. Luma) reads as "nothing
/// installed" — never checked as if it were a RenoDX install.
pub async fn check_update(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
) -> Result<RenoDxUpdateReport, ServiceError> {
    match records::active_record_of_kind(context, game_id, AddonKind::RenoDx)? {
        Some(record) => Ok(check_record(context, manifest, reshade_sources, &record).await),
        None => Ok(RenoDxUpdateReport::new(None, None, None)),
    }
}

/// Bulk update check over every active RenoDX record. The shared records layer
/// applies kind and tool-presence policy before anything is update-checked.
pub async fn check_updates(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
) -> Result<Vec<(GameId, UpdateStatus)>, ServiceError> {
    let records = installed_renodx_records(context)?;
    let mut out = Vec::new();
    for record in records {
        let report = check_record(context, manifest, reshade_sources, &record).await;
        out.push((record.game_id().clone(), report.overall));
    }
    Ok(out)
}

/// Produces the honest bulk result when the live manifest is unavailable.
/// Presentation layers use this instead of returning an empty map that would
/// imply no installed RenoDX add-ons.
pub fn unknown_updates_for_installed(
    context: &Context,
) -> Result<Vec<(GameId, UpdateStatus)>, ServiceError> {
    Ok(installed_renodx_records(context)?
        .map(|record| (record.game_id().clone(), UpdateStatus::Unknown))
        .collect())
}

fn installed_renodx_records(
    context: &Context,
) -> Result<impl Iterator<Item = InstalledAddon>, ServiceError> {
    records::active_records_of_kind(context, AddonKind::RenoDx)
}

async fn check_record(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
    record: &InstalledAddon,
) -> RenoDxUpdateReport {
    let addon = check_addon(record).await;
    let host_check = check_host(context, manifest, reshade_sources, record).await;
    let dlss_fix = check_dlss_fix(record).await;
    let mut report = RenoDxUpdateReport::with_vulkan_diagnostics(
        addon,
        host_check.status,
        dlss_fix,
        host_check.vulkan_diagnostics,
    );
    if let Some(config) = config_update_status(context, manifest, record) {
        report.overall = crate::addons::update::combine(report.overall, config);
    }
    if record_has_unsupported_settings(context, manifest, record) {
        report.overall = UpdateStatus::Unknown;
    }
    report
}

fn record_has_unsupported_settings(
    context: &Context,
    manifest: &RenoDxManifest,
    record: &InstalledAddon,
) -> bool {
    let Ok(game) = crate::addons::renodx::game_context::require_game(context, record.game_id())
    else {
        return false;
    };
    let analysis = analyze_game(
        &game,
        crate::addons::renodx::game_context::executable_override(context, record.game_id())
            .as_deref(),
    );
    crate::addons::renodx::matcher::has_unsupported_settings(manifest, &analysis.facts)
}

/// ReShade.ini policy availability is derived only from the desired catalogue
/// path and the durable RenoDX configuration receipt. It deliberately does not read the
/// live INI, so a user edit remains a reconcile decision for the explicit
/// update command rather than a background/status mutation.
fn config_update_status(
    context: &Context,
    manifest: &RenoDxManifest,
    record: &InstalledAddon,
) -> Option<UpdateStatus> {
    let game = crate::addons::renodx::game_context::require_game(context, record.game_id()).ok()?;
    let analysis = analyze_game(
        &game,
        crate::addons::renodx::game_context::executable_override(context, record.game_id())
            .as_deref(),
    );
    let receipt = record.renodx_config_receipt();
    match crate::addons::renodx::matcher::resolve(manifest, &analysis.facts) {
        crate::addons::renodx::matcher::RenoDxResolution::Installable(plan) => {
            Some(config_status_for_desired(
                plan.processing_path.desired_set_path(),
                plan.renodx_config.as_ref(),
                receipt,
            ))
        }
        crate::addons::renodx::matcher::RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => Some(config_status_for_desired(
            plan.processing_path.desired_set_path(),
            plan.renodx_config.as_ref(),
            receipt,
        )),
        _ => None,
    }
}

fn config_status_for_desired(
    desired_set_path: Option<RenoDxSetPathValue>,
    desired_config: Option<&RenoDxConfig>,
    receipt: Option<&RenoDxConfigReceipt>,
) -> UpdateStatus {
    let desired_has_config = desired_set_path.is_some()
        || desired_config.is_some_and(|config| !config.settings.is_empty());
    match (desired_has_config, receipt) {
        (false, None) => UpdateStatus::Current,
        (true, Some(receipt))
            if desired_config_matches_receipt(desired_set_path, desired_config, receipt) =>
        {
            UpdateStatus::Current
        }
        _ => UpdateStatus::Available,
    }
}

/// Compares the complete desired typed RenoDX key set with durable receipt
/// provenance.  This deliberately does not rely on the legacy mirror fields:
/// v1 receipts are normalized by `managed_entries`, while v2 receipts expose
/// every owned key.
fn desired_config_matches_receipt(
    desired_set_path: Option<RenoDxSetPathValue>,
    desired_config: Option<&RenoDxConfig>,
    receipt: &RenoDxConfigReceipt,
) -> bool {
    if !receipt.is_supported() {
        return false;
    }
    let settings = desired_config.map_or(&[][..], |config| config.settings.as_slice());
    let expected_count = usize::from(desired_set_path.is_some()) + settings.len();
    let entries = receipt.managed_entries();
    if entries.len() != expected_count {
        return false;
    }
    if let Some(desired) = desired_set_path {
        let expected = desired.as_i32();
        if !entries.iter().any(|entry| {
            entry.key == RenoDxManagedConfigKey::SetPath.as_str() && entry.last_written == expected
        }) {
            return false;
        }
    }
    for (index, setting) in settings.iter().enumerate() {
        if settings[..index]
            .iter()
            .any(|prior| prior.key == setting.key)
        {
            return false;
        }
        if !entries
            .iter()
            .any(|entry| entry.key == setting.key.as_str() && entry.last_written == setting.value)
        {
            return false;
        }
    }
    true
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
    reshade_sources: &ReshadeSourceCatalog,
    record: &InstalledAddon,
) -> HostCheckResult {
    if matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    ) {
        let channel = recorded_reshade_channel(record)
            .unwrap_or_else(|| reshade_sources.default_install_channel());
        if !reshade_sources.supports_channel(channel) {
            return HostCheckResult {
                status: Some(UpdateStatus::Unknown),
                vulkan_diagnostics: Vec::new(),
            };
        }
        match check_layer_update(context.storage(), reshade_sources, channel).await {
            Some(verdict) => HostCheckResult {
                status: Some(verdict.status),
                vulkan_diagnostics: verdict.diagnostics,
            },
            None => HostCheckResult::none(),
        }
    } else {
        HostCheckResult {
            status: check_proxy_host(context, manifest, reshade_sources, record).await,
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
    reshade_sources: &ReshadeSourceCatalog,
    record: &InstalledAddon,
) -> Option<UpdateStatus> {
    let host = match channel::single_host_source(record) {
        Ok(source) => source?,
        Err(channel::ChannelReadIssue::DuplicateHostSources) => return Some(UpdateStatus::Unknown),
    };
    let recorded_channel = recorded_reshade_channel(record)?;
    // `resolve_host_update_target` returns `Ok(None)` for a recognized custom
    // build (e.g. GShade) — never checked for updates, its versioning is its own
    // maintainer's concern — so that guarantee holds here for free.
    let target = match resolve_host_update_target(
        context,
        manifest,
        reshade_sources,
        record.game_id(),
        recorded_channel,
    ) {
        Ok(target) => target?,
        Err(error) => {
            tracing::warn!(
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
    match fetch_reshade_from_source(&target.source, target.arch, None).await {
        Ok(download) => {
            if download.digest == recorded_digest {
                return Some(UpdateStatus::Current);
            }
            // The recorded channel's upstream artifact does not match the installed
            // digest. Check whether the digest matches the other known channel — a
            // channel mismatch, not an update.
            if let Some(other) =
                other_channel_source(reshade_sources, recorded_channel, target.arch)
                && let Ok(other_download) =
                    fetch_reshade_from_source(&other, target.arch, None).await
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
    reshade_sources: &ReshadeSourceCatalog,
    recorded_channel: ReshadeChannel,
    arch: Architecture,
) -> Option<ReshadeSource> {
    let other = match recorded_channel {
        ReshadeChannel::Stable => ReshadeChannel::Nightly,
        ReshadeChannel::Nightly => ReshadeChannel::Stable,
    };
    if !reshade_sources.supports_channel(other) {
        return None;
    }
    reshade_source(reshade_sources, other, arch)
}

/// Update verdict for the DLSS-Fix companion add-on. Not installed (no DlssFix
/// source) contributes `None`.
async fn check_dlss_fix(record: &InstalledAddon) -> Option<UpdateStatus> {
    let binding = crate::addons::renodx::dlss_fix_binding::resolve(record);
    match binding.state {
        crate::addons::renodx::dlss_fix_binding::DlssFixBindingState::None => None,
        crate::addons::renodx::dlss_fix_binding::DlssFixBindingState::Invalid
        | crate::addons::renodx::dlss_fix_binding::DlssFixBindingState::SourceOnly
        | crate::addons::renodx::dlss_fix_binding::DlssFixBindingState::OwnedOnly => {
            Some(UpdateStatus::UnknownNeedsValidation)
        }
        crate::addons::renodx::dlss_fix_binding::DlssFixBindingState::Bound => {
            let crate::file_mutation::V2DiskObservation::Regular { digest } = binding.observation
            else {
                return Some(UpdateStatus::UnknownNeedsValidation);
            };
            let source = binding.source?;
            // A validator is a safe optimization only when the live target still
            // matches the recorded source digest and there is a stored validator
            // to compare. Otherwise skip the inconclusive HEAD request and let the
            // authoritative fetch below compare the live target bytes to upstream.
            if digest == source.digest()
                && source.etag().is_some()
                && let Ok(validators) = head_validators(source.url(), "DLSS-Fix update check").await
            {
                let current = validators.cache_validator();
                if let Some(status) = dlss_fix_validator_fast_path(
                    &digest,
                    source.digest(),
                    source.etag(),
                    current.as_deref(),
                ) {
                    return Some(status);
                }
            }
            match fetch::fetch_addon(source.url(), "DLSS-Fix", None).await {
                Ok(download) => Some(digest_verdict(&digest, &download.digest)),
                Err(_) => Some(UpdateStatus::Unknown),
            }
        }
    }
}

/// Returns the validator verdict only when both the live file and its recorded
/// source digest agree. A matching validator without that local digest guard is
/// insufficient because the managed file may have been edited in place.
fn dlss_fix_validator_fast_path(
    live_digest: &str,
    recorded_digest: &str,
    stored_validator: Option<&str>,
    current_validator: Option<&str>,
) -> Option<UpdateStatus> {
    if live_digest != recorded_digest {
        return None;
    }
    validator_fast_path(stored_validator, current_validator)
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
    reshade_config: &ReshadeSourceCatalog,
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

    let source = reshade_source(reshade_config, channel, Architecture::X64)?;
    let verdict = compute_layer_verdict(storage, source, channel).await;
    Some(verdict)
}

/// Computes the update verdict by comparing the actual on-disk DLL digest
/// against the upstream artifact. The DB digest is advisory fallback only.
async fn compute_layer_verdict(
    storage: &impl SharedArtifactRepository,
    source: ReshadeSource,
    channel: ReshadeChannel,
) -> LayerUpdateVerdict {
    // Step 1: Actual DLL digest — the authoritative source of truth.
    let actual_digest = vulkan::current_layer_digest();

    // Step 2: Fetch upstream for comparison.
    let Ok(download) = fetch_reshade_from_source(&source, Architecture::X64, None).await else {
        return LayerUpdateVerdict {
            status: if channel == ReshadeChannel::Nightly {
                UpdateStatus::UnknownNeedsValidation
            } else {
                UpdateStatus::Unknown
            },
            diagnostics: Vec::new(),
        };
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
    use crate::Context;
    use crate::addons::renodx::test_support::{manifest, reshade_sources};
    use crate::addons::renodx::types::{RenoDxConfigKey, RenoDxConfigSetting};
    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::{
        InstalledAddon, PathRef, RenoDxConfigEntry, RenoDxManagedBaseline, RenoDxSetPathBaseline,
        RenoDxSetPathValue,
    };
    use tempfile::tempdir;

    #[tokio::test]
    async fn check_updates_skips_a_record_belonging_to_a_different_addon_kind() {
        let db_dir = tempdir().expect("db dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let luma_record = InstalledAddon::new(
            GameId::new("steam:1091500").expect("id"),
            AddonKind::Luma,
            PathRef::new(r"C:\Games\Test\Luma-Test.addon").expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&luma_record)
            .expect("seed luma record");

        let results = check_updates(&context, &manifest(Vec::new()), &reshade_sources())
            .await
            .expect("check_updates");

        assert!(
            results.is_empty(),
            "a Luma record must never be reported as a RenoDX update result"
        );
    }

    #[test]
    fn other_channel_source_resolves_the_opposite_channel() {
        let reshade_sources = reshade_sources();
        let stable =
            other_channel_source(&reshade_sources, ReshadeChannel::Nightly, Architecture::X64);
        assert!(
            stable.is_some(),
            "stable should be resolvable as the other channel"
        );

        let nightly =
            other_channel_source(&reshade_sources, ReshadeChannel::Stable, Architecture::X64);
        assert!(
            nightly.is_some(),
            "nightly should be resolvable as the other channel"
        );
    }

    #[test]
    fn other_channel_source_returns_none_when_other_channel_unsupported() {
        let mut reshade_sources = reshade_sources();
        reshade_sources.stable = None;
        let other =
            other_channel_source(&reshade_sources, ReshadeChannel::Nightly, Architecture::X64);
        assert!(other.is_none());
    }

    fn receipt_with_entries(entries: Vec<RenoDxConfigEntry>) -> RenoDxConfigReceipt {
        RenoDxConfigReceipt::from_entries(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            true,
            None,
            entries,
        )
    }

    fn entry(key: &str, value: i32) -> RenoDxConfigEntry {
        RenoDxConfigEntry {
            key: key.to_owned(),
            baseline: RenoDxManagedBaseline::Absent,
            last_written: value,
        }
    }

    fn upgrade_config(value: i32) -> RenoDxConfig {
        RenoDxConfig {
            settings: vec![RenoDxConfigSetting {
                key: RenoDxConfigKey::UpgradeR10G10B10A2Unorm,
                value,
            }],
        }
    }

    #[test]
    fn config_only_desired_and_receipt_are_current() {
        let config = upgrade_config(2);
        let receipt = receipt_with_entries(vec![entry("Upgrade_R10G10B10A2_UNORM", 2)]);
        assert_eq!(
            config_status_for_desired(None, Some(&config), Some(&receipt)),
            UpdateStatus::Current
        );
    }

    #[test]
    fn config_only_value_change_is_available() {
        let config = upgrade_config(2);
        let receipt = receipt_with_entries(vec![entry("Upgrade_R10G10B10A2_UNORM", 1)]);
        assert_eq!(
            config_status_for_desired(None, Some(&config), Some(&receipt)),
            UpdateStatus::Available
        );
    }

    #[test]
    fn unchanged_set_path_with_changed_config_is_available() {
        let config = upgrade_config(2);
        let receipt = receipt_with_entries(vec![
            entry("Set_Path", 1),
            entry("Upgrade_R10G10B10A2_UNORM", 1),
        ]);
        assert_eq!(
            config_status_for_desired(Some(RenoDxSetPathValue::One), Some(&config), Some(&receipt)),
            UpdateStatus::Available
        );
    }

    #[test]
    fn exact_set_path_and_config_are_current() {
        let config = upgrade_config(2);
        let receipt = receipt_with_entries(vec![
            entry("Set_Path", 1),
            entry("Upgrade_R10G10B10A2_UNORM", 2),
        ]);
        assert_eq!(
            config_status_for_desired(Some(RenoDxSetPathValue::One), Some(&config), Some(&receipt)),
            UpdateStatus::Current
        );
    }

    #[test]
    fn removing_a_desired_config_key_is_available() {
        let config = None;
        let receipt = receipt_with_entries(vec![
            entry("Set_Path", 1),
            entry("Upgrade_R10G10B10A2_UNORM", 2),
        ]);
        assert_eq!(
            config_status_for_desired(Some(RenoDxSetPathValue::One), config, Some(&receipt)),
            UpdateStatus::Available
        );
    }

    #[test]
    fn schema_v1_set_path_receipt_remains_current() {
        let mut receipt = RenoDxConfigReceipt::new(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            RenoDxSetPathBaseline::Absent,
            false,
            RenoDxSetPathValue::One,
        );
        receipt.schema_version = 1;
        assert_eq!(
            config_status_for_desired(Some(RenoDxSetPathValue::One), None, Some(&receipt)),
            UpdateStatus::Current
        );
    }

    #[test]
    fn inconsistent_or_unsupported_receipt_is_not_current() {
        let mut receipt = RenoDxConfigReceipt::from_entries(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            true,
            None,
            vec![entry("Upgrade_R10G10B10A2_UNORM", 2)],
        );
        receipt.last_written = RenoDxSetPathValue::One;
        assert_eq!(
            config_status_for_desired(None, Some(&upgrade_config(2)), Some(&receipt)),
            UpdateStatus::Available
        );
    }

    #[test]
    fn dlss_fix_validator_fast_path_requires_matching_live_and_recorded_digests() {
        assert_eq!(
            dlss_fix_validator_fast_path("same", "same", Some("etag"), Some("etag")),
            Some(UpdateStatus::Current)
        );
        assert_eq!(
            dlss_fix_validator_fast_path("edited", "same", Some("etag"), Some("etag")),
            None
        );
    }

    #[test]
    fn dlss_fix_validator_fast_path_defers_when_validator_is_not_conclusive() {
        assert_eq!(
            dlss_fix_validator_fast_path("same", "same", Some("old"), Some("new")),
            None
        );
        assert_eq!(
            dlss_fix_validator_fast_path("same", "same", None, Some("etag")),
            None
        );
        assert_eq!(
            dlss_fix_validator_fast_path("same", "same", Some("etag"), None),
            None
        );
    }

    #[test]
    fn bulk_update_candidates_skip_a_record_whose_addon_was_removed() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1091501").expect("game id");
        let addon = game_dir.path().join("renodx-test.addon64");
        let record = InstalledAddon::new(
            game_id,
            AddonKind::RenoDx,
            PathRef::new(addon.to_string_lossy()).expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed renodx record");

        assert_eq!(
            installed_renodx_records(&context).expect("records").count(),
            0
        );

        std::fs::write(addon, b"addon").expect("write addon");
        assert_eq!(
            installed_renodx_records(&context).expect("records").count(),
            1
        );
    }
}
