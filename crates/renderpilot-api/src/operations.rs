use renderpilot_orchestration::catalog;

use super::utils::{parse_artifact_id, parse_component_id, parse_game_id, JsonResult};

/// Applies a swap using a caller-provided storage connection.
pub fn apply_swap(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
    component_id: impl Into<String>,
    artifact_id: impl Into<String>,
) -> JsonResult {
    let result = catalog::apply_swap(
        context,
        &parse_game_id(game_id.into())?,
        &parse_component_id(component_id.into())?,
        &parse_artifact_id(artifact_id.into())?,
    )?;

    serde_json::to_value(result).map_err(Into::into)
}

/// Rolls back a component by restoring its `.bak` sidecar using a caller-provided storage connection.
pub fn rollback_component(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
    component_id: impl Into<String>,
) -> JsonResult {
    let result = catalog::rollback_component(
        context,
        &parse_game_id(game_id.into())?,
        &parse_component_id(component_id.into())?,
    )?;

    serde_json::to_value(result).map_err(Into::into)
}
