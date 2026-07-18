//! Turns a fetched Luma install into a generic [`InstallPlan`] for the shared
//! [`addons::engine`](crate::addons::engine) and maps the result back to a record.
//!
//! Unlike RenoDX's single opaque add-on file, a Luma install lays down a whole
//! tree (the root `.addon` and the `Luma/` shader tree) through the generic
//! engine. The optional `nvngx_dlss.dll` is deliberately extracted from that
//! plan and handled by the version-aware managed-file policy, so a classic
//! `.bak` remains the immutable pre-first-overlay baseline. The proxy ReShade
//! host, when this install must write one, uses a no-backup [`FileOp::Replace`]
//! (an official redistributable RenderPilot already PE-checked itself). Luma
//! never touches `ReShade.ini` -- its manifest's ini tweaks are always empty, so
//! no ini op is ever emitted and uninstall never has to reason about one.
//!
//! All Luma-specific layout lives here; the filesystem mechanics are the
//! engine's, and the ReShade host detection/decision is the shared
//! [`crate::addons::reshade`] subsystem -- this module never imports from
//! [`crate::addons::renodx`].

use std::path::Path;

use renderpilot_domain::{AddonKind, GameId, InstalledAddon};

use crate::ServiceError;
use crate::addons::engine;
use crate::addons::path_bufs;
use crate::addons::reshade::host_policy;
use crate::addons::reshade::scan;
use crate::addons::reshade::split_install::{InstallRoots, PayloadRollback, run_split_install};

use super::dgvoodoo::DgVoodooInstall;
use super::errors;
use super::fetch::types::LumaPayloadFile;

pub(crate) mod plan;
mod record;
mod recovery;

#[cfg(test)]
mod tests;

pub(crate) use record::build_record;

/// Everything the engine needs to lay down an install, with payloads already
/// downloaded and integrity-verified by the fetch layer.
#[derive(Debug, Clone)]
pub(crate) struct PreparedInstall {
    /// Game the install belongs to.
    pub(crate) game_id: GameId,
    /// Proxy DLL file name to install the ReShade host as (e.g. `dxgi.dll`).
    pub(crate) proxy_dll_name: String,
    /// Every payload file to lay down, including the main `.addon` and, when
    /// present, `nvngx_dlss.dll`.
    pub(crate) payload: Vec<LumaPayloadFile>,
    /// Relative path of the main `.addon` within `payload`.
    pub(crate) main_addon_rel: String,
    /// Upstream URL the release asset was fetched from.
    pub(crate) asset_source_url: String,
    /// SHA-256 of the raw ZIP bytes -- the durable change-detection digest.
    pub(crate) zip_digest: String,
    /// HTTP cache validator for a cheap update pre-check.
    pub(crate) source_etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string of the asset, when the host sent one.
    pub(crate) source_last_modified: Option<String>,
    /// UI-facing build label ("Build 515"), when the redirect tag could be parsed.
    pub(crate) build_label: Option<String>,
    /// ReShade host DLL bytes; used only when no compatible host is already present.
    pub(crate) reshade_dll_bytes: Vec<u8>,
    /// Upstream URL the ReShade host came from (empty when none is installed).
    pub(crate) reshade_source_url: String,
    /// The ReShade host zip's cache validator, for a cheap host update pre-check.
    pub(crate) reshade_source_etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string of the ReShade host zip, when sent.
    pub(crate) reshade_last_modified: Option<String>,
    /// SHA-256 of the installed ReShade host DLL (empty when none installed).
    pub(crate) reshade_digest: String,
    /// Managed dgVoodoo dependency, when this Luma profile needs one.
    pub(crate) dgvoodoo: Option<DgVoodooInstall>,
}

