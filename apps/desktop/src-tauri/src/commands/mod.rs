//! Tauri command handlers for the desktop frontend.
//!
//! Blocking catalog / filesystem work is dispatched via `run_desktop_command` to avoid
//! stalling the async runtime.

mod error;
mod query_game_cards;
mod validation;

pub use error::CommandError;

use renderpilot_cli::{desktop, CliError};
use serde_json::Value;

pub type JsonCommandResult = Result<Value, CommandError>;

type DesktopCommandResult = Result<Value, CliError>;

use query_game_cards::{QueryGameCardsArgs, QueryGameCardsDto};
use validation::{require_non_empty_path, require_non_empty_string};

async fn run_desktop_command<F>(command: F) -> JsonCommandResult
where
    F: FnOnce() -> DesktopCommandResult + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(command)
        .await
        .map_err(CommandError::task_failed)?
        .map_err(CommandError::from)
}

async fn run_desktop_async_command<F, Fut>(command: F) -> JsonCommandResult
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = DesktopCommandResult> + Send + 'static,
{
    tauri::async_runtime::spawn(command())
        .await
        .map_err(CommandError::task_failed)?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn scan_manual_folder(path: String) -> JsonCommandResult {
    let path = require_non_empty_path(path)?;

    run_desktop_command(move || desktop::scan_manual_folder(path)).await
}

#[tauri::command]
pub async fn scan_auto_libraries() -> JsonCommandResult {
    run_desktop_command(desktop::scan_auto_libraries).await
}

#[tauri::command]
pub async fn query_game_cards(query: QueryGameCardsDto) -> JsonCommandResult {
    let QueryGameCardsArgs {
        search_query,
        selected_libraries,
        selected_launchers,
        sort_field,
        sort_direction,
        limit,
        offset,
    } = query.into_desktop_args()?;

    run_desktop_command(move || {
        desktop::query_game_cards(
            search_query,
            selected_libraries,
            selected_launchers,
            sort_field,
            sort_direction,
            limit,
            offset,
        )
    })
    .await
}

#[tauri::command]
pub async fn get_game_details(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;

    run_desktop_command(move || desktop::get_game_details(game_id)).await
}

#[tauri::command]
pub async fn fetch_game_cover(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;

    run_desktop_command(move || desktop::fetch_game_cover(game_id)).await
}

#[tauri::command]
pub async fn clear_game_cover(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;

    run_desktop_command(move || desktop::clear_game_cover(game_id)).await
}

#[tauri::command]
pub async fn set_game_cover(game_id: String, source_path: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let source_path = require_non_empty_string("source_path", source_path)?;

    run_desktop_command(move || desktop::set_game_cover(game_id, source_path)).await
}

#[tauri::command]
pub async fn get_catalog_setting(key: String) -> JsonCommandResult {
    let key = require_non_empty_string("key", key)?;

    run_desktop_command(move || desktop::get_catalog_setting(key)).await
}

#[tauri::command]
pub async fn set_catalog_setting(key: String, value: String) -> JsonCommandResult {
    let key = require_non_empty_string("key", key)?;

    run_desktop_command(move || desktop::set_catalog_setting(key, value)).await
}

#[tauri::command]
pub async fn apply_swap(
    game_id: String,
    component_id: String,
    artifact_id: String,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let component_id = require_non_empty_string("component_id", component_id)?;
    let artifact_id = require_non_empty_string("artifact_id", artifact_id)?;

    run_desktop_command(move || desktop::apply_swap(game_id, component_id, artifact_id)).await
}

#[tauri::command]
pub async fn rollback_component(game_id: String, component_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let component_id = require_non_empty_string("component_id", component_id)?;

    run_desktop_command(move || desktop::rollback_component(game_id, component_id)).await
}

#[tauri::command]
pub async fn fetch_libraries_manifest() -> JsonCommandResult {
    run_desktop_async_command(desktop::fetch_libraries_manifest).await
}

#[tauri::command]
pub async fn get_libraries_manifest() -> JsonCommandResult {
    run_desktop_async_command(desktop::get_libraries_manifest).await
}

#[tauri::command]
pub async fn download_library(entry_id: String) -> JsonCommandResult {
    let entry_id = require_non_empty_string("entry_id", entry_id)?;

    run_desktop_async_command(move || desktop::download_library(entry_id)).await
}

#[tauri::command]
pub async fn delete_library(entry_id: String) -> JsonCommandResult {
    let entry_id = require_non_empty_string("entry_id", entry_id)?;

    run_desktop_async_command(move || desktop::delete_library(entry_id)).await
}

#[tauri::command]
pub async fn get_library_states() -> JsonCommandResult {
    run_desktop_async_command(desktop::get_library_states).await
}

// ---------------------------------------------------------------------------
// NVAPI / DLSS preset commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_nvapi_supported_settings(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    run_desktop_command(move || desktop::list_nvapi_supported_settings(game_id)).await
}

