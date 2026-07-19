use renderpilot_domain::{AddonKind, InstalledAddon, TrackedSourceRole};

use crate::Context;
use crate::addons::engine;
use crate::addons::luma::dgvoodoo;
use crate::addons::luma::tracking;
use crate::addons::luma::types::LumaManifest;
use crate::addons::luma::use_cases::update_target::{
    self, ResolvedUpdateTarget, host_status_from_digests, host_status_when_validators_match,
};
use crate::addons::luma::{fetch, source};
use crate::addons::records::source_with_role;
use crate::addons::reshade::fetch::{fetch_reshade_from_source, sha256_hex};
use crate::addons::reshade::host_policy::HostLifecycle;
use crate::addons::reshade::source::require_reshade_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::addons::update::{UpdateStatus, digest_verdict, validator_fast_path};
use crate::net::{head_validators, head_with_url_chain};

/// When a crash-safety sentinel is present on the resolved game dir, force the
/// addon source to `Available` so the UI can offer Update. Host/dgVoodoo
/// checks still run independently. If the target is unresolved, leave the
/// addon verdict unchanged (avoid false Available on an unknown path).
pub(super) fn elevate_addon_if_torn(
    addon: Option<UpdateStatus>,
    target: Option<&ResolvedUpdateTarget>,
) -> Option<UpdateStatus> {
    let Some(target) = target else {
        return addon;
    };
    if engine::is_install_torn(&target.game_dir, AddonKind::Luma) {
        Some(UpdateStatus::Available)
    } else {
        addon
    }
}

/// Update verdict for the release asset. Normal installs use the ZIP digest;
/// DB-loss recovery uses an advisory digest of the exact root add-on plus the
/// `Luma/**` tree. Full ZIP work runs when `deep` (callers may elevate passive
/// probes for unbound advisory payloads).
pub(super) async fn check_addon(
    context: &Context,
    record: &InstalledAddon,
    target: Option<&ResolvedUpdateTarget>,
    deep: bool,
) -> Option<UpdateStatus> {
    let addon = source_with_role(record, TrackedSourceRole::AddonPayload)?;
    if addon.url().is_empty() {
        return None;
    }

    // Missing on-disk payload (main `.addon` or any other tracked payload path)
    // is always actionable for repair/update without waiting on ETag / ZIP
    // identity that still matches the record.
    if !tracking::payload_disk_intact(record) {
        return Some(UpdateStatus::Available);
    }

    if addon.is_advisory() {
        return check_advisory_addon(context, record, addon, target, deep).await;
    }

    // A single HEAD serves both cheap tiers: validators for the ETag fast path,
    // and the hop chain for build-number comparison (CDN final URLs strip
    // `latest-<n>` — scan the full chain).
    if let Ok((validators, url_chain)) = head_with_url_chain(addon.url(), "Luma update check").await
    {
        let current = validators.cache_validator();
        if let Some(status) = validator_fast_path(addon.etag(), current.as_deref()) {
            return Some(status);
        }
        // Build-number mismatch is a cheap Available signal. Matching build alone
        // is NOT Current without ETag confirmation -- fall through to ZIP digest
        // so re-pointed tags / ETag churn cannot hide a real payload change.
        if let Some(current_build) = source::parse_build_number_from_chain(&url_chain)
            && let Some(recorded_label) = record.addon_version()
            && source::build_label(current_build) != recorded_label
        {
            return Some(UpdateStatus::Available);
        }
    }

    // Passive probes never full-download the release ZIP on ETag miss -- that can
    // be multi-MB per details-page load. Explicit deep checks still compare
    // digests against a fresh download. Advisory ZIP work is isolated in
    // `check_advisory_addon` (deep only).
    if !deep {
        return Some(UpdateStatus::Unknown);
    }

    let target = target?;
    match fetch::download::fetch_luma_payload(&target.asset, &target.addon_file, target.arch, None)
        .await
    {
        Ok(payload) => {
            let status = addon_payload_verdict(addon, &payload);
            if status == UpdateStatus::Current {
                // Heal rotated CDN validators so the next passive open is cheap.
                tracking::try_refresh_payload_validators(
                    context,
                    record.game_id(),
                    addon.digest(),
                    &payload,
                )
                .await;
            }
            Some(status)
        }
        Err(_) => Some(UpdateStatus::Unknown),
    }
}

