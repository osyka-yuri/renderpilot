//! Tauri command handlers for the desktop frontend.
//!
//! Blocking catalog / filesystem work is dispatched through `CommandBoundary`
//! to avoid stalling the async runtime and to record mapped failures once.

pub(crate) mod addon_catalog;
mod app_update;
mod background_catalog_refresh;
mod error;
mod luma;
mod nvapi;
mod query_game_cards;
mod renodx;
mod validation;

pub use app_update::*;
pub use error::CommandError;
pub use luma::*;
pub use nvapi::*;
pub use renodx::*;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::{
    backend_diagnostics,
    diagnostic_event::{
        BackendDiagnosticEvent, CapabilityOperation, CommandOperation, CoverGcOperation,
    },
};
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
    boundary: &CommandBoundary,
    game_id: String,
    context: &tauri::State<'_, Arc<Context>>,
) -> Result<(String, Arc<Context>), CommandError> {
    Ok((
        require_non_empty_string(boundary, "game_id", game_id)?,
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct CommandBoundary {
    operation: CommandOperation,
}

impl CommandBoundary {
    pub(crate) const fn new(operation: CommandOperation) -> Self {
        Self { operation }
    }

    pub(crate) fn record(self, error: CommandError) -> CommandError {
        error.recorded(self.operation)
    }

    fn require_portable_commit(self) -> Result<(), CommandError> {
        #[cfg(all(windows, feature = "portable"))]
        crate::portable_runtime::activation::require_committed().map_err(|error| {
            self.record(CommandError::with_diagnostic(
                error::CommandErrorKind::CommandFailed,
                error,
            ))
        })?;
        Ok(())
    }

    pub(crate) async fn run_output<T, F>(self, command: F) -> Result<T, CommandError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, ApiError> + Send + 'static,
    {
        self.require_portable_commit()?;
        tauri::async_runtime::spawn_blocking(command)
            .await
            .map_err(|error| self.record(CommandError::task_failed(error)))?
            .map_err(|error| self.record(CommandError::from(error)))
    }

    pub(crate) async fn run<F>(self, command: F) -> JsonCommandResult
    where
        F: FnOnce() -> DesktopCommandResult + Send + 'static,
    {
        self.run_output(command).await
    }

    pub(crate) async fn run_async<F, Fut>(self, command: F) -> JsonCommandResult
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = DesktopCommandResult> + Send + 'static,
    {
        self.require_portable_commit()?;
        tauri::async_runtime::spawn(command())
            .await
            .map_err(|error| self.record(CommandError::task_failed(error)))?
            .map_err(|error| self.record(CommandError::from(error)))
    }
}

#[tauri::command]
pub async fn inspect_game_install(
    path: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::InspectGameInstall);
    let path = require_non_empty_path(&boundary, path)?;
    let context = Arc::clone(&context);

    boundary
        .run(move || desktop::inspect_game_install(&context, &path))
        .await
}

#[tauri::command]
pub async fn add_game(
    selected_root: String,
    root_choice: String,
    allow_root_correction: bool,
    chosen_executable: Option<String>,
    inspection_fingerprint: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::AddGame);
    let selected_root = require_non_empty_path(&boundary, selected_root)?;
    let chosen_executable = chosen_executable
        .map(|path| require_non_empty_path(&boundary, path))
        .transpose()?;
    let inspection_fingerprint =
        require_non_empty_string(&boundary, "inspection_fingerprint", inspection_fingerprint)?;
    let context = Arc::clone(&context);

    boundary
        .run(move || {
            desktop::add_game(
                &context,
                selected_root,
                &root_choice,
                allow_root_correction,
                chosen_executable,
                inspection_fingerprint,
            )
        })
        .await
}

#[tauri::command]
pub async fn remove_game_from_catalog(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::RemoveGameFromCatalog);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    boundary
        .run(move || desktop::remove_game_from_catalog(&context, game_id))
        .await
}

#[tauri::command]
pub async fn scan_auto_libraries(context: tauri::State<'_, Arc<Context>>) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::ScanAutoLibraries);
    let context = Arc::clone(&context);

    boundary
        .run(move || desktop::scan_auto_libraries(&context))
        .await
}

