use renderpilot_orchestration::catalog;
use renderpilot_orchestration::catalog::output::{RollbackPlanOutput, SwapPlanOutput};

use super::utils::{JsonResult, parse_artifact_id, parse_component_id, parse_game_id};

/// Builds a fresh swap preflight including the canonical EXE confirmation token.
pub fn plan_swap(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
    component_id: impl Into<String>,
    artifact_id: impl Into<String>,
) -> JsonResult {
    let result = catalog::build_swap_plan(
        context,
        &parse_game_id(game_id.into())?,
        &parse_component_id(component_id.into())?,
        &parse_artifact_id(artifact_id.into())?,
    )?;
    serde_json::to_value(SwapPlanOutput::from(&result.plan)).map_err(Into::into)
}

/// Applies a swap using a caller-provided storage connection.
pub fn apply_swap(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
    component_id: impl Into<String>,
    artifact_id: impl Into<String>,
    confirmation_token: Option<&str>,
    safety_context_token: Option<&str>,
) -> JsonResult {
    let game_id = parse_game_id(game_id.into())?;
    let component_id = parse_component_id(component_id.into())?;
    let artifact_id = parse_artifact_id(artifact_id.into())?;
    let safety = renderpilot_orchestration::FileSafetyAuthority::new()
        .game_permit(game_id.clone(), safety_context_token)?;
    let result = catalog::apply_swap(catalog::ApplySwapRequest {
        context,
        game_id: &game_id,
        component_id: &component_id,
        artifact_id: &artifact_id,
        executable_confirmation: confirmation_token,
        safety: &safety,
    })?;
    serde_json::to_value(result).map_err(Into::into)
}

/// Builds a fresh rollback preflight, including any managed EXE restore.
pub fn plan_rollback(
    context: &renderpilot_orchestration::Context,
    game_id: impl Into<String>,
    component_id: impl Into<String>,
) -> JsonResult {
    let plan = catalog::build_rollback_plan(
        context,
        &parse_game_id(game_id.into())?,
        &parse_component_id(component_id.into())?,
    )?;
    serde_json::to_value(RollbackPlanOutput::from(&plan)).map_err(Into::into)
}

/// Rolls a component back to its verified immutable baseline.
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