/// Installs Luma into `game_dir`, returning the record and an open commit guard.
///
/// The crash-safety sentinel stays open until the caller finishes
/// [`engine::PendingInstallCommit`] after durable DB persist (or after a complete FS
/// revert). Dropping the commit without finishing leaves a torn marker.
///
/// Refuses if a Luma install record already exists here (the caller should
/// uninstall first to reinstall -- enforced by the resolved-directory presence
/// check upstream in the command layer, not here). A missing ReShade host is
/// installed; a compatible empty host is adopted, while a recognised empty
/// ordinary/old host is repaired into the Add-on build Luma requires. User
/// content remains untouched.
pub(crate) fn install(
    context: &crate::Context,
    game_dir: &Path,
    mut prepared: PreparedInstall,
    min_host_version: &renderpilot_domain::Version,
) -> Result<(InstalledAddon, engine::PendingInstallCommit), ServiceError> {
    let host = host_policy::assess_for_tool(
        game_dir,
        &prepared.proxy_dll_name,
        "Luma",
        Some(min_host_version),
    );
    host.ensure_initial_installable(&prepared.proxy_dll_name)?;
    if host.initial_writes_host() && prepared.reshade_dll_bytes.is_empty() {
        return Err(errors::invalid(
            "the active ReShade host needs installation or repair, but no ReShade bytes were provided"
                .to_owned(),
        ));
    }

    let paths = scan::resolve_paths(game_dir, Some(&host.target_path));
    if !paths.effective_addon_path.is_dir() {
        return Err(errors::invalid(format!(
            "ReShade AddonPath `{}` does not exist",
            paths.effective_addon_path.display()
        )));
    }
    let mut adopted_existing = host.initial_owned_existing_paths(paths.ini_path.as_deref());
    adopted_existing.extend(prepared.adopted_dgvoodoo_paths());
    let adopted_host_path = (host.lifecycle == host_policy::HostLifecycle::AdoptEmpty)
        .then_some(host.target_path.as_path());

    let roots = InstallRoots::resolve(game_dir, &host.target_path);
    // Plan DLSS while payload is borrowed; then move payload/host bytes into ops.
    let dlss = super::dlss::plan_install(
        context,
        &prepared.game_id,
        &paths.effective_addon_path,
        &prepared.payload,
    )?;
    let payload_ops = plan::payload_ops(std::mem::take(&mut prepared.payload));
    let game_dir_ops = plan::game_dir_ops(&mut prepared, &host);
    let success_result = if roots.is_unified {
        let mut unified_ops = payload_ops;
        unified_ops.extend(game_dir_ops);
        run_split_install(
            &roots,
            AddonKind::Luma,
            unified_ops,
            Vec::new(),
            Vec::new(),
            PayloadRollback::Tree,
        )
    } else {
        run_split_install(
            &roots,
            AddonKind::Luma,
            Vec::new(),
            payload_ops,
            game_dir_ops,
            PayloadRollback::Tree,
        )
    };
    let success = success_result?;
    dlss.execute()?;
    let record = build_record(
        &prepared,
        game_dir,
        &paths.effective_addon_path,
        record::RecordInstallResult {
            tracks_host: host.initial_writes_host(),
            adopted_host_path,
            adopted_existing: &adopted_existing,
            receipt: &success.receipt,
            managed_file: dlss.binding,
        },
    )?;
    Ok((record, success.commit))
}

impl PreparedInstall {
    fn reused_dgvoodoo_config_file(&self) -> Option<&str> {
        match self.dgvoodoo.as_ref() {
            Some(DgVoodooInstall::Reused(reused)) => Some(&reused.config_file),
            Some(DgVoodooInstall::Managed(_) | DgVoodooInstall::Adopted(_)) | None => None,
        }
    }

    fn adopted_dgvoodoo_paths(&self) -> Vec<std::path::PathBuf> {
        match self.dgvoodoo.as_ref() {
            Some(DgVoodooInstall::Adopted(adopted)) => adopted.existing_paths.clone(),
            Some(DgVoodooInstall::Managed(_) | DgVoodooInstall::Reused(_)) | None => Vec::new(),
        }
    }
}

