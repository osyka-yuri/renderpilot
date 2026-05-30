//! The desktop-layer facade responsible for orchestrating NVAPI operations and DLSS preset management.
//!
//! These entry points serve as handlers for asynchronous Tauri commands. A typical 
//! execution flow for these handlers involves:
//!
//!   1. Resolving the target game profile from the local SQLite catalog.
//!   2. Constructing a `SettingContext` exclusively once per request, ensuring that 
//!      costly operations such as DLSS DLL detection and PE (Portable Executable) 
//!      parsing are not duplicated.
//!   3. Determining the effective executable by evaluating explicit user overrides, 
//!      falling back to auto-detected candidates, or yielding an error.
//!   4. Initializing an NVAPI session (if the driver is involved), performing the 
//!      requested read/write/delete operations on the setting, and committing the changes.
//!   5. Capturing a pristine baseline snapshot prior to the first write operation, 
//!      guaranteeing the availability of a "Revert to baseline" feature.
//!   6. Producing a comprehensive `SettingStateResponse` that describes the post-operation 
//!      state, including predefined driver values, the baseline snapshot, valid configuration 
//!      options, and any diagnostic warnings.
//!
//! Any encountered failures are mapped to `CliError::CommandFailed` containing 
//! actionable, human-readable context, which the frontend UI subsequently displays.

mod dto;
mod ops;
mod registry;
mod resolve;

use std::path::Path;

use crate::{
    catalog::open_catalog_storage,
    desktop::utils::{to_json, JsonResult},
    error::CliError,
};

use self::dto::{
    executable_candidate_dto, setting_descriptor_dto, ExecutableCandidateDto, SettingDescriptorDto,
};
use self::ops::{read_setting_state, validate_value_supported, write_setting_value, WriteOp};
use self::registry::{lookup_setting, supported_settings};
use self::resolve::{
    build_setting_context_with_storage, collect_executable_candidates, load_game,
    load_game_with_storage,
};

// ---------------------------------------------------------------------------
// Public Tauri-facing entry points
// ---------------------------------------------------------------------------

/// Retrieves a comprehensive list of all NVAPI settings currently supported and recognized by RenderPilot.
pub fn list_nvapi_supported_settings(game_id: String) -> JsonResult {
    let _ = game_id;
    let dtos: Vec<SettingDescriptorDto> = supported_settings()
        .iter()
        .map(|setting| setting_descriptor_dto(setting.as_ref()))
        .collect();
    to_json(dtos)
}

/// Scans the game's installation directory and returns a comprehensive list of all detected executables.
pub fn list_game_executable_candidates(game_id: String) -> JsonResult {
    let game = load_game(&game_id)?;
    let install_dir = Path::new(game.install_path().as_str()).to_path_buf();
    let candidates = collect_executable_candidates(&install_dir);
    let dtos: Vec<ExecutableCandidateDto> = candidates
        .into_iter()
        .map(executable_candidate_dto)
        .collect();
    to_json(dtos)
}

/// Enforces a persistent, explicit executable override for the specified `game_id`.
pub fn set_game_executable_override(game_id: String, absolute_path: String) -> JsonResult {
    let game = load_game(&game_id)?;
    let install_dir = Path::new(game.install_path().as_str());
    let exe_path = Path::new(&absolute_path);

    let canonical_install = install_dir.canonicalize().map_err(|error| {
        CliError::CommandFailed(format!(
            "could not canonicalize install dir {}: {error}",
            install_dir.display()
        ))
    })?;
    let canonical_exe = exe_path.canonicalize().map_err(|error| {
        CliError::CommandFailed(format!(
            "could not canonicalize executable {}: {error}",
            exe_path.display()
        ))
    })?;
    if !canonical_exe.starts_with(&canonical_install) {
        return Err(CliError::CommandFailed(format!(
            "executable must be located inside the install directory ({})",
            install_dir.display()
        )));
    }
    let file_name = canonical_exe
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| CliError::CommandFailed("executable path has no file name".to_owned()))?;

    let storage = open_catalog_storage()?;
    let normalized = canonical_exe.to_string_lossy().replace('\\', "/");
    storage.upsert_nvapi_executable_override(game_id.as_str(), &normalized, file_name)?;
    to_json(serde_json::json!({"ok": true}))
}

