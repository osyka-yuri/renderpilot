use std::path::{Path, PathBuf};

use renderpilot_domain::{InstalledAddon, InstalledAddonHostKind, PathRef};

use crate::ServiceError;
use crate::addons::engine;

use super::super::reshade_ini::ini_remove_renodx_strategy;
use super::super::tracking;
use crate::addons::reshade::scan as reshade;

/// Reverses an install, returning the game folder to its prior state.
///
/// `ReShade.ini` is never part of the engine's generic list-based reversal below —
/// it is filtered out of `created_files`/`backed_up_files` first and handled on
/// its own, so it is **never restored from a `.bak` snapshot** even for a legacy
/// record whose ini predates RenoDX's no-backup policy (an old `MergeText`
/// install that backed one up). Its fate: deleted outright only when this record
/// created it from nothing, no legacy backup exists for it either, *and* this
/// install owns the whole ReShade stack it sits beside — a Vulkan install (the
/// per-game ini is exclusively RenoDX's; the shared layer is a separate concern),
/// or a proxy install that also wrote/replaced the host DLL itself. A reused,
/// merely-compatible host (nothing about it is "ours") never gets its freshly
/// created ini deleted either, even though RenoDX is the one that wrote it —
/// only stripped, same as a pre-existing one. Stripping (via
/// [`ini_remove_renodx_strategy`]) removes exactly RenoDX's own keys, leaving
/// everything else — the user's own settings, and any orphaned legacy `.bak` —
/// exactly as it was. `game_dir_hint` (from the caller's own game-folder
/// resolution) is a best-effort assist for locating an ini this record's
/// book-keeping doesn't reference at all (an `UpdateText` merge into a
/// pre-existing ini is deliberately untracked; see [`super::ops::ini_op_for_game`]) if the
/// host's own directory can't be resolved either.
pub fn uninstall(
    record: &InstalledAddon,
    game_dir_hint: Option<&Path>,
) -> Result<(), ServiceError> {
    let log_base_path = if record.has_host_binary_provenance() {
        tracking::rollback_host_path(record).and_then(|host_path| {
            host_path.parent().map(|game_dir| {
                reshade::resolve_paths(game_dir, Some(&host_path)).effective_base_path
            })
        })
    } else {
        None
    };

    let ini_in_created = ini_path_in(record.created_files());
    let ini_in_backed_up = ini_path_in(record.backed_up_files());
    let owns_whole_stack = matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    ) || host_dll_written_by_this_install(record);

    engine::uninstall(
        &non_ini_path_bufs(record.created_files()),
        &non_ini_path_bufs(record.backed_up_files()),
    )?;

    match ini_in_created.or(ini_in_backed_up) {
        Some(ini_ref) if ini_in_backed_up.is_none() && owns_whole_stack => {
            crate::fs::remove_file_if_exists(Path::new(ini_ref.as_str()))?;
        }
        Some(ini_ref) => strip_renodx_ini_keys_best_effort(Path::new(ini_ref.as_str())),
        None => {
            if let Some(ini_path) = locate_untracked_ini(record, game_dir_hint) {
                strip_renodx_ini_keys_best_effort(&ini_path);
            }
        }
    }

    if let Some(base_path) = log_base_path {
        reshade::remove_reshade_logs_best_effort(&base_path);
    }
    Ok(())
}

fn host_dll_written_by_this_install(record: &InstalledAddon) -> bool {
    record
        .created_files()
        .iter()
        .any(|path| path.file_name().is_some_and(reshade::is_proxy_slot))
}

fn ini_path_in(paths: &[PathRef]) -> Option<&PathRef> {
    paths.iter().find(|path| is_ini_path(path))
}

fn is_ini_path(path: &PathRef) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(reshade::RESHADE_INI_FILE_NAME))
}

fn non_ini_path_bufs(paths: &[PathRef]) -> Vec<PathBuf> {
    let filtered: Vec<PathRef> = paths
        .iter()
        .filter(|path| !is_ini_path(path))
        .cloned()
        .collect();
    crate::addons::path_bufs(&filtered)
}

fn locate_untracked_ini(record: &InstalledAddon, game_dir_hint: Option<&Path>) -> Option<PathBuf> {
    let host_dir =
        tracking::rollback_host_path(record).and_then(|path| path.parent().map(Path::to_path_buf));
    let addon_dir = Path::new(record.addon_file().as_str())
        .parent()
        .map(Path::to_path_buf);

    host_dir
        .into_iter()
        .chain(game_dir_hint.map(Path::to_path_buf))
        .chain(addon_dir)
        .find_map(|dir| reshade::reshade_ini_path(&dir))
}

fn strip_renodx_ini_keys_best_effort(ini_path: &Path) {
    let existing = match std::fs::read_to_string(ini_path) {
        Ok(contents) => contents,
        Err(error) => {
            log::warn!(
                "RenoDX uninstall: failed to read `{}` to strip its keys: {error}",
                ini_path.display()
            );
            return;
        }
    };
    let stripped = ini_remove_renodx_strategy().apply(&existing);
    if let Err(error) = crate::fs::write_file_atomically(ini_path, stripped.as_bytes()) {
        log::warn!(
            "RenoDX uninstall: failed to strip its keys from `{}`: {error}",
            ini_path.display()
        );
    }
}
