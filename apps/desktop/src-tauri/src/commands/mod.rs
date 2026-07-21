//! Tauri command handlers for the desktop frontend.
//!
//! Blocking catalog / filesystem work is dispatched via `run_desktop_command` to avoid
//! stalling the async runtime.

pub(crate) mod addon_catalog;
mod app;
mod error;
mod luma;
mod nvapi;
mod query_game_cards;
mod renodx;
mod validation;

pub use app::*;
pub use error::CommandError;
pub use luma::*;
pub use nvapi::*;
pub use renodx::*;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use renderpilot_api::{self as desktop, ApiError};
use renderpilot_orchestration::Context;
use serde_json::Value;
use tauri::Emitter;

pub type JsonCommandResult = Result<Value, CommandError>;

type DesktopCommandResult = Result<Value, ApiError>;

use query_game_cards::{QueryGameCardsArgs, QueryGameCardsDto};
use validation::{require_non_empty_path, require_non_empty_string};

/// Validates `game_id` and clones the managed orchestration context for a command.
pub(crate) fn require_game_context(
    game_id: String,
    context: &tauri::State<'_, Arc<Context>>,
) -> Result<(String, Arc<Context>), CommandError> {
    Ok((
        require_non_empty_string("game_id", game_id)?,
        Arc::clone(context),
    ))
}

// ---------------------------------------------------------------------------
// Download-progress event contract
// ---------------------------------------------------------------------------

const DOWNLOAD_PROGRESS_EVENT: &str = "download-progress";
const DOWNLOAD_PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(100);

#[derive(serde::Serialize, Clone)]
struct DownloadProgressEvent<'a> {
    id: &'a str,
    downloaded: u64,
    total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<&'a str>,
}

/// Creates a `ProgressObserver` closure that emits `download-progress` Tauri
/// events with source-side throttling.
///
/// * The **first** emit (downloaded == 0) and the **final** emit
///   (downloaded >= total) always pass through regardless of the interval.
/// * Intermediate emits are skipped when less than
///   `DOWNLOAD_PROGRESS_MIN_INTERVAL` has elapsed since the last one.
/// * A race on `last_ms` can only cause an extra emit -- never a missed final.
pub(crate) fn download_progress_emitter(
    app: tauri::AppHandle,
    id: String,
) -> impl Fn(desktop::DownloadProgress<'_>) + Send + Sync {
    let epoch = Instant::now();
    let last_ms = AtomicU64::new(u64::MAX); // sentinel: "never emitted"

    move |progress: desktop::DownloadProgress<'_>| {
        let is_final = progress.downloaded_bytes >= progress.total_bytes;
        let is_first = last_ms.load(Ordering::Relaxed) == u64::MAX;

        if !is_first && !is_final {
            let now_ms = epoch.elapsed().as_millis() as u64;
            let prev_ms = last_ms.load(Ordering::Relaxed);
            if now_ms.saturating_sub(prev_ms) < DOWNLOAD_PROGRESS_MIN_INTERVAL.as_millis() as u64 {
                return;
            }
        }

        let now_ms = epoch.elapsed().as_millis() as u64;
        last_ms.store(now_ms, Ordering::Relaxed);

        let _ = app.emit(
            DOWNLOAD_PROGRESS_EVENT,
            DownloadProgressEvent {
                id: &id,
                downloaded: progress.downloaded_bytes,
                total: progress.total_bytes,
                phase: progress.phase,
            },
        );
    }
}

pub(crate) async fn run_desktop_command<F>(command: F) -> JsonCommandResult
where
    F: FnOnce() -> DesktopCommandResult + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(command)
        .await
        .map_err(CommandError::task_failed)?
        .map_err(CommandError::from)
}

pub(crate) async fn run_desktop_async_command<F, Fut>(command: F) -> JsonCommandResult
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
pub async fn scan_manual_folder(
    path: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let path = require_non_empty_path(path)?;
    let context = Arc::clone(&context);
    let scan_context = Arc::clone(&context);

    let result =
        run_desktop_command(move || desktop::scan_manual_folder(&scan_context, path)).await;
    if result.is_ok() {
        addon_catalog::refresh_catalog_addon_capabilities(context).await;
    }
    result
}

#[tauri::command]
pub async fn scan_auto_libraries(context: tauri::State<'_, Arc<Context>>) -> JsonCommandResult {
    let context = Arc::clone(&context);
    let scan_context = Arc::clone(&context);

    let result = run_desktop_command(move || desktop::scan_auto_libraries(&scan_context)).await;
    if result.is_ok() {
        addon_catalog::refresh_catalog_addon_capabilities(context).await;
    }
    result
}

