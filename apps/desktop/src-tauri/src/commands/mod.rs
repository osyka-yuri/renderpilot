//! Tauri command handlers for the desktop frontend.
//!
//! Blocking catalog / filesystem work is dispatched via `run_desktop_command` to avoid
//! stalling the async runtime.

mod error;
mod query_game_cards;
mod validation;

pub use error::CommandError;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use renderpilot_api::{self as desktop, ApiError};
use renderpilot_orchestration::{Context, ServiceError};
use serde_json::Value;
use tauri::Emitter;

pub type JsonCommandResult = Result<Value, CommandError>;

type DesktopCommandResult = Result<Value, ApiError>;

use query_game_cards::{QueryGameCardsArgs, QueryGameCardsDto};
use validation::{require_non_empty_path, require_non_empty_string};

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
/// * A race on `last_ms` can only cause an extra emit — never a missed final.
fn download_progress_emitter(
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
pub async fn scan_manual_folder(
    path: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let path = require_non_empty_path(path)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::scan_manual_folder(&context, path)).await
}

#[tauri::command]
pub async fn scan_auto_libraries(context: tauri::State<'_, Arc<Context>>) -> JsonCommandResult {
    let context = Arc::clone(&context);
    run_desktop_command(move || desktop::scan_auto_libraries(&context)).await
}

#[tauri::command]
pub async fn query_game_cards(
    query: QueryGameCardsDto,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let QueryGameCardsArgs {
        search_query,
        selected_libraries,
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
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::get_game_details(&context, game_id)).await
}

#[tauri::command]
pub async fn fetch_game_cover(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::fetch_game_cover(&context, game_id)).await
}

#[tauri::command]
pub async fn clear_game_cover(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::clear_game_cover(&context, game_id)).await
}

#[tauri::command]
pub async fn set_game_cover(
    game_id: String,
    source_path: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let source_path = require_non_empty_string("source_path", source_path)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::set_game_cover(&context, game_id, source_path)).await
}

#[tauri::command]
pub async fn set_game_favorite(
    game_id: String,
    is_favorite: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::set_game_favorite(&context, game_id, is_favorite)).await
}

#[tauri::command]
pub async fn set_game_hidden(
    game_id: String,
    is_hidden: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

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
    let game_id = require_non_empty_string("game_id", game_id)?;
    let component_id = require_non_empty_string("component_id", component_id)?;
    let artifact_id = require_non_empty_string("artifact_id", artifact_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::apply_swap(&context, game_id, component_id, artifact_id))
        .await
}

#[tauri::command]
pub async fn rollback_component(
    game_id: String,
    component_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let component_id = require_non_empty_string("component_id", component_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::rollback_component(&context, game_id, component_id)).await
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
pub async fn download_library(
    app: tauri::AppHandle,
    entry_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let entry_id = require_non_empty_string("entry_id", entry_id)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        let emit = download_progress_emitter(app, entry_id.clone());
        desktop::download_library(
            &context,
            entry_id,
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
pub async fn delete_library(
    entry_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let entry_id = require_non_empty_string("entry_id", entry_id)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(
        move || async move { desktop::delete_library(&context, entry_id).await },
    )
    .await
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
pub async fn list_nvapi_setting_states(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);
    run_desktop_command(move || desktop::list_nvapi_setting_states(&context, game_id)).await
}

#[tauri::command]
pub async fn list_game_executable_candidates(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);
    run_desktop_command(move || desktop::list_game_executable_candidates(&context, game_id)).await
}

#[tauri::command]
pub async fn resolve_game_executable(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);
    run_desktop_command(move || desktop::resolve_game_executable(&context, game_id)).await
}

#[tauri::command]
pub async fn set_game_executable_override(
    game_id: String,
    absolute_path: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let absolute_path = require_non_empty_string("absolute_path", absolute_path)?;
    let context = Arc::clone(&context);
    run_desktop_command(move || {
        desktop::set_game_executable_override(&context, game_id, &absolute_path)
    })
    .await
}

#[tauri::command]
pub async fn clear_game_executable_override(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);
    run_desktop_command(move || desktop::clear_game_executable_override(&context, game_id)).await
}

