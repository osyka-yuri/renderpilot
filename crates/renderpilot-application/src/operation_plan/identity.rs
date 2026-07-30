use std::time::{SystemTime, UNIX_EPOCH};

use rand::Rng;
use renderpilot_domain::{LibraryArtifact, LibraryComponent, OperationId};

use crate::{AppError, AppResult, OperationKind};

pub(crate) const CONFIRMATION_TOKEN_BYTES: usize = 32;
pub(crate) const OPERATION_ID_NONCE_BYTES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationPlanIdentity {
    pub(crate) operation_id: OperationId,
    pub(crate) confirmation_token: String,
}

pub(crate) fn generate_operation_plan_identity(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> AppResult<OperationPlanIdentity> {
    Ok(OperationPlanIdentity {
        operation_id: generate_operation_id(component, artifact)?,
        confirmation_token: generate_confirmation_token(),
    })
}

fn generate_operation_id(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> AppResult<OperationId> {
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
    .map_err(|error| AppError::invalid_input(format!("cannot generate operation id: {error}")))
}

fn generate_confirmation_token() -> String {
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