/// Removes only the generic add-on-engine payload. Compound uninstall uses this
/// after coordinated files and intersecting catalog components were unwound.
///
/// Delegates to [`engine::uninstall_tree`] (the generic list-based reversal plus
/// best-effort empty-directory cleanup, bounded to the add-on's own directory) --
/// Luma never writes `ReShade.ini`, but an empty pre-existing config can be
/// recorded as part of an adopted runtime and removed with it. Whenever this
/// record owns the ReShade host, its `ReShade.log`/rotated logs are removed too.
pub(crate) fn uninstall_engine_files(record: &InstalledAddon) -> Result<(), ServiceError> {
    let boundary = Path::new(record.addon_file().as_str())
        .parent()
        .ok_or_else(|| {
            errors::failed("Luma install record's add-on file has no parent directory".to_owned())
        })?;

    let log_base_path =
        crate::addons::tracking::owned_proxy_host_path(record).and_then(|host_path| {
            host_path
                .parent()
                .map(|dir| scan::resolve_paths(dir, Some(&host_path)).effective_base_path)
        });

    engine::uninstall_tree(
        &path_bufs(record.created_files()),
        &path_bufs(record.backed_up_files()),
        boundary,
    )?;

    // Defense in depth: if this install recorded host provenance / a channel,
    // remove any leftover known owned proxy even when it was missing from
    // `created_files` (legacy records).
    // Never touch a path that is not a ReShade PE.
    remove_owned_reshade_host_best_effort(record);

    if let Some(base_path) = log_base_path {
        scan::remove_reshade_logs_best_effort(&base_path);
    }
    Ok(())
}

/// Removes a Luma-owned ReShade proxy that may have been left on disk after the
/// primary created_files pass (e.g. older records that stamped provenance
/// without listing the host path).
fn remove_owned_reshade_host_best_effort(record: &InstalledAddon) {
    if !record.has_host_binary_provenance() && record.reshade_channel().is_none() {
        return;
    }
    let candidates = [
        crate::addons::tracking::owned_proxy_host_path(record),
        crate::addons::tracking::host_proxy_path(record),
    ];
    for path in candidates.into_iter().flatten() {
        if path.is_file()
            && scan::is_reshade_proxy_file(&path)
            && let Err(error) = std::fs::remove_file(&path)
        {
            log::warn!(
                "Luma uninstall: failed to remove owned ReShade host `{}`: {error}",
                path.display()
            );
        }
    }
}

/// Recovers from a torn Luma install -- a crash mid-install or mid-rollback
/// (see [`engine::is_install_torn`]) that left tool-owned debris behind with
/// no database record to reverse it. Called from the install command, under
/// the per-game `game_mutation_lock`, only after confirming no Luma record exists
/// and no *other* tool is blocking this game -- so any debris found here is
/// unambiguously abandoned, never a live install.
///
/// **Only runs when a torn sentinel is present** (via install_guard). Do not
/// call this to clean arbitrary debris without that gate.
///
/// Removes top-level `Luma-*.addon*` files and the entire `Luma/` tree
/// (removed recursively -- blast radius is intentional for crash debris only).
/// Shadowed game files are restored from allowlisted `{name}.bak` siblings
/// (`nvngx_dlss.dll`, managed dgVoodoo wrappers/configs, etc.) -- never deleted
/// without a bak. Restoration runs on **every** scan root (game dir and split
/// AddonPath payload dir). Proxy hosts are deliberately left to the normal
/// host-policy adoption/conflict checks because the sentinel alone cannot prove
/// whether a ReShade DLL was written by the failed operation or by the user.
///
/// Best-effort: a removal failure is logged and left for the caller's
/// existing unmanaged-files check to catch -- this only widens the happy path
/// (a clean folder installs immediately instead of requiring a manual
/// cleanup), it never weakens the safety net. The crash-safety sentinel is
/// cleared only once the folder is confirmed clean, so a partial recovery
/// still reports as torn on the next scan.
pub(crate) fn recover_torn_install(scan_dirs: &[&Path]) {
    recovery::recover_torn_install(scan_dirs);
}