#[tauri::command]
pub async fn list_nvapi_setting_states(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    run_desktop_command(move || desktop::list_nvapi_setting_states(game_id)).await
}

#[tauri::command]
pub async fn list_game_executable_candidates(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    run_desktop_command(move || desktop::list_game_executable_candidates(game_id)).await
}

#[tauri::command]
pub async fn set_game_executable_override(
    game_id: String,
    absolute_path: String,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let absolute_path = require_non_empty_string("absolute_path", absolute_path)?;
    run_desktop_command(move || desktop::set_game_executable_override(game_id, absolute_path)).await
}

#[tauri::command]
pub async fn clear_game_executable_override(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    run_desktop_command(move || desktop::clear_game_executable_override(game_id)).await
}

#[tauri::command]
pub async fn get_nvapi_setting_state(game_id: String, setting_key: String) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    run_desktop_command(move || desktop::get_nvapi_setting_state(game_id, setting_key)).await
}

#[tauri::command]
pub async fn set_nvapi_setting_value(
    game_id: String,
    setting_key: String,
    value: String,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    let value = require_non_empty_string("value", value)?;
    run_desktop_command(move || desktop::set_nvapi_setting_value(game_id, setting_key, value)).await
}

#[tauri::command]
pub async fn revert_nvapi_setting(
    game_id: String,
    setting_key: String,
    target: String,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    let target = require_non_empty_string("target", target)?;
    run_desktop_command(move || desktop::revert_nvapi_setting(game_id, setting_key, target)).await
}

// ---------------------------------------------------------------------------
// DLSS indicator (system-wide overlay)
// ---------------------------------------------------------------------------

/// Reads whether the global NVIDIA DLSS indicator overlay is currently enabled.
#[tauri::command]
pub async fn get_dlss_indicator_state() -> JsonCommandResult {
    run_desktop_command(desktop::get_dlss_indicator_state).await
}

/// Enables or disables the global NVIDIA DLSS indicator overlay (requires admin).
#[tauri::command]
pub async fn set_dlss_indicator_enabled(enabled: bool) -> JsonCommandResult {
    run_desktop_command(move || desktop::set_dlss_indicator_enabled(enabled)).await
}

/// Returns the `AppInitializationState` snapshot computed at startup.
/// Synchronous: the state is already in managed memory, no I/O.
#[tauri::command]
pub fn get_app_initialization_state(
    state: tauri::State<'_, crate::AppInitializationState>,
) -> crate::AppInitializationState {
    *state.inner()
}

/// Relaunches the app elevated via `ShellExecuteW(verb="runas")` and exits this process.
/// Returns `CommandFailed` if the user declines the UAC prompt or policy blocks elevation;
/// the frontend shows a non-fatal toast in that case.
#[tauri::command]
pub async fn request_admin_relaunch(app: tauri::AppHandle) -> JsonCommandResult {
    #[cfg(windows)]
    {
        use crate::elevation::{attempt_self_relaunch_elevated, ElevationStartupDecision};
        match attempt_self_relaunch_elevated() {
            ElevationStartupDecision::Relaunched => {
                app.exit(0);
                Ok(serde_json::json!({ "relaunched": true }))
            }
            ElevationStartupDecision::UserCancelled => Err(CommandError::from(
                CliError::CommandFailed("UAC consent was declined".to_owned()),
            )),
            ElevationStartupDecision::PolicyBlocked(code) => {
                Err(CommandError::from(CliError::CommandFailed(format!(
                    "OS denied the elevation request (ShellExecute code {code})"
                ))))
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err(CommandError::from(CliError::CommandFailed(
            "administrator relaunch is only supported on Windows".to_owned(),
        )))
    }
}
