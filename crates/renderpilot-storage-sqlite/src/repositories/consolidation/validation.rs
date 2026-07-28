//! Structural invariants for consolidation plans.

use std::collections::HashSet;

use renderpilot_application::{AppError, AppResult};

use super::ConsolidationPlan;

pub(super) fn validate_plan(plan: &ConsolidationPlan) -> AppResult<()> {
    let mut sources = HashSet::new();
    let mut source_components = HashSet::new();
    let mut destination_components = HashSet::new();
    for source in &plan.sources {
        if source.source_game_id == plan.destination_game_id {
            return Err(AppError::storage_failed(
                "consolidation source cannot equal destination",
            ));
        }
        if !sources.insert(source.source_game_id.as_str()) {
            return Err(AppError::storage_failed(format!(
                "duplicate consolidation source {}",
                source.source_game_id
            )));
        }
        for rekey in &source.component_rekeys {
            if rekey.source_component_id.trim().is_empty()
                || rekey.destination_component_id.trim().is_empty()
            {
                return Err(AppError::storage_failed(
                    "component rekey identifiers cannot be empty",
                ));
            }
            if rekey.source_component_id == rekey.destination_component_id {
                return Err(AppError::storage_failed(format!(
                    "component rekey for {} must change ownership and identity",
                    source.source_game_id
                )));
            }
            if !source_components.insert(rekey.source_component_id.as_str())
                || !destination_components.insert(rekey.destination_component_id.as_str())
            {
                return Err(AppError::storage_failed(format!(
                    "component rekeys must be one-to-one across the whole consolidation plan; \
                     duplicate mapping found for {}",
                    source.source_game_id,
                )));
            }
        }
    }
    Ok(())
}
