//! Update detection and application for an installed RenoDX add-on, its ReShade
//! host, and optional DLSS-Fix companion.
//!
//! Both the add-on and the nightly host are rolling upstream snapshots, so "is
//! there an update?" is answered per source by comparing the recorded identity
//! against upstream: a cheap `HEAD`/ETag pre-check first, falling back to a full
//! fetch and SHA-256 compare against the stored digest. ReShade also has a
//! structural host policy layered on top: the active host can require repair or
//! replacement with the full add-on-support build even if that host was not
//! originally installed by RenderPilot.

use std::path::{Path, PathBuf};

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, TrackedSource, TrackedSourceRole,
};

use crate::addons::engine::{self, FileOp, InstallPlan, InstallReceipt};
use crate::addons::update::{combine, digest_verdict, validator_fast_path};
use crate::net::{ProgressObserver, head_validators};
use crate::{Context, ServiceError};

use super::channel;
use super::errors;
use super::facts::{analyze_game, install_target_dir};
use super::host_policy;
use super::matcher::{RenoDxResolution, resolve};
use super::operation_lock;
use super::progress::emit_finalizing;
use super::reshade::ReshadeHostAction;
use super::source;
use super::tracking;
use super::types::{RenoDxManifest, ReshadeChannel};
use super::{fetch, install};

pub use crate::addons::update::UpdateStatus;
use serde::Serialize;

/// A per-source update report for RenoDX, its ReShade host, and the optional
/// DLSS-Fix companion add-on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenoDxUpdateReport {
    /// Update verdict for the add-on payload. `None` if the source is not tracked (e.g., file install).
    pub addon: Option<UpdateStatus>,
    /// Update verdict for the ReShade host. `None` when no safe host verdict can
    /// be derived for this install.
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
    /// * A missing host verdict (`None`) contributes [`UpdateStatus::Current`] —
    ///   some installs have no resolvable automatic ReShade target, and that must
    ///   not force the add-on verdict to unknown.
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
    let host = check_host(context, manifest, record).await;
    let dlss_fix = check_dlss_fix(record).await;
    RenoDxUpdateReport::new(addon, host, dlss_fix)
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

