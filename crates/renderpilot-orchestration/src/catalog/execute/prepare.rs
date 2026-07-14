//! Loads and plans an apply before filesystem overlay.
//!
//! Source validation and stale-artifact recovery belong to the execution
//! boundary. This module remains a catalog/planning step once loaded inputs are
//! supplied.

use renderpilot_application::{AppError, AppResult, build_swap_operation_plan};
use renderpilot_domain::{ArtifactId, ComponentId, GameId, GraphicsComponent, LibraryArtifact};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::catalog::swap::{require_artifact, require_component_for_game, require_game};

use super::planning::{fsr_members_to_remove, planned_target_files, resolve_target_dir};
use super::types::{LoadedApplySwap, PreparedApplySwap};

pub(super) fn prepare_apply_swap(
    game_id: &GameId,
    component_id: &ComponentId,
    loaded: LoadedApplySwap,
) -> AppResult<PreparedApplySwap> {
    let LoadedApplySwap {
        component,
        artifact,
        baseline,
        first_swap,
    } = loaded;

    validate_apply_is_allowed(&component, &artifact)?;

    let target_dir = resolve_target_dir(&component)?;
    let planned = planned_target_files(&artifact, &target_dir, &component)?;

    // Membership removals are planned against the baseline before any FS/DB
    // mutation. The post-apply component set is rebuilt after the overlay so
    // installed PE version / hash can be re-read from disk.
    let removed = fsr_members_to_remove(&baseline, &artifact, &planned);

    Ok(PreparedApplySwap {
        game_id: game_id.clone(),
        component_id: component_id.clone(),
        component,
        artifact,
        baseline,
        planned,
        removed,
        first_swap,
    })
}

fn validate_apply_is_allowed(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> AppResult<()> {
    let plan = build_swap_operation_plan(component, artifact)?;

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

pub(super) fn load_apply_swap(
    storage: &SqliteStorage,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> AppResult<LoadedApplySwap> {
    require_game(storage, game_id)?;
    let component = require_component_for_game(storage, game_id, component_id)?;
    let artifact = require_artifact(storage, artifact_id)?;

    let recorded_baseline = storage.get_component_backup(component_id)?;
    let first_swap = recorded_baseline.is_none();
    // The baseline is the *original* file set: the recorded one on a re-swap,
    // or the current files on the very first swap.
    let baseline = recorded_baseline.unwrap_or_else(|| component.files().to_vec());

    Ok(LoadedApplySwap {
        component,
        artifact,
        baseline,
        first_swap,
    })
}