/// Clears any previously pinned executable overrides associated with the specified `game_id`.
pub fn clear_game_executable_override(game_id: String) -> JsonResult {
    let _game = load_game(&game_id)?;
    let storage = open_catalog_storage()?;
    storage.delete_nvapi_executable_override(game_id.as_str())?;
    to_json(serde_json::json!({"ok": true}))
}

/// Interrogates the driver to retrieve the live operational state of a specific setting for `game_id`.
pub fn get_nvapi_setting_state(game_id: String, setting_key: String) -> JsonResult {
    let setting = lookup_setting(&setting_key)
        .ok_or_else(|| CliError::CommandFailed(format!("unknown nvapi setting: {setting_key}")))?;
    let storage = open_catalog_storage()?;
    let game = load_game_with_storage(&storage, &game_id)?;
    let install_dir = Path::new(game.install_path().as_str()).to_path_buf();
    let ctx = build_setting_context_with_storage(&storage, &install_dir, &game_id)?;
    let response = read_setting_state(&game_id, setting.as_ref(), &ctx)?;
    to_json(response)
}

/// Commits a new configuration value for a specified NVAPI setting.
pub fn set_nvapi_setting_value(game_id: String, setting_key: String, value: String) -> JsonResult {
    let setting = lookup_setting(&setting_key)
        .ok_or_else(|| CliError::CommandFailed(format!("unknown nvapi setting: {setting_key}")))?;
    let storage = open_catalog_storage()?;
    let game = load_game_with_storage(&storage, &game_id)?;
    let install_dir = Path::new(game.install_path().as_str()).to_path_buf();
    let ctx = build_setting_context_with_storage(&storage, &install_dir, &game_id)?;

    let dword = setting.parse_wire(&value).ok_or_else(|| {
        CliError::CommandFailed(format!("invalid value `{value}` for {}", setting.key()))
    })?;
    validate_value_supported(setting.as_ref(), dword, &ctx)?;

    write_setting_value(&game_id, setting.as_ref(), &ctx, WriteOp::Set(dword))?;
    let response = read_setting_state(&game_id, setting.as_ref(), &ctx)?;
    to_json(response)
}

/// Restores a designated NVAPI setting to either its driver-predefined default or its historical baseline state.
pub fn revert_nvapi_setting(game_id: String, setting_key: String, target: String) -> JsonResult {
    let setting = lookup_setting(&setting_key)
        .ok_or_else(|| CliError::CommandFailed(format!("unknown nvapi setting: {setting_key}")))?;
    let storage = open_catalog_storage()?;
    let game = load_game_with_storage(&storage, &game_id)?;
    let install_dir = Path::new(game.install_path().as_str()).to_path_buf();
    let ctx = build_setting_context_with_storage(&storage, &install_dir, &game_id)?;

    let op = match target.as_str() {
        "predefined" => WriteOp::Delete,
        "baseline" => {
            let baseline = storage
                .get_nvapi_baseline(game_id.as_str(), setting.key())?
                .ok_or_else(|| {
                    CliError::CommandFailed(
                        "no baseline recorded yet — set a value at least once first".to_owned(),
                    )
                })?;
            if baseline.baseline_was_predefined {
                WriteOp::Delete
            } else {
                WriteOp::Set(baseline.baseline_dword)
            }
        }
        other => {
            return Err(CliError::CommandFailed(format!(
                "invalid revert target `{other}`; expected 'predefined' or 'baseline'"
            )))
        }
    };

    write_setting_value(&game_id, setting.as_ref(), &ctx, op)?;
    let response = read_setting_state(&game_id, setting.as_ref(), &ctx)?;
    to_json(response)
}

// ---------------------------------------------------------------------------
// Test stubs
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_nvapi::dlss::DlssRenderPresetSetting;

    #[test]
    fn setting_registry_finds_dlss_render_preset() {
        let s = lookup_setting(DlssRenderPresetSetting::KEY).expect("known setting should resolve");
        assert_eq!(s.key(), "dlss_sr_render_preset");
        assert_eq!(s.dll_kind(), Some(renderpilot_nvapi::DlssDllKind::Sr));
    }

    #[test]
    fn setting_registry_returns_none_for_unknown_key() {
        assert!(lookup_setting("does.not.exist").is_none());
    }

    #[test]
    fn supported_settings_lists_dlss_render_preset_first() {
        let settings = supported_settings();
        assert!(!settings.is_empty());
        assert_eq!(settings[0].key(), DlssRenderPresetSetting::KEY);
    }
}