/// Update verdict for the managed ReShade host. The durable comparison is the
/// digest of the extracted DLL for the installed channel; validators are only a
/// fast path when the manifest URL did not change.
async fn check_host(
    context: &Context,
    manifest: &RenoDxManifest,
    record: &InstalledAddon,
) -> Option<UpdateStatus> {
    let host = match channel::single_host_source(record) {
        Ok(source) => source?,
        Err(channel::ChannelReadIssue::DuplicateHostSources) => return Some(UpdateStatus::Unknown),
    };
    let channel = match channel::installed_channel(record) {
        Ok(Some(channel)) => channel,
        Ok(None) | Err(_) => return Some(UpdateStatus::Unknown),
    };
    let target = match resolve_host_update_target(context, manifest, record.game_id(), channel) {
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
    if target.source.url == host.url() {
        if let Ok(validators) = head_validators(host.url(), "ReShade update check").await {
            let current = validators.cache_validator();
            if let Some(status) = validator_fast_path(host.etag(), current.as_deref()) {
                if status == UpdateStatus::Current {
                    return Some(UpdateStatus::Current);
                }
            }
        }
    }
    match fetch::fetch_reshade_from_source(&target.source, target.arch, None).await {
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

/// Applies an update: re-fetches tracked sources, applies the ReShade host policy
/// for the active slot, atomically replaces changed files, and refreshes the
/// record's tracking. If this is the first time RenderPilot replaces the host, the
/// replacement is recorded as a reversible backup in the install record.
pub async fn update(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<(), ServiceError> {
    let _guard = operation_lock::lock(game_id).await;
    let record = context
        .storage()
        .get_installed_addon(game_id)?
        .ok_or_else(|| errors::invalid("RenoDX is not installed for this game".to_owned()))?;

    let addon = source_with_role(&record, TrackedSourceRole::AddonPayload);
    let host = match channel::single_host_source(&record) {
        Ok(host) => host,
        Err(channel::ChannelReadIssue::DuplicateHostSources) => {
            return Err(errors::duplicate_host_sources());
        }
    };
    let dlss_fix = source_with_role(&record, TrackedSourceRole::DlssFix);
    let host_channel =
        match channel::installed_channel(&record).map_err(|_| errors::duplicate_host_sources())? {
            Some(channel) => Some(channel),
            None => host.and_then(|source| channel::infer_legacy_channel_from_url(source.url())),
        };
    let host_target = match host_channel {
        Some(channel) => resolve_host_update_target(context, manifest, game_id, channel)?,
        None => None,
    };
    if let Some(target) = host_target.as_ref() {
        if target.conflict {
            return Err(errors::invalid(
                "ReShade host conflict must be resolved before updating RenoDX".to_owned(),
            ));
        }
    }
    let host_policy_writes = host.is_some()
        && host_target
            .as_ref()
            .is_some_and(|target| target.action.writes_host());

    let addon_tracked = addon.is_some_and(|source| !source.url().is_empty());
    if !addon_tracked && host.is_none() && dlss_fix.is_none() && !host_policy_writes {
        return Err(errors::invalid(
            "this RenoDX install has no recorded source to update from".to_owned(),
        ));
    }

    // Rebuild the tracked-source list with refreshed digests/validators, preserving
    // the install order (add-on first, then ReShade host, then DLSS-Fix).
    let mut refreshed_sources: Vec<TrackedSource> = Vec::new();
    let mut replacements: Vec<Replacement> = Vec::new();
    let mut host_install: Option<HostInstall> = None;

    if let Some(addon) = addon {
        if addon.url().is_empty() {
            refreshed_sources.push(addon.clone());
        } else {
            let prepared = prepare_addon_update(&record, addon, progress).await?;
            refreshed_sources.push(prepared.source);
            replacements.extend(prepared.replacement);
        }
    }

    if let (Some(host), Some(target)) = (host, host_target.as_ref()) {
        let prepared = prepare_policy_host_update(&record, target, host, progress).await?;
        refreshed_sources.push(prepared.source);
        if let Some(replacement) = prepared.replacement {
            match replacement {
                HostReplacement::InPlace(replacement) => replacements.push(replacement),
                HostReplacement::Install(install) => host_install = Some(install),
            }
        }
    } else if let Some(host) = host {
        refreshed_sources.push(host.clone());
    }

    if let Some(dlss_fix) = dlss_fix {
        let prepared = prepare_dlss_fix_update(&record, dlss_fix, progress).await?;
        refreshed_sources.push(prepared.source);
        replacements.extend(prepared.replacement);
    }

    emit_finalizing(progress);
    let originals = apply_replacements(&replacements)?;
    let host_receipt = match host_install.as_ref() {
        Some(install) => match apply_host_install(install) {
            Ok(receipt) => Some(receipt),
            Err(error) => {
                restore_originals_best_effort(&originals);
                return Err(error);
            }
        },
        None => None,
    };
    let refreshed = tracking::rebuild_with_sources_and_receipt(
        &record,
        refreshed_sources,
        host_receipt.as_ref(),
        "RenoDX update rebuild",
    )?;
    if let Err(error) = context.storage().upsert_installed_addon(&refreshed) {
        if let Some(receipt) = &host_receipt {
            if let Err(revert_error) =
                engine::uninstall(&receipt.created_files, &receipt.backed_up_files)
            {
                log::warn!(
                    "RenoDX update: record persistence failed and the ReShade host restore also failed: {revert_error}"
                );
            }
        }
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
    mtime: Option<String>,
}

struct HostInstall {
    game_dir: PathBuf,
    name: String,
    bytes: Vec<u8>,
}

enum HostReplacement {
    InPlace(Replacement),
    Install(HostInstall),
}

struct PreparedHostPolicyUpdate {
    source: TrackedSource,
    replacement: Option<HostReplacement>,
}

struct HostUpdateTarget {
    game_dir: PathBuf,
    slot: String,
    arch: Architecture,
    action: ReshadeHostAction,
    conflict: bool,
    source: source::ReshadeSource,
    channel: ReshadeChannel,
    target_path: PathBuf,
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
            mtime: download.last_modified.clone(),
        }),
    })
}

async fn prepare_policy_host_update(
    record: &InstalledAddon,
    target: &HostUpdateTarget,
    existing_source: &TrackedSource,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedHostPolicyUpdate, ServiceError> {
    let download = fetch::fetch_reshade_from_source(&target.source, target.arch, progress).await?;
    let source = TrackedSource::new(
        TrackedSourceRole::Host,
        target.source.url.clone(),
        download.etag.clone(),
        download.digest.clone(),
    )
    .with_last_modified(download.last_modified.clone())
    .with_channel(target.channel.as_str());

    let changed = download.digest != existing_source.digest() || target.action.writes_host();
    let replacement = if changed {
        match tracking::required_managed_host_path(record) {
            Ok(path) if super::reshade::same_path(&path, &target.target_path) => {
                Some(HostReplacement::InPlace(Replacement {
                    path,
                    bytes: download.bytes,
                    mtime: None,
                }))
            }
            Ok(_) | Err(_) => Some(HostReplacement::Install(HostInstall {
                game_dir: target.game_dir.clone(),
                name: target.slot.clone(),
                bytes: download.bytes,
            })),
        }
    } else {
        None
    };

    Ok(PreparedHostPolicyUpdate {
        source,
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
            mtime: download.last_modified,
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

fn dlss_fix_path(record: &InstalledAddon) -> Result<PathBuf, ServiceError> {
    install::dlss_fix_file_path(record)
        .ok_or_else(|| errors::invalid("no DLSS-Fix add-on in this install".to_owned()))
}

fn resolve_host_update_target(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    channel: ReshadeChannel,
) -> Result<Option<HostUpdateTarget>, ServiceError> {
    let Some(game) = context.storage().find_game(game_id)? else {
        return Ok(None);
    };
    let override_path = crate::nvapi::resolve::stored_override_path(context, game_id.as_str())
        .ok()
        .flatten();
    let analysis = analyze_game(&game, override_path.as_deref());
    let resolution = resolve(manifest, &analysis.facts);
    let (arch, proxy_dll_name) = match resolution {
        RenoDxResolution::Installable(plan) => (plan.arch, plan.proxy_dll_name.clone()),
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => (plan.arch, plan.proxy_dll_name.clone()),
        _ => return Ok(None),
    };
    let game_dir = install_target_dir(&analysis)?;
    let assessment = host_policy::assess(&game_dir, &proxy_dll_name);
    let source = source::reshade_source(&manifest.reshade, channel, arch).ok_or_else(|| {
        errors::invalid(format!(
            "ReShade channel `{}` is not available",
            channel.as_str()
        ))
    })?;
    Ok(Some(HostUpdateTarget {
        game_dir,
        slot: assessment.slot,
        arch,
        action: assessment.action,
        conflict: assessment.conflict,
        source,
        channel,
        target_path: assessment.target_path,
    }))
}

fn apply_host_install(install: &HostInstall) -> Result<InstallReceipt, ServiceError> {
    engine::install(
        &install.game_dir,
        &InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::BackupAndReplace {
                name: install.name.clone(),
                bytes: install.bytes.clone(),
            }],
        },
    )
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
        crate::fs::stamp_mtime_best_effort(&replacement.path, replacement.mtime.as_deref(), None);

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
