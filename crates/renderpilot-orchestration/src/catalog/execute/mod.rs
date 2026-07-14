//! Swap execution: apply an artifact overlay and roll it back.

use renderpilot_application::{
    AppError, AppErrorKind, AppResult, ArtifactRepository, OperationKind,
};
use renderpilot_domain::{ArtifactId, ComponentId, GameId, component_version_report, fsr};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;
use crate::catalog::swap::{require_component_for_game, require_game};

mod fs_ops;
mod journal;
mod planning;
mod prepare;
mod source_integrity;
mod streamline_install;
mod types;

#[cfg(test)]
mod tests;

pub use self::types::{OperationMetadata, RollbackResult, SwapResult};

use self::fs_ops::{perform_apply_fs, revert_to_baseline_fs};
use self::journal::{JournalEntryItem, JournalEntryParams, record_operation_journal_entry};
use self::planning::{rebuild_component, rebuild_component_set_after_overlay};
use self::prepare::{load_apply_swap, prepare_apply_swap};
use self::source_integrity::{
    ArtifactSourceCheck, rebind_planned_files_from_disk, validate_artifact_sources,
};
use self::types::AppliedFsChanges;

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
    let loaded = load_apply_swap(storage, game_id, component_id, artifact_id)?;
    ensure_artifact_sources_usable(storage, &loaded.artifact)?;
    let mut prepared = prepare_apply_swap(game_id, component_id, loaded)?;

    let changes = perform_apply_fs(
        &prepared.component,
        &prepared.baseline,
        &prepared.planned,
        &prepared.removed,
        prepared.first_swap,
    )?;

    // Re-read installed files so the catalog stores PE/hash truth, and verify
    // the copied bytes still match the preflight snapshot.
    rebind_planned_files_from_disk(&mut prepared.planned)
        .map_err(|error| abort_apply_after_fs(storage, &changes, prepared.artifact.id(), error))?;

    let (next_components, to_version) = with_undo(
        &changes,
        rebuild_component_set_after_overlay(
            storage,
            &prepared.game_id,
            &prepared.component,
            &prepared.component_id,
            &prepared.baseline,
            &prepared.planned,
            &prepared.removed,
        ),
    )?;

    let baseline_backup = prepared
        .first_swap
        .then_some((&prepared.component_id, prepared.baseline.as_slice()));
    with_undo(
        &changes,
        storage.commit_bundle_apply(&prepared.game_id, &next_components, baseline_backup),
    )?;

    record_operation_journal_entry(
        storage,
        JournalEntryParams {
            game_id: &prepared.game_id,
            component_id: &prepared.component_id,
            kind: OperationKind::ReplaceComponent,
            component: &prepared.component,
            to_version: to_version.as_deref(),
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
    let next_components = planning::full_component_set(storage, game_id, rebuilt)?;

    revert_to_baseline_fs(component.files(), &baseline)?;

    storage.commit_bundle_rollback(game_id, &next_components, component_id)?;

    record_operation_journal_entry(
        storage,
        JournalEntryParams {
            game_id,
            component_id,
            kind: OperationKind::RollbackComponent,
            component: &component,
            to_version: component_version_report(&baseline, component.technology())
                .known_version()
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

/// On error, undoes the FS overlay and converts the application error.
fn with_undo<T>(changes: &AppliedFsChanges, result: AppResult<T>) -> Result<T, ServiceError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            changes.undo();
            Err(error.into())
        }
    }
}

/// Recovery after the overlay is already on disk but post-copy rebind failed.
///
/// Always undoes the FS overlay. When the installed bytes diverge from the
/// planned artifact snapshot (`StaleReplacementSource`), also drops the stale
/// catalog row so a details reload can offer a healthy candidate.
fn abort_apply_after_fs(
    storage: &SqliteStorage,
    changes: &AppliedFsChanges,
    artifact_id: &ArtifactId,
    error: AppError,
) -> ServiceError {
    changes.undo();
    if matches!(error.kind(), AppErrorKind::StaleReplacementSource) {
        invalidate_stale_artifact(storage, artifact_id, "installed target hash mismatch");
    }
    error.into()
}

/// Performs read-only source validation and owns best-effort stale-row recovery.
///
/// The planning module never mutates the catalog. Once apply proves that a
/// source is unusable, this boundary removes the stale row so a details reload
/// can offer a healthy downloaded or re-scanned candidate.
fn ensure_artifact_sources_usable(
    storage: &SqliteStorage,
    artifact: &renderpilot_domain::LibraryArtifact,
) -> AppResult<()> {
    match validate_artifact_sources(artifact)? {
        ArtifactSourceCheck::Ok => Ok(()),
        ArtifactSourceCheck::Unusable(issue) => {
            invalidate_stale_artifact(storage, artifact.id(), &issue.to_string());
            Err(AppError::stale_replacement_source())
        }
    }
}

fn invalidate_stale_artifact(storage: &SqliteStorage, artifact_id: &ArtifactId, reason: &str) {
    if let Err(error) = storage.delete_artifact(artifact_id) {
        log::warn!(
            "failed to invalidate stale artifact {} ({reason}): {error}",
            artifact_id.as_str()
        );
    } else {
        log::info!(
            "invalidated stale artifact {} ({reason})",
            artifact_id.as_str()
        );
    }
}
