//! Tauri commands invoked from the desktop UI.
//!
//! Potentially blocking catalog work is executed on Tauri's blocking pool via
//! [`run_desktop_command`].

mod error;

pub use error::CommandError;

use std::path::PathBuf;

use renderpilot_cli::{desktop, CliError};
use serde_json::Value;

pub type JsonCommandResult = Result<Value, CommandError>;

type DesktopCommandResult = Result<Value, CliError>;

async fn run_desktop_command<F>(command: F) -> JsonCommandResult
where
    F: FnOnce() -> DesktopCommandResult + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(command)
        .await
        .map_err(CommandError::task_failed)?
        .map_err(CommandError::from)
}

fn require_non_empty_arg(name: &'static str, value: String) -> Result<String, CommandError> {
    let value = value.trim().to_owned();

    if value.is_empty() {
        return Err(CommandError::invalid_argument(name, "must not be empty"));
    }

    Ok(value)
}

fn require_non_empty_path(path: String) -> Result<PathBuf, CommandError> {
    if path.trim().is_empty() {
        return Err(CommandError::invalid_argument("path", "must not be empty"));
    }

    Ok(PathBuf::from(path))
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
pub async fn get_game_cards() -> JsonCommandResult {
    run_desktop_command(desktop::get_game_cards).await
}

#[tauri::command]
pub async fn get_game_details(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_arg("game_id", game_id)?;

    run_desktop_command(move || desktop::get_game_details(game_id)).await
}

#[tauri::command]
pub async fn fetch_game_cover(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_arg("game_id", game_id)?;

    run_desktop_command(move || desktop::fetch_game_cover(game_id)).await
}

#[tauri::command]
pub async fn clear_game_cover(game_id: String) -> JsonCommandResult {
    let game_id = require_non_empty_arg("game_id", game_id)?;

    run_desktop_command(move || desktop::clear_game_cover(game_id)).await
}

#[tauri::command]
pub async fn set_game_cover(game_id: String, source_path: String) -> JsonCommandResult {
    let game_id = require_non_empty_arg("game_id", game_id)?;
    let source_path = require_non_empty_arg("source_path", source_path)?;

    run_desktop_command(move || desktop::set_game_cover(game_id, source_path)).await
}

#[tauri::command]
pub async fn get_catalog_setting(key: String) -> JsonCommandResult {
    let key = require_non_empty_arg("key", key)?;

    run_desktop_command(move || desktop::get_catalog_setting(key)).await
}

#[tauri::command]
pub async fn set_catalog_setting(key: String, value: String) -> JsonCommandResult {
    let key = require_non_empty_arg("key", key)?;
    let value = require_non_empty_arg("value", value)?;

    run_desktop_command(move || desktop::set_catalog_setting(key, value)).await
}

#[tauri::command]
pub async fn build_swap_plan(
    game_id: String,
    component_id: String,
    artifact_id: String,
) -> JsonCommandResult {
    let game_id = require_non_empty_arg("game_id", game_id)?;
    let component_id = require_non_empty_arg("component_id", component_id)?;
    let artifact_id = require_non_empty_arg("artifact_id", artifact_id)?;

    run_desktop_command(move || desktop::build_swap_plan(game_id, component_id, artifact_id)).await
}

#[tauri::command]
pub async fn apply_operation_plan(
    operation_id: String,
    confirmation_token: String,
) -> JsonCommandResult {
    let operation_id = require_non_empty_arg("operation_id", operation_id)?;
    let confirmation_token = require_non_empty_arg("confirmation_token", confirmation_token)?;

    run_desktop_command(move || desktop::apply_operation_plan(operation_id, confirmation_token))
        .await
}

#[tauri::command]
pub async fn rollback_operation(operation_id: String) -> JsonCommandResult {
    let operation_id = require_non_empty_arg("operation_id", operation_id)?;

    run_desktop_command(move || desktop::rollback_operation(operation_id)).await
}
