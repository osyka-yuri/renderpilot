//! Desktop-layer facade for NVAPI and DLSS preset operations.
//!
//! Each public function is a thin handler for a Tauri command: it resolves the game,
//! builds a `SettingContext` once (DLL detection + exe resolution), opens a DRS session
//! as needed, and returns a `SettingStateResponse`. Errors map to `ApiError::Service`
//! with human-readable context for the frontend.

use std::path::Path;

use renderpilot_orchestration::ServiceError;

use crate::{
    ApiError,
    utils::{JsonResult, parse_game_id, to_json},
};

use renderpilot_orchestration::domain::GameId;
#[cfg(windows)]
use renderpilot_orchestration::nvapi::dto::executable_candidate_dto;
use renderpilot_orchestration::nvapi::dto::{
    ExecutableCandidateDto, SettingDescriptorDto, setting_descriptor_dto,
};
use renderpilot_orchestration::nvapi::ops::{
    SettingTarget, WriteOp, read_all_setting_states, read_setting_state, resolve_revert_op,
    validate_value_supported, write_setting_value,
};
use renderpilot_orchestration::nvapi::registry::{lookup_setting, supported_settings};
use renderpilot_orchestration::nvapi::resolve::{
    build_setting_context_with_context, clear_executable_override, collect_executable_candidates,
    global_setting_context, load_game_with_context, resolve_effective_executable,
    set_executable_override,
};
use renderpilot_orchestration::nvapi::{NvapiSetting, SettingContext};

/// Looks up an NVAPI setting by key, or returns the standard "unknown setting"
/// error. Shared by every handler so the message stays identical.
fn lookup_setting_or_err(setting_key: &str) -> Result<Box<dyn NvapiSetting>, ApiError> {
    lookup_setting(setting_key).ok_or_else(|| {
        ApiError::Service(ServiceError::command_failed(format!(
            "unknown nvapi setting: {setting_key}"
        )))
    })
}

