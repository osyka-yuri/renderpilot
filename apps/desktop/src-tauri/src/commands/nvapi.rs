//! NVAPI / DLSS preset and global driver-profile Tauri commands.

use std::sync::Arc;

use renderpilot_api as desktop;
use renderpilot_orchestration::Context;

use super::validation::require_non_empty_string;
use super::{CommandError, JsonCommandResult, require_game_context, run_desktop_command};

// ---------------------------------------------------------------------------
// NVAPI / DLSS preset commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_nvapi_supported_settings(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    run_desktop_command(move || desktop::list_nvapi_supported_settings(game_id)).await
}

#[tauri::command]
pub async fn list_nvapi_setting_states(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    run_desktop_command(move || desktop::list_nvapi_setting_states(&context, game_id)).await
}

#[tauri::command]
pub async fn list_game_executable_candidates(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    run_desktop_command(move || desktop::list_game_executable_candidates(&context, game_id)).await
}

#[tauri::command]
pub async fn resolve_game_executable(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    run_desktop_command(move || desktop::resolve_game_executable(&context, game_id)).await
}

#[tauri::command]
pub async fn set_game_executable_override(
    game_id: String,
    absolute_path: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    let absolute_path = require_non_empty_string("absolute_path", absolute_path)?;
    let refresh_context = Arc::clone(&context);
    let changed_game_id = renderpilot_orchestration::domain::GameId::new(game_id.clone())
        .map_err(|_| CommandError::invalid_argument("game_id", "must be a valid game id"))?;
    let result = run_desktop_command(move || {
        desktop::set_game_executable_override(&context, game_id, &absolute_path)
    })
    .await;
    if result.is_ok() {
        super::addon_catalog::refresh_game_catalog_addon_capabilities(
            refresh_context,
            changed_game_id,
        )
        .await;
    }
    result
}

#[tauri::command]
pub async fn clear_game_executable_override(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    let refresh_context = Arc::clone(&context);
    let changed_game_id = renderpilot_orchestration::domain::GameId::new(game_id.clone())
        .map_err(|_| CommandError::invalid_argument("game_id", "must be a valid game id"))?;
    let result =
        run_desktop_command(move || desktop::clear_game_executable_override(&context, game_id))
            .await;
    if result.is_ok() {
        super::addon_catalog::refresh_game_catalog_addon_capabilities(
            refresh_context,
            changed_game_id,
        )
        .await;
    }
    result
}

#[tauri::command]
pub async fn get_nvapi_setting_state(
    game_id: String,
    setting_key: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    run_desktop_command(move || desktop::get_nvapi_setting_state(&context, game_id, &setting_key))
        .await
}

#[tauri::command]
pub async fn set_nvapi_setting_value(
    game_id: String,
    setting_key: String,
    value: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    let value = require_non_empty_string("value", value)?;
    run_desktop_command(move || {
        desktop::set_nvapi_setting_value(&context, game_id, &setting_key, &value)
    })
    .await
}

#[tauri::command]
pub async fn revert_nvapi_setting(
    game_id: String,
    setting_key: String,
    target: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    let target = require_non_empty_string("target", target)?;
    run_desktop_command(move || {
        desktop::revert_nvapi_setting(&context, game_id, &setting_key, &target)
    })
    .await
}

// ---------------------------------------------------------------------------
// Global (base profile) NVAPI settings
// ---------------------------------------------------------------------------

/// Reads the live state of every supported NVAPI setting from NVIDIA's
/// global/base driver profile.
#[tauri::command]
pub async fn list_global_nvapi_setting_states(
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let context = Arc::clone(&context);
    run_desktop_command(move || desktop::list_global_nvapi_setting_states(&context)).await
}

/// Commits a new value for an NVAPI setting on the global/base driver profile.
#[tauri::command]
pub async fn set_global_nvapi_setting_value(
    setting_key: String,
    value: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    let value = require_non_empty_string("value", value)?;
    let context = Arc::clone(&context);
    run_desktop_command(move || {
        desktop::set_global_nvapi_setting_value(&context, &setting_key, &value)
    })
    .await
}

/// Reverts an NVAPI setting on the global/base driver profile to the driver default.
#[tauri::command]
pub async fn revert_global_nvapi_setting(
    setting_key: String,
    target: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    let target = require_non_empty_string("target", target)?;
    let context = Arc::clone(&context);
    run_desktop_command(move || {
        desktop::revert_global_nvapi_setting(&context, &setting_key, &target)
    })
    .await
}

// ---------------------------------------------------------------------------
// DLSS indicator (system-wide overlay)
// ---------------------------------------------------------------------------

/// Reads whether the global NVIDIA DLSS indicator overlay is currently enabled.
#[tauri::command]
pub async fn get_dlss_indicator_state() -> JsonCommandResult {
    run_desktop_command(renderpilot_api::get_dlss_indicator_state).await
}

/// Enables or disables the global NVIDIA DLSS indicator overlay (requires admin).
#[tauri::command]
pub async fn set_dlss_indicator_enabled(enabled: bool) -> JsonCommandResult {
    run_desktop_command(move || renderpilot_api::set_dlss_indicator_enabled(enabled)).await
}
