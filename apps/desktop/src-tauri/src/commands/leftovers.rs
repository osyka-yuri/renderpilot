//! Retired-game proposals and explicit cleanup, using the normal IPC boundary.

use std::sync::Arc;

use renderpilot_orchestration::Context;

use crate::diagnostic_event::CommandOperation;

use super::{
    CommandBoundary, JsonCommandResult, require_game_context, validation::require_non_empty_string,
};

#[tauri::command]
pub async fn list_retired_game_leftovers(
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::ListRetiredGameLeftovers);
    let context = Arc::clone(&context);
    boundary
        .run(move || renderpilot_api::list_retired_game_leftovers(&context))
        .await
}

#[tauri::command]
pub async fn clean_retired_game_leftovers(
    game_id: String,
    intent: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::CleanRetiredGameLeftovers);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let intent = require_non_empty_string(&boundary, "intent", intent)?;
    boundary
        .run(move || renderpilot_api::clean_retired_game_leftovers(&context, game_id, intent))
        .await
}

#[tauri::command]
pub async fn leave_retired_game_leftovers(
    game_id: String,
    intent: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::LeaveRetiredGameLeftovers);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;
    let intent = require_non_empty_string(&boundary, "intent", intent)?;
    boundary
        .run(move || renderpilot_api::leave_retired_game_leftovers(&context, game_id, intent))
        .await
}
