//! Tauri commands invoked from the desktop UI.
//!
//! Potentially blocking catalog work is executed on Tauri's blocking pool via
//! [`run_desktop_command`].

mod error;

pub use error::CommandError;

use std::path::PathBuf;

use renderpilot_cli::{desktop, CliError};
use serde::Deserialize;
use serde_json::Value;

pub type JsonCommandResult = Result<Value, CommandError>;

type DesktopCommandResult = Result<Value, CliError>;

const MAX_GAME_CARDS_PAGE_LIMIT: u32 = 10_000;

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

fn require_non_empty_string(
    name: &'static str,
    value: impl Into<String>,
) -> Result<String, CommandError> {
    let value = value.into().trim().to_owned();

    if value.is_empty() {
        return Err(CommandError::invalid_argument(name, "must not be empty"));
    }

    Ok(value)
}

fn require_non_empty_path(path: String) -> Result<PathBuf, CommandError> {
    let path = require_non_empty_string("path", path)?;

    Ok(PathBuf::from(path))
}

fn trim_string(value: String) -> String {
    value.trim().to_owned()
}

fn trim_string_vec(values: Vec<String>) -> Vec<String> {
    values.into_iter().map(trim_string).collect()
}

fn reject_empty_items(name: &'static str, values: &[String]) -> Result<(), CommandError> {
    if values.iter().any(|value| value.is_empty()) {
        return Err(CommandError::invalid_argument(
            name,
            "items must not be empty",
        ));
    }

    Ok(())
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GameCardsSortFieldDto {
    Title,
    Updates,
    Risk,
}

impl GameCardsSortFieldDto {
    fn as_cli_value(&self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Updates => "updates",
            Self::Risk => "risk",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameCardsSortDirectionDto {
    Asc,
    Desc,
}

impl GameCardsSortDirectionDto {
    fn as_cli_value(&self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameCardsSortDto {
    field: GameCardsSortFieldDto,
    direction: GameCardsSortDirectionDto,
}

impl GameCardsSortDto {
    fn into_cli_values(self) -> (String, String) {
        (
            self.field.as_cli_value().to_owned(),
            self.direction.as_cli_value().to_owned(),
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameCardsPageDto {
    limit: u32,
    offset: u32,
}

impl GameCardsPageDto {
    fn into_cli_values(self) -> Result<(i64, i64), CommandError> {
        if self.limit == 0 {
            return Err(CommandError::invalid_argument(
                "limit",
                "must be greater than 0",
            ));
        }

        if self.limit > MAX_GAME_CARDS_PAGE_LIMIT {
            return Err(CommandError::invalid_argument(
                "limit",
                "must not exceed maximum page size",
            ));
        }

        Ok((i64::from(self.limit), i64::from(self.offset)))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryGameCardsDto {
    #[serde(default)]
    search_query: String,

    #[serde(default)]
    selected_libraries: Vec<String>,

    #[serde(default)]
    selected_launchers: Vec<String>,

    sort: GameCardsSortDto,
    page: GameCardsPageDto,
}

struct QueryGameCardsArgs {
    search_query: String,
    selected_libraries: Vec<String>,
    selected_launchers: Vec<String>,
    sort_field: String,
    sort_direction: String,
    limit: i64,
    offset: i64,
}

impl QueryGameCardsDto {
    fn into_desktop_args(self) -> Result<QueryGameCardsArgs, CommandError> {
        let search_query = trim_string(self.search_query);
        let selected_libraries = trim_string_vec(self.selected_libraries);
        let selected_launchers = trim_string_vec(self.selected_launchers);

        reject_empty_items("selected_libraries", &selected_libraries)?;
        reject_empty_items("selected_launchers", &selected_launchers)?;

        let (sort_field, sort_direction) = self.sort.into_cli_values();
        let (limit, offset) = self.page.into_cli_values()?;

        Ok(QueryGameCardsArgs {
            search_query,
            selected_libraries,
            selected_launchers,
            sort_field,
            sort_direction,
            limit,
            offset,
        })
    }
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
pub async fn build_swap_plan(
    game_id: String,
    component_id: String,
    artifact_id: String,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let component_id = require_non_empty_string("component_id", component_id)?;
    let artifact_id = require_non_empty_string("artifact_id", artifact_id)?;

    run_desktop_command(move || desktop::build_swap_plan(game_id, component_id, artifact_id)).await
}

#[tauri::command]
pub async fn apply_operation_plan(
    operation_id: String,
    confirmation_token: String,
) -> JsonCommandResult {
    let operation_id = require_non_empty_string("operation_id", operation_id)?;
    let confirmation_token = require_non_empty_string("confirmation_token", confirmation_token)?;

    run_desktop_command(move || desktop::apply_operation_plan(operation_id, confirmation_token))
        .await
}

#[tauri::command]
pub async fn rollback_operation(operation_id: String) -> JsonCommandResult {
    let operation_id = require_non_empty_string("operation_id", operation_id)?;

    run_desktop_command(move || desktop::rollback_operation(operation_id)).await
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
