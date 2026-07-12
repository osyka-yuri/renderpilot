use std::path::{Path, PathBuf};

use renderpilot_domain::{AddonKind, InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::ServiceError;
use crate::addons::engine::{self, InstallPlan};
use crate::addons::record;
use crate::addons::reshade::split_install::{InstallRoots, run_split_install};

use super::super::errors;
use super::PreparedInstall;
use super::ops::{addon_op, combined_ops, host_ops, ini_op_for_game};
use crate::addons::reshade::host_policy;
use crate::addons::reshade::scan as reshade;
use crate::addons::reshade::update::host_binary_source;

/// Installs a Direct3D (proxy-DLL) RenoDX host.
///
/// Refuses if a RenoDX install record already exists here (the caller should
/// uninstall first to reinstall). When no ReShade host is present one is installed
/// (proxy DLL + fresh `ReShade.ini` + marker); a compatible empty host is
/// adopted, while a compatible user setup remains reused with only RenoDX's
/// additive, no-backup INI merge.
pub(super) fn install_proxy(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<InstalledAddon, ServiceError> {
    let host = host_policy::assess(game_dir, &prepared.proxy_dll_name);
    host.ensure_initial_installable(&prepared.proxy_dll_name)?;
    if host.initial_writes_host() && prepared.reshade_dll_bytes.is_empty() {
        return Err(errors::invalid(
            "the active ReShade host needs installation or repair, but no ReShade bytes were provided"
                .to_owned(),
        ));
    }

    let paths = reshade::resolve_paths(game_dir, Some(&host.target_path));
    if !paths.effective_addon_path.is_dir() {
        return Err(errors::invalid(format!(
            "ReShade AddonPath `{}` does not exist",
            paths.effective_addon_path.display()
        )));
    }
    let adopted_existing = host.initial_owned_existing_paths(paths.ini_path.as_deref());

    let roots = InstallRoots::resolve(game_dir, &host.target_path);
    let receipt = run_split_install(
        &roots,
        AddonKind::RenoDx,
        combined_ops(game_dir, prepared, host.initial_writes_host()),
        vec![addon_op(prepared)],
        host_ops(game_dir, prepared, host.initial_writes_host()),
    )?;
    build_record(
        prepared,
        &paths.effective_addon_path,
        host.initial_writes_host(),
        &adopted_existing,
        &receipt,
    )
}

/// Assembles the [`InstalledAddon`] from the engine receipt and the upstream entries
/// to track: the add-on (when fetched upstream) and any replaced/created ReShade host.
pub(super) fn build_record(
    prepared: &PreparedInstall,
    addon_dir: &Path,
    tracks_host: bool,
    adopted_existing: &[PathBuf],
    receipt: &engine::InstallReceipt,
) -> Result<InstalledAddon, ServiceError> {
    let addon_path = addon_dir.join(&prepared.addon_file_name);

    let mut sources = Vec::new();
    if !prepared.addon_source_url.is_empty() || prepared.source_last_modified.is_some() {
        sources.push(
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                prepared.addon_source_url.clone(),
                prepared.source_etag.clone(),
                prepared.source_digest.clone(),
            )
            .with_last_modified(prepared.source_last_modified.clone()),
        );
    }
    if tracks_host {
        sources.push(host_binary_source(
            prepared.reshade_source_url.clone(),
            prepared.reshade_source_etag.clone(),
            prepared.reshade_digest.clone(),
            prepared.reshade_last_modified.clone(),
            prepared.reshade_channel,
        ));
    }

    let record = record::build(
        prepared.game_id.clone(),
        AddonKind::RenoDx,
        &addon_path,
        receipt,
        sources,
    )?;
    record::adopt_existing_paths(record, adopted_existing)
}

/// The upstream add-on entry to track for updates, or `None` for a file install
/// (empty URL and no mtime placeholder) which has nothing to track.
pub(super) fn addon_tracked_source(prepared: &PreparedInstall) -> Option<TrackedSource> {
    if prepared.addon_source_url.is_empty() && prepared.source_last_modified.is_none() {
        return None;
    }
    Some(
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            prepared.addon_source_url.clone(),
            prepared.source_etag.clone(),
            prepared.source_digest.clone(),
        )
        .with_last_modified(prepared.source_last_modified.clone()),
    )
}

/// Installs a Vulkan RenoDX add-on into `game_dir`.
///
/// The host is the shared Vulkan layer (installed separately by the service),
/// so this lays down only per-game files: the add-on and a `ReShade.ini` with
/// the required add-on configuration. Refuses if the add-on is already present.
pub(super) fn install_vulkan(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<InstalledAddon, ServiceError> {
    let addon_path = game_dir.join(&prepared.addon_file_name);
    if addon_path.is_file() {
        return Err(errors::invalid(
            "RenoDX is already installed for this game; uninstall before reinstalling".to_owned(),
        ));
    }

    let plan = build_vulkan_plan(prepared, game_dir)?;
    let receipt = engine::install(game_dir, &plan)?;

    let sources: Vec<TrackedSource> = addon_tracked_source(prepared).into_iter().collect();
    record::build(
        prepared.game_id.clone(),
        AddonKind::RenoDx,
        &addon_path,
        &receipt,
        sources,
    )
}

/// Builds the per-game file operations for a Vulkan install: the add-on and the
/// `ReShade.ini` merge. Install state is tracked in the app database.
pub(super) fn build_vulkan_plan(
    prepared: &PreparedInstall,
    game_dir: &Path,
) -> Result<InstallPlan, ServiceError> {
    let mut ops = vec![addon_op(prepared)];
    if let Some(ini_op) = ini_op_for_game(game_dir, &prepared.ini_tweaks) {
        ops.push(ini_op);
    }
    Ok(InstallPlan {
        kind: AddonKind::RenoDx,
        ops,
    })
}
