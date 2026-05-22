use renderpilot_domain::{GraphicsComponent, LibraryArtifact};

use crate::AppResult;

use super::assessment::{primary_component_file, OperationPlanAssessment};
use super::{generate_operation_plan_identity, OperationPlan};

/// Builds a swap operation plan without applying any filesystem changes.
pub fn build_swap_operation_plan(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> AppResult<OperationPlan> {
    let target_file = primary_component_file(component)?;
    let assessment = OperationPlanAssessment::assess(component, artifact, target_file);
    let identity = generate_operation_plan_identity(component, artifact);

    Ok(OperationPlan::new(
        component,
        artifact,
        target_file,
        assessment,
        identity,
    ))
}