/// Force-refreshes all remote CDN manifests (libraries, RenoDX, Luma, ReShade).
///
/// Subject to a process-local cooldown / single-flight gate. Best-effort: the
/// report encodes per-kind failures; the command itself succeeds so shell
/// Refresh can still scan the disk. Capability projection refresh is an
/// explicit, separate command so scan/manifest transport cannot publish a
/// duplicate catalog update behind the caller's atomic refresh.
#[tauri::command]
pub async fn refresh_remote_manifests(
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::RefreshRemoteManifests);
    let output = tauri::async_runtime::spawn(desktop::refresh_remote_manifests_forced_output())
        .await
        .map_err(|error| boundary.record(CommandError::task_failed(error)))?
        .map_err(|error| boundary.record(CommandError::from(error)))?;
    if output.library_catalog_refreshed {
        // The library catalog is an authoritative atomic file rather than a
        // SQLite table, so its revision must invalidate the card projection
        // explicitly. The caller still performs exactly one query after the
        // separate scan + capabilities phases finish.
        context.invalidate_catalog_snapshot();
    }
    Ok(output.json)
}

/// Rebuilds the durable add-on capability projection independently of scan.
/// The caller performs one catalog query only after all requested refresh
/// phases have completed.
#[tauri::command]
pub async fn refresh_catalog_capabilities(
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let refreshed = match addon_catalog::refresh_catalog_addon_capabilities(Arc::clone(&context))
        .await
    {
        Ok(refreshed) => refreshed,
        Err(error) => {
            log::warn!(
                "Desktop command warning [operation=refresh_catalog_capabilities code=capability_refresh_failed]: {error}"
            );
            backend_diagnostics::record(BackendDiagnosticEvent::capability_failure(
                CapabilityOperation::RefreshCatalogCapabilities,
            ));
            false
        }
    };
    Ok(serde_json::json!({ "refreshed": refreshed }))
}

#[tauri::command]
pub async fn query_game_cards(
    query: QueryGameCardsDto,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::QueryGameCards);
    let QueryGameCardsArgs {
        search_query,
        selected_libraries,
        selected_addons,
        selected_launchers,
        launcher_order,
        show_hidden,
        favorites_only,
        sort_field,
        sort_direction,
        limit,
        offset,
    } = query.into_desktop_args(&boundary)?;
    let context = Arc::clone(&context);

    boundary
        .run(move || {
            desktop::query_game_cards(
                &context,
                desktop::QueryGameCardsRequest {
                    search_query,
                    selected_libraries,
                    selected_addons,
                    selected_launchers,
                    launcher_order,
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
pub async fn bootstrap_games_catalog(
    limit: u32,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::BootstrapGamesCatalog);
    if limit == 0 || limit > 10_000 {
        return Err(boundary.record(CommandError::invalid_argument(
            "limit",
            "must be between 1 and 10000",
        )));
    }
    let context = Arc::clone(&context);
    boundary
        .run(move || desktop::bootstrap_games_catalog(&context, i64::from(limit)))
        .await
}

/// Starts non-critical catalog maintenance after the frontend's first catalog paint.
#[tauri::command]
pub async fn start_background_refresh(
    context: tauri::State<'_, Arc<Context>>,
    app: tauri::AppHandle,
) -> JsonCommandResult {
    let refresh = background_catalog_refresh::start(Arc::clone(&context), app).await;
    Ok(serde_json::json!({
        "started": refresh.started,
        "partialFailureCount": refresh.partial_failure_count,
    }))
}

#[tauri::command]
pub async fn get_game_details(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::GetGameDetails);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::get_game_details(&context, game_id))
        .await
}

#[tauri::command]
pub async fn fetch_game_cover(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::FetchGameCover);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::fetch_game_cover(&context, game_id))
        .await
}

#[tauri::command]
pub async fn clear_game_cover(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::ClearGameCover);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    let output = boundary
        .run_output(move || desktop::clear_game_cover_with_observation(&context, game_id))
        .await?;
    if let Some(error) = output.cleanup_issue {
        log::warn!(
            "Desktop command warning [operation=clear_game_cover code=orphan_cleanup_failed]: {error}"
        );
        backend_diagnostics::record(BackendDiagnosticEvent::cover_gc_failure(
            CoverGcOperation::ClearGameCover,
        ));
    }
    Ok(output.json)
}

