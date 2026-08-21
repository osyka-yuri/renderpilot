//! Tauri commands for acquiring fresh file-safety contexts.
//!
//! The handlers deliberately bypass the generation-keyed Game Details cache.

use std::sync::Arc;

use renderpilot_api as desktop;
use renderpilot_orchestration::Context;

use super::{CommandBoundary, JsonCommandResult, require_game_context};
use crate::diagnostic_event::CommandOperation;

/// Acquires a fresh, uncached game-file safety assessment.
#[tauri::command]
pub async fn get_game_file_safety_assessment(
    game_id: String,
    context: tauri::State<'_, Arc<Context>>,
) -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::GetGameFileSafetyAssessment);
    let (game_id, context) = require_game_context(&boundary, game_id, &context)?;

    boundary
        .run(move || desktop::get_game_file_safety_assessment(&context, game_id))
        .await
}

/// Acquires a fresh assessment for the process-wide shared Vulkan layer.
#[tauri::command]
pub async fn get_shared_vulkan_safety_assessment() -> JsonCommandResult {
    let boundary = CommandBoundary::new(CommandOperation::GetSharedVulkanSafetyAssessment);

    boundary
        .run(desktop::get_shared_vulkan_safety_assessment)
        .await
}
