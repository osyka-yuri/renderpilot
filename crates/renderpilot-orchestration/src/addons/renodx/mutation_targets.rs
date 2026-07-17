//! Durable file-mutation target computation for RenoDX commands.
//!
//! Each command snapshots every live/sidecar path it may touch so the outer
//! `DurableFileTransaction` can restore the exact before-state after a crash.
//! Targets are over-inclusive — an untouched path is only snapshotted.

use std::path::{Path, PathBuf};

use renderpilot_domain::InstalledAddon;

use crate::addons::mutation_targets::MutationTargets;
use crate::addons::renodx::install::PreparedInstall;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::scan as reshade;
use crate::addons::reshade::split_install::InstallRoots;

/// Live/sidecar paths a RenoDX install (proxy or Vulkan) may touch.
pub(crate) fn install_targets(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<MutationTargets, crate::ServiceError> {
    match prepared.host_kind {
        HostKind::Proxy => {
            let roots = InstallRoots::resolve_from_ini(game_dir);
            let addon_path = roots.addon_dir.join(&prepared.addon_file_name);
            let host_path = roots.game_dir.join(&prepared.proxy_dll_name);
            let ini_path = reshade::reshade_ini_path(&roots.game_dir)
                .unwrap_or_else(|| roots.game_dir.join(reshade::RESHADE_INI_FILE_NAME));
            Ok(MutationTargets::from_roots_and_live_paths(
                [roots.game_dir, roots.addon_dir],
                [addon_path, host_path, ini_path],
            ))
        }
        HostKind::Vulkan => {
            let addon_path = game_dir.join(&prepared.addon_file_name);
            let ini_path = reshade::reshade_ini_path(game_dir)
                .unwrap_or_else(|| game_dir.join(reshade::RESHADE_INI_FILE_NAME));
            Ok(MutationTargets::from_roots_and_live_paths(
                [game_dir.to_path_buf()],
                [addon_path, ini_path],
            ))
        }
    }
}

/// Live/sidecar paths a RenoDX uninstall may touch.
pub(crate) fn uninstall_targets(
    record: &InstalledAddon,
    game_dir_hint: Option<&Path>,
) -> MutationTargets {
    let mut extra = Vec::new();
    if let Some(ini) = locate_uninstall_ini(record, game_dir_hint) {
        extra.push(ini);
    }
    MutationTargets::for_record(record, std::iter::empty(), extra)
}

/// Live/sidecar paths a RenoDX update may touch.
pub(crate) fn update_targets(
    record: &InstalledAddon,
    replacement_paths: &[PathBuf],
    host_install_path: Option<&Path>,
) -> MutationTargets {
    let mut extra: Vec<PathBuf> = replacement_paths.to_vec();
    if let Some(host_path) = host_install_path {
        extra.push(host_path.to_path_buf());
    }
    MutationTargets::for_record(record, std::iter::empty(), extra)
}

/// Live/sidecar paths a proxy ReShade channel switch may touch.
pub(crate) fn channel_switch_targets(target_path: &Path, game_dir: &Path) -> MutationTargets {
    MutationTargets::for_live_paths([game_dir.to_path_buf()], [target_path.to_path_buf()])
}

/// Live/sidecar paths a DLSS-Fix install or uninstall may touch.
pub(crate) fn dlss_fix_targets(game_dir: &Path, addon_file_name: &str) -> MutationTargets {
    let addon_path = game_dir.join(addon_file_name);
    let ini_path = reshade::reshade_ini_path(game_dir)
        .unwrap_or_else(|| game_dir.join(reshade::RESHADE_INI_FILE_NAME));
    MutationTargets::for_live_paths([game_dir.to_path_buf()], [addon_path, ini_path])
}

fn locate_uninstall_ini(record: &InstalledAddon, game_dir_hint: Option<&Path>) -> Option<PathBuf> {
    let host_dir = crate::addons::tracking::host_proxy_path(record)
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let addon_dir = Path::new(record.addon_file().as_str())
        .parent()
        .map(Path::to_path_buf);
    host_dir
        .into_iter()
        .chain(game_dir_hint.map(Path::to_path_buf))
        .chain(addon_dir)
        .find_map(|dir| reshade::reshade_ini_path(&dir))
}