#[tauri::command]
pub async fn get_nvapi_setting_state(
    game_id: String,
    setting_key: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    let context = Arc::clone(&context);
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
    let game_id = require_non_empty_string("game_id", game_id)?;
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    let value = require_non_empty_string("value", value)?;
    let context = Arc::clone(&context);
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
    let game_id = require_non_empty_string("game_id", game_id)?;
    let setting_key = require_non_empty_string("setting_key", setting_key)?;
    let target = require_non_empty_string("target", target)?;
    let context = Arc::clone(&context);
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

/// Returns the `AppInitializationState` snapshot computed at startup.
/// Synchronous: the state is already in managed memory, no I/O.
// `tauri::command` requires `State` parameters by value.
#[allow(clippy::needless_pass_by_value)]
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
        use crate::elevation::{ElevationStartupDecision, attempt_self_relaunch_elevated};
        match attempt_self_relaunch_elevated() {
            ElevationStartupDecision::Relaunched => {
                app.exit(0);
                Ok(serde_json::json!({ "relaunched": true }))
            }
            ElevationStartupDecision::UserCancelled => Err(CommandError::from(ApiError::Service(
                ServiceError::CommandFailed("UAC consent was declined".to_owned()),
            ))),
            ElevationStartupDecision::PolicyBlocked(code) => Err(CommandError::from(
                ApiError::Service(ServiceError::CommandFailed(format!(
                    "OS denied the elevation request (ShellExecute code {code})"
                ))),
            )),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err(CommandError::from(ApiError::Service(
            ServiceError::CommandFailed(
                "administrator relaunch is only supported on Windows".to_owned(),
            ),
        )))
    }
}

// ---------------------------------------------------------------------------
// RenoDX HDR add-on
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn renodx_status(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::renodx_status(&context, game_id)).await
}

#[tauri::command]
pub async fn renodx_availability(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        desktop::renodx_availability(&context, game_id).await
    })
    .await
}

#[tauri::command]
pub async fn renodx_install(
    app: tauri::AppHandle,
    game_id: String,
    reshade_channel: String,
    confirm_anticheat: bool,
    confirm_vulkan_layer: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let reshade_channel = require_non_empty_string("reshade_channel", reshade_channel)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        let emit = download_progress_emitter(app, game_id.clone());
        desktop::renodx_install(
            &context,
            game_id,
            reshade_channel,
            confirm_anticheat,
            confirm_vulkan_layer,
            Some(&emit as &desktop::ProgressObserver<'_>),
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn renodx_install_from_file(
    app: tauri::AppHandle,
    game_id: String,
    file_path: String,
    reshade_channel: String,
    confirm_anticheat: bool,
    confirm_vulkan_layer: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let file_path = require_non_empty_string("file_path", file_path)?;
    let reshade_channel = require_non_empty_string("reshade_channel", reshade_channel)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        let emit = download_progress_emitter(app, game_id.clone());
        desktop::renodx_install_from_file(
            &context,
            game_id,
            file_path,
            reshade_channel,
            confirm_anticheat,
            confirm_vulkan_layer,
            Some(&emit as &desktop::ProgressObserver<'_>),
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn renodx_switch_reshade_channel(
    app: tauri::AppHandle,
    game_id: String,
    reshade_channel: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let reshade_channel = require_non_empty_string("reshade_channel", reshade_channel)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        let emit = download_progress_emitter(app, game_id.clone());
        desktop::renodx_switch_reshade_channel(
            &context,
            game_id,
            reshade_channel,
            Some(&emit as &desktop::ProgressObserver<'_>),
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn renodx_uninstall(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::renodx_uninstall(&context, game_id)).await
}

#[tauri::command]
pub async fn renodx_vulkan_layer_status() -> JsonCommandResult {
    run_desktop_command(desktop::renodx_vulkan_layer_status).await
}

#[tauri::command]
pub async fn renodx_remove_vulkan_layer() -> JsonCommandResult {
    run_desktop_command(desktop::renodx_remove_vulkan_layer).await
}

#[tauri::command]
pub async fn renodx_check_update(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        desktop::renodx_check_update(&context, game_id).await
    })
    .await
}

#[tauri::command]
pub async fn renodx_update(
    app: tauri::AppHandle,
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move {
        let emit = download_progress_emitter(app, game_id.clone());
        desktop::renodx_update(
            &context,
            game_id,
            Some(&emit as &desktop::ProgressObserver<'_>),
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn renodx_check_updates(context: tauri::State<'_, Arc<Context>>) -> JsonCommandResult {
    let context = Arc::clone(&context);

    run_desktop_async_command(move || async move { desktop::renodx_check_updates(&context).await })
        .await
}

#[tauri::command]
pub async fn renodx_install_dlss_fix(
    context: tauri::State<'_, Arc<Context>>,
    app: tauri::AppHandle,
    game_id: String,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);
    let progress = download_progress_emitter(app, game_id.clone());

    run_desktop_async_command(move || async move {
        desktop::renodx_install_dlss_fix(&context, game_id, Some(&progress)).await
    })
    .await
}

#[tauri::command]
pub async fn renodx_uninstall_dlss_fix(
    context: tauri::State<'_, Arc<Context>>,
    game_id: String,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::renodx_uninstall_dlss_fix(&context, game_id)).await
}

#[tauri::command]
pub async fn renodx_dlss_fix_availability(
    context: tauri::State<'_, Arc<Context>>,
    game_id: String,
) -> JsonCommandResult {
    let game_id = require_non_empty_string("game_id", game_id)?;
    let context = Arc::clone(&context);

    run_desktop_command(move || desktop::renodx_dlss_fix_availability(&context, game_id)).await
}
