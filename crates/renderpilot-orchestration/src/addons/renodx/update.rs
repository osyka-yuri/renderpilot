//! Update detection and application for an installed RenoDX add-on **and** its
//! managed ReShade host.
//!
//! Both the add-on and the nightly host are rolling upstream snapshots, so "is
//! there an update?" is answered per source by comparing the recorded identity
//! against upstream: a cheap `HEAD`/ETag pre-check first, falling back to a full
//! fetch and SHA-256 compare against the stored digest (correct even when the host
//! rotates ETags without a content change). The two verdicts are then combined —
//! an update is available if **either** part changed. A *foreign* ReShade host is
//! never checked or touched (it has no recorded source).

use std::path::{Path, PathBuf};

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{GameId, InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::addons::engine;
use crate::addons::update::{combine, digest_verdict, validator_fast_path};
use crate::net::{DownloadProgress, ProgressObserver, head_validators};
use crate::{Context, ServiceError};

use super::errors;
use super::{
    arch_from_addon_file,
    reshade::{ReshadeState, detect_reshade},
};
use super::{fetch, install};

pub use crate::addons::update::UpdateStatus;
use serde::Serialize;

/// Progress phase emitted after downloads complete while update writes are applied
/// and the persisted record is refreshed.
const FINALIZING_PHASE: &str = "renodx.phase.finalizing";

/// A per-source update report for RenoDX, its managed ReShade host, and the
/// optional DLSS-Fix companion add-on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenoDxUpdateReport {
    /// Update verdict for the add-on payload. `None` if the source is not tracked (e.g., file install).
    pub addon: Option<UpdateStatus>,
    /// Update verdict for the managed ReShade host. `None` if the host is foreign or absent.
    pub host: Option<UpdateStatus>,
    /// Update verdict for the DLSS-Fix companion add-on. `None` if not installed.
    pub dlss_fix: Option<UpdateStatus>,
    /// The combined verdict: available if any tracked source changed, current if
    /// all tracked sources are current.
    pub overall: UpdateStatus,
}

impl RenoDxUpdateReport {
    /// Creates a new update report, automatically combining the verdicts into
    /// [`RenoDxUpdateReport::overall`].
    ///
    /// The combine rule is asymmetric by design:
    /// * A missing add-on source (`None`, e.g. a file install) contributes
    ///   [`UpdateStatus::Unknown`] — there is nothing upstream to compare, so
    ///   "is there an update?" is genuinely unknown.
    /// * A missing host source (`None`, e.g. a foreign/absent ReShade host)
    ///   contributes [`UpdateStatus::Current`] — a foreign host is never
    ///   updated by RenderPilot, so it is effectively "current" from this
    ///   tool's perspective and must not force the overall verdict to unknown.
    /// * A missing DLSS-Fix source (`None`, e.g. not installed) contributes
    ///   [`UpdateStatus::Current`] — like a foreign host, an absent companion
    ///   must not force the overall verdict to unknown.
    #[must_use]
    pub fn new(
        addon: Option<UpdateStatus>,
        host: Option<UpdateStatus>,
        dlss_fix: Option<UpdateStatus>,
    ) -> Self {
        let overall = combine(
            addon.unwrap_or(UpdateStatus::Unknown),
            combine(
                host.unwrap_or(UpdateStatus::Current),
                dlss_fix.unwrap_or(UpdateStatus::Current),
            ),
        );
        Self {
            addon,
            host,
            dlss_fix,
            overall,
        }
    }
}

/// Returns the tracked source with the given role, if the install recorded one.
fn source_with_role(record: &InstalledAddon, role: TrackedSourceRole) -> Option<&TrackedSource> {
    record
        .tracked_sources()
        .iter()
        .find(|source| source.role() == role)
}

/// A cosmetic fetch/log label for the add-on (the file name identifies the title).
fn addon_label(record: &InstalledAddon) -> &str {
    Path::new(record.addon_file().as_str())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("RenoDX add-on")
}

/// Checks whether the installed add-on for `game_id` has an upstream update.
pub async fn check_update(
    context: &Context,
    game_id: &GameId,
) -> Result<RenoDxUpdateReport, ServiceError> {
    match context.storage().get_installed_addon(game_id)? {
        Some(record) => Ok(check_record(&record).await),
        None => Ok(RenoDxUpdateReport::new(None, None, None)),
    }
}

/// Bulk update check over every installed RenoDX add-on.
pub async fn check_updates(context: &Context) -> Result<Vec<(GameId, UpdateStatus)>, ServiceError> {
    let records = context.storage().list_installed_addons()?;
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let report = check_record(&record).await;
        out.push((record.game_id().clone(), report.overall));
    }
    Ok(out)
}

async fn check_record(record: &InstalledAddon) -> RenoDxUpdateReport {
    let addon = check_addon(record).await;
    let host = check_host(record).await;
    let dlss_fix = check_dlss_fix(record).await;
    RenoDxUpdateReport::new(addon, host, dlss_fix)
}

