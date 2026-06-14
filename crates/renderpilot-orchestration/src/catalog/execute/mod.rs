//! Swap execution: apply an artifact overlay and roll it back.

use renderpilot_application::{AppError, OperationKind};
use renderpilot_domain::{fsr, ArtifactId, ComponentId, GameId};

use crate::catalog::swap::{require_component_for_game, require_game};
use crate::ServiceError;

mod fs_ops;
mod journal;
mod planning;
mod prepare;
mod types;

#[cfg(test)]
mod tests;

pub use self::types::{OperationMetadata, RollbackResult, SwapResult};

use self::fs_ops::{perform_apply_fs, revert_to_baseline_fs};
use self::journal::{record_operation_journal_entry, JournalEntryItem, JournalEntryParams};
use self::planning::{full_component_set, rebuild_component};
use self::prepare::prepare_apply_swap;

/// Label written to the journal when rolling back to the pre-swap baseline.
const ROLLBACK_TARGET_LABEL: &str = "Original";

/// Installs an artifact package over a component as an **additive overlay**.
pub fn apply_swap(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> Result<SwapResult, ServiceError> {
    let storage = context.storage();
    let prepared = prepare_apply_swap(storage, game_id, component_id, artifact_id)?;

    let changes = perform_apply_fs(
        &prepared.component,
        &prepared.baseline,
        &prepared.planned,
        &prepared.removed,
        prepared.first_swap,
    )?;

    let commit = if prepared.first_swap {
        storage.commit_bundle_apply(
            &prepared.game_id,
            &prepared.next_components,
            Some((&prepared.component_id, &prepared.baseline)),
        )
    } else {
        storage.commit_bundle_apply(&prepared.game_id, &prepared.next_components, None)
    };
    if let Err(error) = commit {
        changes.undo();
        return Err(error.into());
    }

    record_operation_journal_entry(
        storage,
        JournalEntryParams {
            game_id: &prepared.game_id,
            component_id: &prepared.component_id,
            kind: OperationKind::ReplaceComponent,
            component: &prepared.component,
            to_version: prepared.artifact.version().map(|v| v.as_str()),
            items: prepared
                .planned
                .iter()
                .map(|plan| JournalEntryItem {
                    path: plan.file.path(),
                    artifact_id: Some(prepared.artifact.id().clone()),
                })
                .collect(),
        },
    );

    Ok(SwapResult {
        game_id: prepared.game_id.as_str().to_owned(),
        component_id: prepared.component_id.as_str().to_owned(),
        applied_path: prepared.applied_path(),
        replacement_path: prepared.replacement_path(),
    })
}

/// Rolls a component back to its recorded baseline.
pub fn rollback_component(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<RollbackResult, ServiceError> {
    let storage = context.storage();
    require_game(storage, game_id)?;
    let component = require_component_for_game(storage, game_id, component_id)?;

    let Some(baseline) = storage.get_component_backup(component_id)? else {
        return Err(AppError::invalid_input(format!(
            "no swap to roll back for component {}",
            component_id.as_str()
        ))
        .into());
    };

    let restored_path = baseline
        .first()
        .map(|file| file.path().as_str().to_owned())
        .unwrap_or_default();

    let mut restored_files = baseline.clone();
    fsr::sort_representative_first(&mut restored_files);
    let rebuilt = rebuild_component(&component, restored_files);
    let next_components = full_component_set(storage, game_id, rebuilt)?;

    revert_to_baseline_fs(component.files(), &baseline)?;

    storage.commit_bundle_rollback(game_id, &next_components, component_id)?;

    record_operation_journal_entry(
        storage,
        JournalEntryParams {
            game_id,
            component_id,
            kind: OperationKind::RollbackComponent,
            component: &component,
            to_version: fsr::version_representative(&baseline)
                .and_then(|f| f.version())
                .map(|v| v.as_str())
                .or(Some(ROLLBACK_TARGET_LABEL)),
            items: baseline
                .iter()
                .map(|file| JournalEntryItem {
                    path: file.path(),
                    artifact_id: None,
                })
                .collect(),
        },
    );

    Ok(RollbackResult {
        game_id: game_id.as_str().to_owned(),
        component_id: component_id.as_str().to_owned(),
        restored_path,
    })
}
