//! RenoDX HDR add-on Tauri command handlers.
//!
//! Desktop IPC is the card/settings surface only. CLI status / bulk check-updates
//! stay on orchestration via the CLI crate.

use std::sync::Arc;

use crate::diagnostic_event::CommandOperation;
use renderpilot_api as desktop;
use renderpilot_orchestration::Context;

use super::validation::require_non_empty_string;
use super::{CommandBoundary, JsonCommandResult, download_progress_emitter, require_game_context};

#[tauri::command]
pub async fn renodx_availability(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::RenodxAvailability);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxInstall);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxInstallFromFile);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxSwitchReshadeChannel);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxUninstall);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::renodx_uninstall(&context, game_id))
        .await
}

#[tauri::command]
pub async fn renodx_vulkan_layer_status() -> JsonCommandResult {
    CommandBoundary::new(CommandOperation::RenodxVulkanLayerStatus)
        .run(desktop::renodx_vulkan_layer_status)
        .await
}

#[tauri::command]
pub async fn renodx_vulkan_layer_management_status(
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::RenodxVulkanLayerManagementStatus);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxApplyVulkanLayer);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxRemoveVulkanLayer);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxCheckUpdate);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxUpdate);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxInstallDlssFix);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let progress = download_progress_emitter(app, game_id.clone());

    boundary
        .run_async(move || async move {
            desktop::renodx_install_dlss_fix(&context, game_id, Some(&progress)).await
        })
        .await
}

#[tauri::command]
pub async fn renodx_update_dlss_fix(
    context: tauri::State<'_, Arc<Context>>,
    app: tauri::AppHandle,
    game_id: String,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::RenodxUpdateDlssFix);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let progress = download_progress_emitter(app, game_id.clone());

    boundary
        .run_async(move || async move {
            desktop::renodx_update_dlss_fix(&context, game_id, Some(&progress)).await
        })
        .await
}

#[tauri::command]
pub async fn renodx_retry_dlss_fix_recovery(
    context: tauri::State<'_, Arc<Context>>,
    game_id: String,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::RenodxRetryDlssFixRecovery);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::renodx_retry_dlss_fix_recovery(&context, game_id))
        .await
}

#[tauri::command]
pub async fn renodx_uninstall_dlss_fix(
    context: tauri::State<'_, Arc<Context>>,
    game_id: String,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::RenodxUninstallDlssFix);
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
    let boundary = CommandBoundary::new(CommandOperation::RenodxDlssFixAvailability);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::renodx_dlss_fix_availability(&context, game_id))
        .await
}
