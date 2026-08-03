//! RenoDX HDR add-on Tauri command handlers.
//!
//! Desktop IPC is the card/settings surface only. CLI status / bulk check-updates
//! stay on orchestration via the CLI crate.

use std::sync::Arc;

use renderpilot_api as desktop;
use renderpilot_orchestration::Context;

use super::validation::require_non_empty_string;
use super::{CommandBoundary, JsonCommandResult, download_progress_emitter, require_game_context};

#[tauri::command]
pub async fn renodx_availability(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_availability");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run_async(move || async move { desktop::renodx_availability(&context, game_id).await })
        .await
}

#[tauri::command]
pub async fn renodx_install(
    app: tauri::AppHandle,
    game_id: String,
    reshade_channel: String,
    confirm_anticheat: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_install");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let reshade_channel = require_non_empty_string(&boundary, "reshade_channel", reshade_channel)?;

    boundary
        .run_async(move || async move {
            let emit = download_progress_emitter(app, game_id.clone());
            desktop::renodx_install(
                &context,
                game_id,
                reshade_channel,
                confirm_anticheat,
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
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_install_from_file");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let file_path = require_non_empty_string(&boundary, "file_path", file_path)?;
    let reshade_channel = require_non_empty_string(&boundary, "reshade_channel", reshade_channel)?;

    boundary
        .run_async(move || async move {
            let emit = download_progress_emitter(app, game_id.clone());
            desktop::renodx_install_from_file(
                &context,
                game_id,
                file_path,
                reshade_channel,
                confirm_anticheat,
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
    let boundary = CommandBoundary::new("renodx_switch_reshade_channel");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let reshade_channel = require_non_empty_string(&boundary, "reshade_channel", reshade_channel)?;

    boundary
        .run_async(move || async move {
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
    let boundary = CommandBoundary::new("renodx_uninstall");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::renodx_uninstall(&context, game_id))
        .await
}

#[tauri::command]
pub async fn renodx_vulkan_layer_status() -> JsonCommandResult {
    CommandBoundary::new("renodx_vulkan_layer_status")
        .run(desktop::renodx_vulkan_layer_status)
        .await
}

#[tauri::command]
pub async fn renodx_vulkan_layer_management_status(
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_vulkan_layer_management_status");
    let context = Arc::clone(&context);

    boundary
        .run_async(
            move || async move { desktop::renodx_vulkan_layer_management_status(&context).await },
        )
        .await
}

#[tauri::command]
pub async fn renodx_apply_vulkan_layer(
    app: tauri::AppHandle,
    reshade_channel: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_apply_vulkan_layer");
    let reshade_channel = require_non_empty_string(&boundary, "reshade_channel", reshade_channel)?;
    let context = Arc::clone(&context);

    boundary
        .run_async(move || async move {
            let emit = download_progress_emitter(app, "renodx:vulkan_layer".to_owned());
            desktop::renodx_apply_vulkan_layer(
                &context,
                reshade_channel,
                Some(&emit as &desktop::ProgressObserver<'_>),
            )
            .await
        })
        .await
}

#[tauri::command]
pub async fn renodx_remove_vulkan_layer(
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_remove_vulkan_layer");
    let context = Arc::clone(&context);

    boundary
        .run(move || desktop::renodx_remove_vulkan_layer(&context))
        .await
}

#[tauri::command]
pub async fn renodx_check_update(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_check_update");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run_async(move || async move { desktop::renodx_check_update(&context, game_id).await })
        .await
}

#[tauri::command]
pub async fn renodx_update(
    app: tauri::AppHandle,
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_update");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run_async(move || async move {
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

// Parameter order for the DLSS-fix family is historically `context` first
// (stable IPC); do not reorder to match other commands without a wire migration.
#[tauri::command]
pub async fn renodx_install_dlss_fix(
    context: tauri::State<'_, Arc<Context>>,
    app: tauri::AppHandle,
    game_id: String,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_install_dlss_fix");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let progress = download_progress_emitter(app, game_id.clone());

    boundary
        .run_async(move || async move {
            desktop::renodx_install_dlss_fix(&context, game_id, Some(&progress)).await
        })
        .await
}

#[tauri::command]
pub async fn renodx_uninstall_dlss_fix(
    context: tauri::State<'_, Arc<Context>>,
    game_id: String,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_uninstall_dlss_fix");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::renodx_uninstall_dlss_fix(&context, game_id))
        .await
}

#[tauri::command]
pub async fn renodx_dlss_fix_availability(
    context: tauri::State<'_, Arc<Context>>,
    game_id: String,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new("renodx_dlss_fix_availability");
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::renodx_dlss_fix_availability(&context, game_id))
        .await
}
