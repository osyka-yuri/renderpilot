use std::time::{SystemTime, UNIX_EPOCH};

use rand::Rng;
use renderpilot_domain::{GraphicsComponent, LibraryArtifact, OperationId};

use crate::{AppResult, OperationKind};

use super::assessment::{primary_component_file, OperationPlanAssessment};
use super::OperationPlan;

pub(crate) const CONFIRMATION_TOKEN_BYTES: usize = 32;
pub(crate) const OPERATION_ID_NONCE_BYTES: usize = 8;
pub(crate) const REQUIRES_BACKUP: bool = true;

/// Builds a swap operation plan without applying any filesystem changes.
pub fn build_swap_operation_plan(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> AppResult<OperationPlan> {
    let target_file = primary_component_file(component)?;
    let assessment = OperationPlanAssessment::assess(component, artifact, target_file);

    Ok(OperationPlan::new(
        component,
        artifact,
        target_file,
        assessment,
    ))
}

pub(crate) fn generate_operation_id(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> OperationId {
    let timestamp = current_epoch_nanos();
    let nonce = random_hex::<OPERATION_ID_NONCE_BYTES>();

    OperationId::new(format!(
        "operation:{}:{}:{}:{}",
        OperationKind::REPLACE_COMPONENT,
        timestamp,
        component.id().as_str(),
        artifact.id().as_str(),
    ))
    .or_else(|_| {
        OperationId::new(format!(
            "operation:{}:{}:{}",
            OperationKind::REPLACE_COMPONENT,
            timestamp,
            nonce,
        ))
    })
    .expect("generated operation id should be valid")
}

pub(crate) fn generate_confirmation_token() -> String {
    random_hex::<CONFIRMATION_TOKEN_BYTES>()
}

fn random_hex<const N: usize>() -> String {
    let mut bytes = [0u8; N];
    let mut rng = rand::rng();

    rng.fill_bytes(&mut bytes);

    hex::encode(bytes)
}

fn current_epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}