/// Parses `game_id`, loads the game, and builds its per-game [`SettingContext`]
/// (DLL detection + executable resolution) in one step. Returns the parsed id
/// alongside the context so the caller can build a [`SettingTarget::Game`].
fn build_game_context(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> Result<(GameId, SettingContext), ApiError> {
    let game_id = parse_game_id(game_id)?;
    let game = load_game_with_context(context, game_id.as_str())?;
    let install_dir = Path::new(game.install_path().as_str()).to_path_buf();
    let ctx = build_setting_context_with_context(context, &install_dir, game_id.as_str())?;
    Ok((game_id, ctx))
}

// ---------------------------------------------------------------------------
// Public Tauri-facing entry points
// ---------------------------------------------------------------------------

/// Retrieves a comprehensive list of all NVAPI settings currently supported and recognized by RenderPilot.
pub fn list_nvapi_supported_settings(_game_id: String) -> JsonResult {
    let dtos: Vec<SettingDescriptorDto> = supported_settings()
        .iter()
        .map(|setting| setting_descriptor_dto(setting.as_ref()))
        .collect();
    to_json(dtos)
}

/// Reads the live state of **every** supported NVAPI setting for `game_id`
/// through a single DRS session. Backs the grouped "DLSS driver settings" UI;
/// far cheaper than calling [`get_nvapi_setting_state`] once per setting.
pub fn list_nvapi_setting_states(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> JsonResult {
    let (game_id, ctx) = build_game_context(context, game_id)?;
    let settings = supported_settings();
    let target = SettingTarget::Game {
        game_id: game_id.as_str(),
    };
    let responses = read_all_setting_states(context, &target, &settings, &ctx)?;
    to_json(responses)
}

/// Scans the game's installation directory and returns a comprehensive list of all detected executables.
pub fn list_game_executable_candidates(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let game = load_game_with_context(context, game_id.as_str())?;
    let install_dir = Path::new(game.install_path().as_str()).to_path_buf();
    let candidates = collect_executable_candidates(&install_dir);
    #[cfg(windows)]
    let dtos: Vec<ExecutableCandidateDto> = candidates
        .into_iter()
        .map(executable_candidate_dto)
        .collect();
    #[cfg(not(windows))]
    let dtos = {
        drop(candidates);
        Vec::<ExecutableCandidateDto>::new()
    };
    to_json(dtos)
}

/// Resolves the game's effective primary executable (a pinned override or the
/// auto-detected renderer), independent of NVAPI. Backs the shared game-level
/// executable selector used by both the NVIDIA and RenoDX panels. Serializes to
/// `null` when the install directory holds no game binary.
pub fn resolve_game_executable(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    let game = load_game_with_context(context, game_id.as_str())?;
    let install_dir = Path::new(game.install_path().as_str()).to_path_buf();
    let effective = resolve_effective_executable(context, &install_dir, game_id.as_str())?;
    to_json(effective)
}

/// Enforces a persistent, explicit executable override for the specified `game_id`.
pub fn set_game_executable_override(
    context: &renderpilot_orchestration::Context,
    game_id: String,
    absolute_path: &str,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    set_executable_override(context, game_id.as_str(), absolute_path)?;
    to_json(serde_json::json!({"ok": true}))
}

/// Clears any previously pinned executable overrides associated with the specified `game_id`.
pub fn clear_game_executable_override(
    context: &renderpilot_orchestration::Context,
    game_id: String,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    clear_executable_override(context, game_id.as_str())?;
    to_json(serde_json::json!({"ok": true}))
}

/// Interrogates the driver to retrieve the live operational state of a specific setting for `game_id`.
pub fn get_nvapi_setting_state(
    context: &renderpilot_orchestration::Context,
    game_id: String,
    setting_key: &str,
) -> JsonResult {
    let setting = lookup_setting_or_err(setting_key)?;
    let (game_id, ctx) = build_game_context(context, game_id)?;
    let target = SettingTarget::Game {
        game_id: game_id.as_str(),
    };
    let response = read_setting_state(context, &target, setting.as_ref(), &ctx)?;
    to_json(response)
}

/// Commits a new configuration value for a specified NVAPI setting.
pub fn set_nvapi_setting_value(
    context: &renderpilot_orchestration::Context,
    game_id: String,
    setting_key: &str,
    value: &str,
) -> JsonResult {
    let setting = lookup_setting_or_err(setting_key)?;
    let (game_id, ctx) = build_game_context(context, game_id)?;

    let dword = setting.parse_wire(value).ok_or_else(|| {
        ApiError::Service(ServiceError::command_failed(format!(
            "invalid value `{value}` for {}",
            setting.key()
        )))
    })?;
    validate_value_supported(setting.as_ref(), dword, &ctx)?;

    let target = SettingTarget::Game {
        game_id: game_id.as_str(),
    };
    write_setting_value(
        context,
        &target,
        setting.as_ref(),
        &ctx,
        WriteOp::Set(dword),
    )?;
    let response = read_setting_state(context, &target, setting.as_ref(), &ctx)?;
    to_json(response)
}

/// Restores a designated NVAPI setting to either its driver-predefined default or its historical baseline state.
pub fn revert_nvapi_setting(
    context: &renderpilot_orchestration::Context,
    game_id: String,
    setting_key: &str,
    target: &str,
) -> JsonResult {
    let setting = lookup_setting_or_err(setting_key)?;
    let (game_id, ctx) = build_game_context(context, game_id)?;

    let setting_target = SettingTarget::Game {
        game_id: game_id.as_str(),
    };
    let op = resolve_revert_op(context, &setting_target, setting.as_ref(), target)?;

    write_setting_value(context, &setting_target, setting.as_ref(), &ctx, op)?;
    let response = read_setting_state(context, &setting_target, setting.as_ref(), &ctx)?;
    to_json(response)
}

// ---------------------------------------------------------------------------
// Global (base profile) entry points
// ---------------------------------------------------------------------------
//
// These mirror the per-game handlers above but target NVIDIA's global/base
// driver profile (`SettingTarget::Global`) and take no `game_id`. All of the
// real work lives in the shared orchestration functions; only the target and
// the empty global context differ.

/// Reads the live state of every supported NVAPI setting from the global/base
/// driver profile in one DRS session.
pub fn list_global_nvapi_setting_states(
    context: &renderpilot_orchestration::Context,
) -> JsonResult {
    let ctx = global_setting_context();
    let settings = supported_settings();
    let responses = read_all_setting_states(context, &SettingTarget::Global, &settings, &ctx)?;
    to_json(responses)
}

/// Commits a new value for an NVAPI setting on the global/base driver profile.
pub fn set_global_nvapi_setting_value(
    context: &renderpilot_orchestration::Context,
    setting_key: &str,
    value: &str,
) -> JsonResult {
    let setting = lookup_setting_or_err(setting_key)?;
    let ctx = global_setting_context();
    let dword = setting.parse_wire(value).ok_or_else(|| {
        ApiError::Service(ServiceError::command_failed(format!(
            "invalid value `{value}` for {}",
            setting.key()
        )))
    })?;
    validate_value_supported(setting.as_ref(), dword, &ctx)?;

    write_setting_value(
        context,
        &SettingTarget::Global,
        setting.as_ref(),
        &ctx,
        WriteOp::Set(dword),
    )?;
    let response = read_setting_state(context, &SettingTarget::Global, setting.as_ref(), &ctx)?;
    to_json(response)
}

/// Reverts an NVAPI setting on the global/base driver profile. Only the
/// `"predefined"` target is valid globally (there is no per-game baseline).
pub fn revert_global_nvapi_setting(
    context: &renderpilot_orchestration::Context,
    setting_key: &str,
    target: &str,
) -> JsonResult {
    let setting = lookup_setting_or_err(setting_key)?;
    let ctx = global_setting_context();
    let op = resolve_revert_op(context, &SettingTarget::Global, setting.as_ref(), target)?;
    write_setting_value(context, &SettingTarget::Global, setting.as_ref(), &ctx, op)?;
    let response = read_setting_state(context, &SettingTarget::Global, setting.as_ref(), &ctx)?;
    to_json(response)
}

// ---------------------------------------------------------------------------
// Test stubs
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    #[test]
    fn setting_registry_finds_dlss_render_preset() {
        let s = lookup_setting("dlss_sr_render_preset").expect("known setting should resolve");
        assert!(s.dll_kind().is_some());
    }

    #[test]
    fn setting_registry_returns_none_for_unknown_key() {
        assert!(lookup_setting("does.not.exist").is_none());
    }

    #[test]
    fn supported_settings_lists_dlss_render_preset_first() {
        let settings = supported_settings();
        assert!(!settings.is_empty());
        assert_eq!(settings[0].key(), "dlss_sr_render_preset");
    }

    #[test]
    fn lookup_setting_or_err_rejects_unknown_key() {
        // Backs the global set/revert handlers' key validation without needing a
        // GPU: an unknown key must surface the standard unknown-setting error.
        match lookup_setting_or_err("does.not.exist") {
            Ok(_) => panic!("unknown key must error"),
            Err(err) => assert_matches!(err, ApiError::Service(_)),
        }
    }

    #[test]
    fn supported_settings_covers_new_dlss_family() {
        let keys: Vec<&str> = supported_settings().iter().map(|s| s.key()).collect();
        for key in [
            "dlss_sr_quality_level",
            "dlss_sr_scaling_ratio",
            "dlss_fg_render_preset",
            "dlss_mfg_fixed_count",
            "dlss_rr_quality_level",
        ] {
            assert!(keys.contains(&key), "missing {key}");
        }
    }
}
