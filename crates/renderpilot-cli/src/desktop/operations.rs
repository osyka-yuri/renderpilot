use super::utils::{parse_artifact_id, parse_component_id, parse_game_id, JsonResult};
use crate::catalog::execute;

/// Applies a swap directly without persisting an operation journal entry.
pub fn apply_swap(
    game_id: impl Into<String>,
    component_id: impl Into<String>,
    artifact_id: impl Into<String>,
) -> JsonResult {
    let result = execute::apply_swap(
        parse_game_id(game_id.into())?,
        parse_component_id(component_id.into())?,
        parse_artifact_id(artifact_id.into())?,
    )?;

    serde_json::to_value(result).map_err(Into::into)
}

/// Rolls back a component by restoring its `.bak` sidecar.
pub fn rollback_component(
    game_id: impl Into<String>,
    component_id: impl Into<String>,
) -> JsonResult {
    let result = execute::rollback_component(
        parse_game_id(game_id.into())?,
        parse_component_id(component_id.into())?,
    )?;

    serde_json::to_value(result).map_err(Into::into)
}
