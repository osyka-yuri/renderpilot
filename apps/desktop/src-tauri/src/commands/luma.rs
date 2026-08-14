//! Luma Framework add-on Tauri command handlers.
//!
//! Desktop IPC is the card surface only (`availability` + per-game mutations).
//! CLI status / bulk check-updates stay on orchestration via the CLI crate.

use std::sync::Arc;

use crate::diagnostic_event::CommandOperation;
use renderpilot_api as desktop;
use renderpilot_orchestration::Context;

use super::{CommandBoundary, JsonCommandResult, download_progress_emitter, require_game_context};

#[tauri::command]
pub async fn luma_availability(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::LumaAvailability);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run_async(move || async move { desktop::luma_availability(&context, game_id).await })
        .await
}

#[tauri::command]
pub async fn luma_install(
    app: tauri::AppHandle,
    game_id: String,
    confirm_anticheat: bool,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::LumaInstall);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run_async(move || async move {
            let emit = download_progress_emitter(app, game_id.clone());
            desktop::luma_install(
                &context,
                game_id,
                confirm_anticheat,
                Some(&emit as &desktop::ProgressObserver<'_>),
            )
            .await
        })
        .await
}

#[tauri::command]
pub async fn luma_uninstall(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::LumaUninstall);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::luma_uninstall(&context, game_id))
        .await
}

#[tauri::command]
pub async fn luma_check_update(
    game_id: String,
    deep: Option<bool>,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::LumaCheckUpdate);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let deep = deep.unwrap_or(false);

    boundary
        .run_async(move || async move { desktop::luma_check_update(&context, game_id, deep).await })
        .await
}

#[tauri::command]
pub async fn luma_update(
    app: tauri::AppHandle,
    game_id: String,
    force_full: Option<bool>,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::LumaUpdate);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let force_full = force_full.unwrap_or(false);

    boundary
        .run_async(move || async move {
            let emit = download_progress_emitter(app, game_id.clone());
            desktop::luma_update(
                &context,
                game_id,
                force_full,
                Some(&emit as &desktop::ProgressObserver<'_>),
            )
            .await
        })
        .await
}