/// Force-refreshes all remote CDN manifests (libraries, RenoDX, Luma, ReShade).
///
/// Subject to a process-local cooldown / single-flight gate. Best-effort: the
/// report encodes per-kind failures; the command itself succeeds so shell
/// Refresh can still scan the disk. After a successful command result, rebuilds
/// catalog add-on capability flags from the (possibly just-warmed) cache --
/// including cooldown skips, which still re-read local manifests.
#[tauri::command]
pub async fn refresh_remote_manifests(
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let context = Arc::clone(&context);
    let result = run_desktop_async_command(desktop::refresh_remote_manifests_forced).await;
    if result.is_ok() {
        addon_catalog::refresh_catalog_addon_capabilities(context).await;
    }
    result
}

#[tauri::command]
pub async fn query_game_cards(
    query: QueryGameCardsDto,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let QueryGameCardsArgs {
        search_query,
        selected_libraries,
        selected_addons,
        selected_launchers,
        show_hidden,
        favorites_only,
        sort_field,
        sort_direction,
        limit,
        offset,
    } = query.into_desktop_args()?;
    let context = Arc::clone(&context);

    run_desktop_command(move || {
        desktop::query_game_cards(
            &context,
            desktop::QueryGameCardsRequest {
                search_query,
                selected_libraries,
                selected_addons,
                selected_launchers,
                show_hidden,
                favorites_only,
                sort_field,
                sort_direction,
                page_limit: limit,
                page_offset: offset,
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn get_game_details(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;

    run_desktop_command(move || desktop::get_game_details(&context, game_id)).await
}

#[tauri::command]
pub async fn fetch_game_cover(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;

    run_desktop_command(move || desktop::fetch_game_cover(&context, game_id)).await
}

#[tauri::command]
pub async fn clear_game_cover(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;

    run_desktop_command(move || desktop::clear_game_cover(&context, game_id)).await
}

#[tauri::command]
pub async fn set_game_cover(
    game_id: String,
    source_path: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    let source_path = require_non_empty_string("source_path", source_path)?;

    run_desktop_command(move || desktop::set_game_cover(&context, game_id, source_path)).await
}

#[tauri::command]
pub async fn set_game_favorite(
    game_id: String,
    is_favorite: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;

    run_desktop_command(move || desktop::set_game_favorite(&context, game_id, is_favorite)).await
}

#[tauri::command]
pub async fn set_game_hidden(
    game_id: String,
    is_hidden: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;

    run_desktop_command(move || desktop::set_game_hidden(&context, game_id, is_hidden)).await
}

#[tauri::command]
pub async fn get_catalog_setting(
    key: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let key = require_non_empty_string("key", key)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::get_catalog_setting(&context, key)).await
}

#[tauri::command]
pub async fn set_catalog_setting(
    key: String,
    value: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let key = require_non_empty_string("key", key)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::set_catalog_setting(&context, key, value)).await
}

#[tauri::command]
pub async fn apply_swap(
    game_id: String,
    component_id: String,
    artifact_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    let component_id = require_non_empty_string("component_id", component_id)?;
    let artifact_id = require_non_empty_string("artifact_id", artifact_id)?;

    run_desktop_command(move || desktop::apply_swap(&context, game_id, component_id, artifact_id))
        .await
}

#[tauri::command]
pub async fn rollback_component(
    game_id: String,
    component_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let (game_id, context) = require_game_context(game_id, &context)?;
    let component_id = require_non_empty_string("component_id", component_id)?;

    run_desktop_command(move || desktop::rollback_component(&context, game_id, component_id)).await
}

#[tauri::command]
pub async fn list_library_packages(context: tauri::State<'_, Arc<Context>>) -> JsonCommandResult {
    let context = Arc::clone(&context);
    run_desktop_async_command(move || async move { desktop::list_library_packages(&context).await })
        .await
}

#[tauri::command]
pub async fn download_library_package(
    app: tauri::AppHandle,
    package_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let package_id = require_non_empty_string("package_id", package_id)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        let emit = download_progress_emitter(app, package_id.clone());
        desktop::download_library_package(
            &context,
            package_id,
            Some(&emit as &desktop::ProgressObserver<'_>),
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn download_artifact(
    app: tauri::AppHandle,
    artifact_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let artifact_id = require_non_empty_string("artifact_id", artifact_id)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        let emit = download_progress_emitter(app, artifact_id.clone());
        desktop::download_artifact(
            &context,
            artifact_id,
            Some(&emit as &desktop::ProgressObserver<'_>),
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn delete_library_package(
    package_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let package_id = require_non_empty_string("package_id", package_id)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        desktop::delete_library_package(&context, package_id).await
    })
    .await
}
