//! Tauri commands invoked from the desktop UI.
//!
//! Potentially blocking catalog work is executed on Tauri's blocking pool via
//! [`run_desktop_command`].

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