/// Advisory provenance: passive = HEAD/build + sticky bind-mark; deep = full ZIP
/// (promote on match, mark on Available so the next passive probe stays cheap).
async fn check_advisory_addon(
    context: &Context,
    record: &InstalledAddon,
    addon: &renderpilot_domain::TrackedSource,
    target: Option<&ResolvedUpdateTarget>,
    deep: bool,
) -> Option<UpdateStatus> {
    let mut matching_build = false;

    // Cheap HEAD/build-number gate shared by passive and deep (before any ZIP).
    if let Ok((_, url_chain)) = head_with_url_chain(addon.url(), "Luma update check").await
        && let Some(current_build) = source::parse_build_number_from_chain(&url_chain)
    {
        let current_label = source::build_label(current_build);
        if let Some(recorded_label) = record.addon_version() {
            if current_label == recorded_label {
                matching_build = true;
            } else {
                return Some(UpdateStatus::Available);
            }
        }
    }

    if !deep {
        return Some(passive_advisory_status(addon, matching_build));
    }

    let target = target?;
    match fetch::download::fetch_luma_payload(&target.asset, &target.addon_file, target.arch, None)
        .await
    {
        Ok(payload) => {
            let status = addon_payload_verdict(addon, &payload);
            persist_advisory_check_outcome(context, record, addon, &payload, status).await;
            Some(status)
        }
        Err(_) => Some(UpdateStatus::Unknown),
    }
}

/// Soft passive verdict before a ZIP bind. A prior deep Available mark sticks as
/// Available even when the build label still matches (disk still differs).
fn passive_advisory_status(
    addon: &renderpilot_domain::TrackedSource,
    matching_build: bool,
) -> UpdateStatus {
    if tracking::source_has_bind_mark(addon) {
        return UpdateStatus::Available;
    }
    if matching_build {
        UpdateStatus::Current
    } else {
        UpdateStatus::Unknown
    }
}

async fn persist_advisory_check_outcome(
    context: &Context,
    record: &InstalledAddon,
    addon: &renderpilot_domain::TrackedSource,
    payload: &fetch::types::LumaPayload,
    status: UpdateStatus,
) {
    match status {
        UpdateStatus::Current => {
            tracking::try_promote_advisory_payload(
                context,
                record.game_id(),
                addon.digest(),
                payload,
                |source, payload| addon_payload_verdict(source, payload) == UpdateStatus::Current,
            )
            .await;
        }
        UpdateStatus::Available => {
            tracking::try_mark_advisory_payload_checked(
                context,
                record.game_id(),
                addon.digest(),
                payload,
            )
            .await;
        }
        _ => {}
    }
}

pub(super) fn addon_payload_verdict(
    source: &renderpilot_domain::TrackedSource,
    payload: &fetch::types::LumaPayload,
) -> UpdateStatus {
    let observed_digest = if source.is_advisory() {
        fetch::digest::recovery_payload_digest(payload)
    } else {
        payload.zip_digest.clone()
    };
    digest_verdict(source.digest(), &observed_digest)
}

pub(super) fn check_dgvoodoo(
    record: &InstalledAddon,
    target: Option<&ResolvedUpdateTarget>,
) -> Option<UpdateStatus> {
    let target = target?;
    let requirement = dgvoodoo::requirement(target.external_requirement.as_ref())?;
    // Soft manage gate: wrapper source + owned subset, no foreign map dest.
    // Full ownership is not required so catalogue map growth still surfaces.
    if !dgvoodoo::record_can_manage_runtime(record, &target.game_dir, requirement) {
        // Partial ownership blocked by a foreign map dest -> Unknown (uncertain).
        // Source-only or fully unowned -> None (not a managed stack we report).
        let blocked_partial = source_with_role(record, TrackedSourceRole::DgVoodooWrapper)
            .is_some()
            && dgvoodoo::record_owns_any_map_dest(record, &target.game_dir, requirement);
        return blocked_partial.then_some(UpdateStatus::Unknown);
    }
    if dgvoodoo::map_needs_ownership_sync(record, &target.game_dir, requirement) {
        return Some(UpdateStatus::Available);
    }
    Some(
        match dgvoodoo::owned_status(&target.game_dir, requirement) {
            dgvoodoo::OwnedDgVoodooStatus::Current => UpdateStatus::Current,
            dgvoodoo::OwnedDgVoodooStatus::Outdated | dgvoodoo::OwnedDgVoodooStatus::Incomplete => {
                UpdateStatus::Available
            }
            dgvoodoo::OwnedDgVoodooStatus::Unknown => UpdateStatus::Unknown,
        },
    )
}

