use renderpilot_application::OperationPlan;
use renderpilot_domain::OperationId;
use serde::Serialize;

use super::utils::{
    parse_artifact_id, parse_component_id, parse_game_id, parse_operation_id, to_json, JsonResult,
};
use crate::{catalog, output, APP_VERSION};

/// Persists a swap operation plan and returns the serialized plan details.
pub fn build_swap_plan(
    game_id: impl Into<String>,
    component_id: impl Into<String>,
    artifact_id: impl Into<String>,
) -> JsonResult {
    let result = catalog::build_swap_plan(
        parse_game_id(game_id.into())?,
        parse_component_id(component_id.into())?,
        parse_artifact_id(artifact_id.into())?,
    )?;

    to_json(OperationPlanOutput::from(&result.plan))
}

/// Creates or refreshes the backup for an operation, then applies it.
///
/// Prefer `apply_operation_plan` for desktop UI flows because it requires the
/// confirmation token returned by `build_swap_plan`.
pub fn apply_operation(operation_id: impl Into<String>) -> JsonResult {
    let operation_id = parse_operation_id(operation_id.into())?;

    apply_operation_with_backup(operation_id)
}

/// Applies a previously built operation plan after the UI echoes back its confirmation token.
pub fn apply_operation_plan(
    operation_id: impl Into<String>,
    confirmation_token: impl Into<String>,
) -> JsonResult {
    let operation_id = parse_operation_id(operation_id.into())?;
    let confirmation_token = confirmation_token.into();

    catalog::verify_confirmation_token(&operation_id, &confirmation_token)?;

    apply_operation_with_backup(operation_id)
}

fn apply_operation_with_backup(operation_id: OperationId) -> JsonResult {
    let _backup = catalog::create_backup(operation_id.clone(), APP_VERSION)?;
    let result = catalog::apply_operation(operation_id)?;

    output::apply_operation_value(&result).map_err(Into::into)
}

/// Rolls back one previously executed operation.
pub fn rollback_operation(operation_id: impl Into<String>) -> JsonResult {
    let operation_id = parse_operation_id(operation_id.into())?;
    let result = catalog::rollback_operation(operation_id)?;

    output::rollback_operation_value(&result).map_err(Into::into)
}

#[derive(Debug, Serialize)]
struct OperationPlanOutput {
    operation_id: String,
    confirmation_token: String,
    game_id: String,
    operation_type: String,
    target_path: String,
    replacement_path: String,
    original_version: Option<String>,
    replacement_version: Option<String>,
    original_sha256: Option<String>,
    replacement_sha256: Option<String>,
    risk_level: String,
    requires_backup: bool,
    requires_elevation: bool,
    artifact_id: String,
    blockers: Vec<String>,
    warnings: Vec<String>,
}

impl From<&OperationPlan> for OperationPlanOutput {
    fn from(plan: &OperationPlan) -> Self {
        Self {
            operation_id: plan.operation_id().as_str().to_owned(),
            confirmation_token: plan.confirmation_token().to_owned(),
            game_id: plan.game_id().as_str().to_owned(),
            operation_type: plan.operation_type().as_str().to_owned(),
            target_path: plan.target_path().as_str().to_owned(),
            replacement_path: plan.replacement_path().as_str().to_owned(),
            original_version: plan
                .original_version()
                .map(|version| version.as_str().to_owned()),
            replacement_version: plan
                .replacement_version()
                .map(|version| version.as_str().to_owned()),
            original_sha256: plan.original_sha256().map(|hash| hash.as_str().to_owned()),
            replacement_sha256: plan
                .replacement_sha256()
                .map(|hash| hash.as_str().to_owned()),
            risk_level: plan.risk_level().as_str().to_owned(),
            requires_backup: plan.requires_backup(),
            requires_elevation: plan.requires_elevation(),
            artifact_id: plan.artifact_id().as_str().to_owned(),
            blockers: plan
                .blockers()
                .iter()
                .map(|blocker| blocker.as_str().to_owned())
                .collect(),
            warnings: plan
                .warnings()
                .iter()
                .map(|warning| warning.as_str().to_owned())
                .collect(),
        }
    }
}