/// Update verdict for the add-on payload. A file install records no add-on source,
/// so there is nothing upstream to compare — it contributes `None`.
async fn check_addon(record: &InstalledAddon) -> Option<UpdateStatus> {
    let addon = source_with_role(record, TrackedSourceRole::AddonPayload)?;
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

/// Update verdict for the **managed** ReShade host. A foreign/absent host records no
/// source, so there is nothing to update — it contributes `None`.
async fn check_host(record: &InstalledAddon) -> Option<UpdateStatus> {
    let host = source_with_role(record, TrackedSourceRole::Host)?;
    let Some(arch) = arch_from_addon_file(record.addon_file().as_str()) else {
        return Some(UpdateStatus::Unknown);
    };
    if let Ok(validators) = head_validators(host.url(), "ReShade update check").await {
        let current = validators.cache_validator();
        if let Some(status) = validator_fast_path(host.etag(), current.as_deref()) {
            return Some(status);
        }
    }
    match fetch::fetch_reshade_from_url(host.url(), arch, None).await {
        Ok(download) => Some(digest_verdict(host.digest(), &download.digest)),
        Err(_) => Some(UpdateStatus::Unknown),
    }
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

/// Applies an update: re-fetches the add-on and the managed ReShade host, atomically
/// replaces whichever one's content changed, and refreshes the record's tracking.
/// A foreign host (no recorded source) is left untouched.
pub async fn update(
    context: &Context,
    game_id: &GameId,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<(), ServiceError> {
    let record = context
        .storage()
        .get_installed_addon(game_id)?
        .ok_or_else(|| errors::invalid("RenoDX is not installed for this game".to_owned()))?;

    let addon = source_with_role(&record, TrackedSourceRole::AddonPayload);
    let host = source_with_role(&record, TrackedSourceRole::Host);
    let dlss_fix = source_with_role(&record, TrackedSourceRole::DlssFix);
    if addon.is_none() && host.is_none() && dlss_fix.is_none() {
        return Err(errors::invalid(
            "this RenoDX install has no recorded source to update from".to_owned(),
        ));
    }

    // Rebuild the tracked-source list with refreshed digests/validators, preserving
    // the install order (add-on first, then the managed host, then DLSS-Fix).
    let mut refreshed_sources: Vec<TrackedSource> = Vec::new();
    let mut replacements: Vec<Replacement> = Vec::new();

    if let Some(addon) = addon {
        let prepared = prepare_addon_update(&record, addon, progress).await?;
        refreshed_sources.push(prepared.source);
        replacements.extend(prepared.replacement);
    }

    if let Some(host) = host {
        let prepared = prepare_host_update(&record, host, progress).await?;
        refreshed_sources.push(prepared.source);
        replacements.extend(prepared.replacement);
    }

    if let Some(dlss_fix) = dlss_fix {
        let prepared = prepare_dlss_fix_update(&record, dlss_fix, progress).await?;
        refreshed_sources.push(prepared.source);
        replacements.extend(prepared.replacement);
    }

    emit_finalizing(progress);
    let originals = apply_replacements(&replacements)?;
    let refreshed = record.with_tracked_sources(refreshed_sources);
    if let Err(error) = context.storage().upsert_installed_addon(&refreshed) {
        if let Err(revert_error) = restore_originals(&originals) {
            log::warn!(
                "RenoDX update: record persistence failed and the filesystem restore also failed: {revert_error}"
            );
        }
        return Err(error.into());
    }
    Ok(())
}

struct PreparedSourceUpdate {
    source: TrackedSource,
    replacement: Option<Replacement>,
}

struct Replacement {
    path: PathBuf,
    bytes: Vec<u8>,
}

struct OriginalFile {
    path: PathBuf,
    bytes: Vec<u8>,
}

async fn prepare_addon_update(
    record: &InstalledAddon,
    source: &TrackedSource,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedSourceUpdate, ServiceError> {
    let download = fetch::fetch_addon(source.url(), addon_label(record), progress).await?;
    let changed = download.digest != source.digest();
    let refreshed = refreshed_source(source, &download);
    Ok(PreparedSourceUpdate {
        source: refreshed,
        replacement: changed.then(|| Replacement {
            path: addon_path(record),
            bytes: download.bytes,
        }),
    })
}

async fn prepare_host_update(
    record: &InstalledAddon,
    source: &TrackedSource,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedSourceUpdate, ServiceError> {
    let Some(arch) = arch_from_addon_file(record.addon_file().as_str()) else {
        return Ok(PreparedSourceUpdate {
            source: source.clone(),
            replacement: None,
        });
    };

    let download = fetch::fetch_reshade_from_url(source.url(), arch, progress).await?;
    let refreshed = refreshed_source(source, &download);
    let replacement = if download.digest == source.digest() {
        None
    } else {
        Some(Replacement {
            path: managed_host_path(record)?,
            bytes: download.bytes,
        })
    };
    Ok(PreparedSourceUpdate {
        source: refreshed,
        replacement,
    })
}

async fn prepare_dlss_fix_update(
    record: &InstalledAddon,
    source: &TrackedSource,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedSourceUpdate, ServiceError> {
    let download = fetch::fetch_addon(source.url(), "DLSS-Fix", progress).await?;
    let refreshed = refreshed_source(source, &download);
    let replacement = if download.digest == source.digest() {
        None
    } else {
        Some(Replacement {
            path: dlss_fix_path(record)?,
            bytes: download.bytes,
        })
    };
    Ok(PreparedSourceUpdate {
        source: refreshed,
        replacement,
    })
}

fn refreshed_source(source: &TrackedSource, download: &fetch::Download) -> TrackedSource {
    TrackedSource::new(
        source.role(),
        source.url().to_owned(),
        download.etag.clone(),
        download.digest.clone(),
    )
    .with_last_modified(download.last_modified.clone())
}

fn addon_path(record: &InstalledAddon) -> PathBuf {
    PathBuf::from(record.addon_file().as_str())
}

fn managed_host_path(record: &InstalledAddon) -> Result<PathBuf, ServiceError> {
    let game_dir = Path::new(record.addon_file().as_str())
        .parent()
        .ok_or_else(|| errors::invalid("installed add-on has no parent directory".to_owned()))?;
    let ReshadeState::Managed(marker) = detect_reshade(game_dir) else {
        return Err(errors::invalid(
            "RenoDX does not manage the ReShade host for this game".to_owned(),
        ));
    };
    Ok(game_dir.join(&marker.proxy_dll))
}

fn dlss_fix_path(record: &InstalledAddon) -> Result<PathBuf, ServiceError> {
    install::dlss_fix_file_path(record)
        .ok_or_else(|| errors::invalid("no DLSS-Fix add-on in this install".to_owned()))
}

fn apply_replacements(replacements: &[Replacement]) -> Result<Vec<OriginalFile>, ServiceError> {
    let mut originals = Vec::with_capacity(replacements.len());

    for replacement in replacements {
        let original = match crate::fs::read_file(&replacement.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                restore_originals_best_effort(&originals);
                return Err(error);
            }
        };

        if let Err(error) = engine::replace_file(&replacement.path, &replacement.bytes) {
            restore_originals_best_effort(&originals);
            return Err(error);
        }

        originals.push(OriginalFile {
            path: replacement.path.clone(),
            bytes: original,
        });
    }

    Ok(originals)
}

fn restore_originals(originals: &[OriginalFile]) -> Result<(), ServiceError> {
    let failures = restore_originals_inner(originals);
    if failures == 0 {
        Ok(())
    } else {
        Err(errors::failed(format!(
            "failed to restore {failures} updated RenoDX file(s)"
        )))
    }
}

fn restore_originals_best_effort(originals: &[OriginalFile]) {
    let failures = restore_originals_inner(originals);
    if failures > 0 {
        log::warn!("RenoDX update rollback failed to restore {failures} file(s)");
    }
}

fn restore_originals_inner(originals: &[OriginalFile]) -> usize {
    let mut failures = 0;
    for original in originals.iter().rev() {
        if let Err(error) = engine::replace_file(&original.path, &original.bytes) {
            log::warn!(
                "RenoDX update rollback: failed to restore `{}`: {error}",
                original.path.display()
            );
            failures += 1;
        }
    }
    failures
}

fn emit_finalizing(progress: Option<&ProgressObserver<'_>>) {
    if let Some(observe) = progress {
        observe(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: 0,
            phase: Some(FINALIZING_PHASE),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::update::UpdateStatus::{Available, Current, Unknown};

    #[test]
    fn report_assembly_managed_tracked() {
        let report = RenoDxUpdateReport::new(Some(Current), Some(Available), None);
        assert_eq!(report.overall, Available);

        let report = RenoDxUpdateReport::new(Some(Current), Some(Current), None);
        assert_eq!(report.overall, Current);
    }

    #[test]
    fn report_assembly_foreign_host() {
        // Host is None, meaning foreign or absent. Falls back to Current.
        let report = RenoDxUpdateReport::new(Some(Current), None, None);
        assert_eq!(report.overall, Current);

        let report = RenoDxUpdateReport::new(Some(Available), None, None);
        assert_eq!(report.overall, Available);
    }

    #[test]
    fn report_assembly_file_install() {
        // Addon is None, meaning file install (not tracked). Falls back to Unknown.
        let report = RenoDxUpdateReport::new(None, Some(Current), None);
        // combine(Unknown, Current) -> Unknown
        assert_eq!(report.overall, Unknown);
    }

    #[test]
    fn report_assembly_with_dlss_fix() {
        // DLSS-Fix has an update available → overall is available.
        let report = RenoDxUpdateReport::new(Some(Current), Some(Current), Some(Available));
        assert_eq!(report.overall, Available);

        // All current → overall is current.
        let report = RenoDxUpdateReport::new(Some(Current), Some(Current), Some(Current));
        assert_eq!(report.overall, Current);

        // No dlss-fix (None) → falls back to Current, doesn't affect overall.
        let report = RenoDxUpdateReport::new(Some(Current), Some(Current), None);
        assert_eq!(report.overall, Current);
    }
}