/// Update verdict for Luma's owned nightly ReShade host. The current DLL is
/// compared directly with a freshly validated nightly download: an advisory
/// source is enough to explain the nightly origin, but never substitutes for
/// checking the bytes that are actually on disk.
pub(super) async fn check_host(
    record: &InstalledAddon,
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
    target: Option<&ResolvedUpdateTarget>,
    deep: bool,
) -> Option<UpdateStatus> {
    let target = target?;
    let host_path = target.game_dir.join(&target.proxy_dll_name);
    if !tracking::owns_path(record, &host_path) {
        return None;
    }
    // Owned but missing on disk: always actionable for repair/update without
    // waiting on a network round-trip to decide.
    if !host_path.is_file() {
        return Some(UpdateStatus::Available);
    }
    let min_version = match manifest.min_reshade_version_parsed() {
        Ok(version) => version,
        Err(_) => return Some(UpdateStatus::Unknown),
    };
    let assessment = match update_target::assess_host_for_update(target, &min_version) {
        Some(assessment) => assessment,
        // The only `None` path is a known custom build. The proxy still belongs
        // to this record, but its current bytes are no longer safe to compare
        // against or replace with Luma's nightly.
        None => return Some(UpdateStatus::Unknown),
    };
    if assessment.conflict {
        return Some(UpdateStatus::Unknown);
    }

    let current = match std::fs::read(&host_path) {
        Ok(bytes) => sha256_hex(&bytes),
        Err(_) => return Some(UpdateStatus::Unknown),
    };
    let source = match require_reshade_source(reshade_sources, ReshadeChannel::Nightly, target.arch)
    {
        Ok(source) => source,
        Err(_) => return Some(UpdateStatus::Unknown),
    };

    // Cheap HEAD/ETag path before downloading the full nightly archive.
    // Mirrors RenoDX: only trust validators when the recorded URL still matches
    // the required nightly source (channel/arch identity).
    if let Some(host_source) = source_with_role(record, TrackedSourceRole::HostBinary)
        && host_source.url() == source.url
        && let Ok(validators) = head_validators(host_source.url(), "ReShade update check").await
    {
        let current_validator = validators.cache_validator();
        if let Some(status) = validator_fast_path(host_source.etag(), current_validator.as_deref())
            && status == UpdateStatus::Current
        {
            return Some(host_status_when_validators_match(assessment.lifecycle));
        }
    }

    // Passive probes never full-download the nightly archive on ETag miss --
    // that can be multi-hundred MB per details-page load. Prefer a local
    // integrity check against the install-time host digest so game-details
    // does not flash "couldn't check" on every open:
    // - disk matches record -> same lifecycle policy as a matching ETag
    // - disk differs from record -> Available (reconverge without a download)
    // Explicit deep checks still re-verify against a fresh upstream download.
    if !deep {
        if let Some(host_source) = source_with_role(record, TrackedSourceRole::HostBinary) {
            let recorded = host_source.digest();
            if !recorded.is_empty() {
                if recorded == current {
                    return Some(host_status_when_validators_match(assessment.lifecycle));
                }
                return Some(UpdateStatus::Available);
            }
        }
        return Some(UpdateStatus::Unknown);
    }

    let download = match fetch_reshade_from_source(&source, target.arch, None).await {
        Ok(download) => download,
        Err(_) => return Some(UpdateStatus::Unknown),
    };

    let status = host_status_from_digests(assessment.lifecycle, &current, &download.digest);
    if status == UpdateStatus::Unknown && matches!(assessment.lifecycle, HostLifecycle::ReuseUser) {
        log::info!(
            "Luma host update skipped for `{}`: owned host has user ReShade content",
            record.game_id()
        );
    }
    Some(status)
}
