//! Durable file-mutation target computation for Luma commands.
//!
//! Each command snapshots every live/sidecar path it may touch so the outer
//! `DurableFileTransaction` can restore the exact before-state after a crash.
//! Targets are over-inclusive — an untouched path is only snapshotted.

use std::path::{Path, PathBuf};

use renderpilot_domain::{InstalledAddon, Version};

use crate::ServiceError;
use crate::addons::luma::install::{PreparedInstall, plan};
use crate::addons::mutation_targets::MutationTargets;
use crate::addons::reshade::host_policy;
use crate::addons::reshade::scan;
use crate::addons::reshade::split_install::InstallRoots;

/// Live/sidecar paths a Luma install may touch.
pub(crate) fn install_targets(
    game_dir: &Path,
    prepared: &PreparedInstall,
    min_host_version: &Version,
) -> Result<MutationTargets, ServiceError> {
    let host = host_policy::assess_for_tool(
        game_dir,
        &prepared.proxy_dll_name,
        "Luma",
        Some(min_host_version),
    );
    host.ensure_initial_installable(&prepared.proxy_dll_name)?;
    let paths = scan::resolve_paths(game_dir, Some(&host.target_path));
    let roots = InstallRoots::resolve(game_dir, &host.target_path);

    let live: Vec<PathBuf> = prepared
        .payload
        .iter()
        .map(|file| paths.effective_addon_path.join(&file.relative_path))
        .chain(plan::game_dir_live_paths(&roots.game_dir, prepared, &host))
        .collect();
    Ok(MutationTargets::from_roots_and_live_paths(
        [roots.game_dir, paths.effective_addon_path],
        live,
    ))
}

/// Live/sidecar paths a Luma uninstall may touch, including cascade restores.
pub(crate) fn uninstall_targets(
    game_root: PathBuf,
    record: &InstalledAddon,
    cascade_paths: impl IntoIterator<Item = PathBuf>,
) -> MutationTargets {
    MutationTargets::for_record(record, [game_root], cascade_paths)
}
