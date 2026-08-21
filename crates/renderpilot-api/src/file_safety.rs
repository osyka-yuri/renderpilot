//! GUI-facing acquisition facade for fresh file-safety contexts.

use renderpilot_orchestration::{Context, FileSafetyAuthority};

use crate::utils::{JsonResult, parse_game_id, to_json};

/// Acquires a fresh, uncached game-file safety assessment.
pub fn get_game_file_safety_assessment(
    context: &Context,
    game_id: impl Into<String>,
) -> JsonResult {
    let game_id = parse_game_id(game_id)?;
    to_json(FileSafetyAuthority::new().issue_game_assessment(context, &game_id)?)
}

/// Acquires a fresh assessment for the process-wide shared Vulkan layer.
pub fn get_shared_vulkan_safety_assessment() -> JsonResult {
    to_json(FileSafetyAuthority::new().issue_shared_vulkan_assessment()?)
}
