//! Loads and plans an apply before filesystem overlay.
//!
//! Source validation and stale-artifact recovery belong to the execution
//! boundary. This module remains a catalog/planning step once loaded inputs are
//! supplied.

use renderpilot_application::{AppError, AppResult, OperationPlan};
use renderpilot_domain::{ComponentId, GameId};

use crate::catalog::swap::ReadySwapPreflight;

use super::types::{PreparedApplySwap, PreparedD3d12Execution, writes_from_transition};

pub(super) fn prepare_apply_swap(
    game_id: &GameId,
    component_id: &ComponentId,
    preflight: ReadySwapPreflight,
) -> AppResult<PreparedApplySwap> {
    let ReadySwapPreflight {
        game: _,
        component,
        artifact,
        baseline,
        rollback_baseline,
        first_swap,
        transition,
        xiph_import_proof,
        operation_plan,
        target_profile,
    } = preflight;

    validate_apply_is_allowed(&operation_plan)?;
    let d3d12_action = operation_plan.d3d12_executable_action().cloned();
    let confirmation_token = operation_plan.confirmation_token().to_owned();
    let d3d12 = match (target_profile.d3d12, d3d12_action) {
        (Some(state), Some(action)) => Some(PreparedD3d12Execution {
            state,
            action,
            confirmation_token,
        }),
        (None, None) => None,
        _ => {
            return Err(AppError::invalid_input(
                "D3D12 execution context is incomplete",
            ));
        }
    };

    let transition = transition.ok_or_else(|| {
        AppError::invalid_input("cannot apply a swap without a resolved transition")
    })?;
    let writes = writes_from_transition(&transition);

    Ok(PreparedApplySwap {
        game_id: game_id.clone(),
        component_id: component_id.clone(),
        component,
        artifact,
        baseline,
        rollback_baseline,
        transition,
        writes,
        xiph_import_proof,
        first_swap,
        d3d12,
    })
}

fn validate_apply_is_allowed(plan: &OperationPlan) -> AppResult<()> {
    if plan.blockers().is_empty() {
        return Ok(());
    }

    let blockers = plan
        .blockers()
        .iter()
        .map(|blocker| blocker.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    Err(AppError::invalid_input(format!(
        "cannot apply blocked swap: {blockers}"
    )))
}