#[tauri::command]
pub async fn set_game_cover(
    game_id: String,
    source_path: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::SetGameCover);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let source_path = require_non_empty_string(&boundary, "source_path", source_path)?;

    boundary
        .run(move || desktop::set_game_cover(&context, game_id, source_path))
        .await
}

#[tauri::command]
pub async fn set_game_favorite(
    game_id: String,
    is_favorite: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::SetGameFavorite);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::set_game_favorite(&context, game_id, is_favorite))
        .await
}

#[tauri::command]
pub async fn set_game_hidden(
    game_id: String,
    is_hidden: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::SetGameHidden);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::set_game_hidden(&context, game_id, is_hidden))
        .await
}

#[tauri::command]
pub async fn get_catalog_setting(
    key: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::GetCatalogSetting);
    let key = require_non_empty_string(&boundary, "key", key)?;
    let context = Arc::clone(&context);

    boundary
        .run(move || desktop::get_catalog_setting(&context, key))
        .await
}

#[tauri::command]
pub async fn set_catalog_setting(
    key: String,
    value: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::SetCatalogSetting);
    let key = require_non_empty_string(&boundary, "key", key)?;
    let context = Arc::clone(&context);

    boundary
        .run(move || desktop::set_catalog_setting(&context, key, value))
        .await
}

#[tauri::command]
pub async fn apply_swap(
    game_id: String,
    component_id: String,
    artifact_id: String,
    confirmation_token: Option<String>,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::ApplySwap);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let component_id = require_non_empty_string(&boundary, "component_id", component_id)?;
    let artifact_id = require_non_empty_string(&boundary, "artifact_id", artifact_id)?;

    boundary
        .run(move || {
            desktop::apply_swap(
                &context,
                game_id,
                component_id,
                artifact_id,
                confirmation_token.as_deref(),
            )
        })
        .await
}

#[tauri::command]
pub async fn plan_swap(
    game_id: String,
    component_id: String,
    artifact_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::PlanSwap);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let component_id = require_non_empty_string(&boundary, "component_id", component_id)?;
    let artifact_id = require_non_empty_string(&boundary, "artifact_id", artifact_id)?;
    boundary
        .run(move || desktop::plan_swap(&context, game_id, component_id, artifact_id))
        .await
}

#[tauri::command]
pub async fn rollback_component(
    game_id: String,
    component_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::RollbackComponent);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let component_id = require_non_empty_string(&boundary, "component_id", component_id)?;

    boundary
        .run(move || desktop::rollback_component(&context, game_id, component_id))
        .await
}

#[tauri::command]
pub async fn plan_rollback(
    game_id: String,
    component_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::PlanRollback);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let component_id = require_non_empty_string(&boundary, "component_id", component_id)?;
    boundary
        .run(move || desktop::plan_rollback(&context, game_id, component_id))
        .await
}

#[tauri::command]
pub async fn list_library_packages(context: tauri::State<'_, Arc<Context>>) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::ListLibraryPackages);
    let context = Arc::clone(&context);
    boundary
        .run_async(move || async move { desktop::list_library_packages(&context).await })
        .await
}

#[tauri::command]
pub async fn download_library_package(
    app: tauri::AppHandle,
    package_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::DownloadLibraryPackage);
    let package_id = require_non_empty_string(&boundary, "package_id", package_id)?;
    let context = Arc::clone(&context);

    boundary
        .run_async(move || async move {
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
    let boundary = CommandBoundary::new(CommandOperation::DownloadArtifact);
    let artifact_id = require_non_empty_string(&boundary, "artifact_id", artifact_id)?;
    let context = Arc::clone(&context);

    boundary
        .run_async(move || async move {
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
    let boundary = CommandBoundary::new(CommandOperation::DeleteLibraryPackage);
    let package_id = require_non_empty_string(&boundary, "package_id", package_id)?;
    let context = Arc::clone(&context);

    boundary
        .run_async(
            move || async move { desktop::delete_library_package(&context, package_id).await },
        )
        .await
}
