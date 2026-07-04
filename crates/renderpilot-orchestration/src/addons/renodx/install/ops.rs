use std::path::Path;

use crate::addons::engine::FileOp;

use super::PreparedInstall;
use crate::addons::reshade::ini_schema::ini_merge_strategy;
use crate::addons::reshade::scan as reshade;
use crate::addons::reshade::types::ReshadeIniTweaks;

pub(super) fn combined_ops(
    game_dir: &Path,
    prepared: &PreparedInstall,
    writes_host: bool,
) -> Vec<FileOp> {
    let mut ops = vec![addon_op(prepared)];
    if writes_host {
        ops.push(host_op(prepared));
    }
    if let Some(ini_op) = ini_op_for_game(game_dir, &prepared.ini_tweaks) {
        ops.push(ini_op);
    }
    ops
}

/// The RenoDX add-on file op: a rolling upstream snapshot RenoDx already
/// PE-sanity-checked, so a pre-existing file at that path (a prior install) is
/// simply overwritten — nothing about the old bytes is worth preserving.
pub(super) fn addon_op(prepared: &PreparedInstall) -> FileOp {
    FileOp::Replace {
        name: prepared.addon_file_name.clone(),
        bytes: prepared.addon_bytes.clone(),
    }
}

/// The ReShade host DLL op: an official redistributable RenoDx fetched itself,
/// so a pre-existing file in that slot is overwritten with no on-disk backup —
/// its identity is confirmed by [`host_policy::assess`] before this ever runs.
pub(super) fn host_op(prepared: &PreparedInstall) -> FileOp {
    FileOp::Replace {
        name: prepared.proxy_dll_name.clone(),
        bytes: prepared.reshade_dll_bytes.clone(),
    }
}

pub(super) fn host_ops(
    game_dir: &Path,
    prepared: &PreparedInstall,
    writes_host: bool,
) -> Vec<FileOp> {
    let mut ops = Vec::new();
    if writes_host {
        ops.push(host_op(prepared));
    }
    if let Some(ini_op) = ini_op_for_game(game_dir, &prepared.ini_tweaks) {
        ops.push(ini_op);
    }
    ops
}

/// The `ReShade.ini` merge operation: additively set RenoDX's `[ADDON]` keys,
/// creating the file from empty when none exists. Uses `UpdateText` rather than
/// `MergeText` — RenoDX never keeps a `.bak` of a config file that may carry the
/// user's own hand-tuned ReShade settings. The engine itself tracks a from-empty
/// write as `created_files` (see `engine::InstallChanges::into_receipt`), so
/// `install_plans`/`build_vulkan_plan` need no extra book-keeping for it.
pub(super) fn ini_op_for_game(game_dir: &Path, tweaks: &ReshadeIniTweaks) -> Option<FileOp> {
    let tweaks = effective_ini_tweaks(game_dir, tweaks);
    ini_tweaks_write_keys(&tweaks).then(|| FileOp::UpdateText {
        name: reshade::RESHADE_INI_FILE_NAME.to_owned(),
        default: String::new(),
        strategy: ini_merge_strategy(&tweaks),
    })
}

pub(super) fn effective_ini_tweaks(game_dir: &Path, tweaks: &ReshadeIniTweaks) -> ReshadeIniTweaks {
    let mut effective = tweaks.clone();
    if reshade::has_user_effect_assets(game_dir) {
        effective.disabled_addons.clear();
    }
    effective
}

pub(super) fn ini_tweaks_write_keys(tweaks: &ReshadeIniTweaks) -> bool {
    !tweaks.disabled_addons.is_empty() || tweaks.addon_path.is_some() || tweaks.dlss_fix.is_some()
}
