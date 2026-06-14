//! Loads prerequisites and validates an apply before any mutation runs.

use renderpilot_application::{build_swap_operation_plan, AppError, AppResult};
use renderpilot_domain::{
    fsr, ArtifactId, ComponentId, GameId, GraphicsComponent, LibraryArtifact,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::catalog::swap::{require_artifact, require_component_for_game, require_game};

use super::planning::{
    additive_active_files, fsr_members_to_remove, full_component_set, planned_target_files,
    rebuild_component, resolve_target_dir, validate_artifact_sources_exist,
};
use super::types::{LoadedApplySwap, PreparedApplySwap};

pub(super) fn prepare_apply_swap(
    storage: &SqliteStorage,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> AppResult<PreparedApplySwap> {
    let LoadedApplySwap {
        component,
        artifact,
        baseline,
        first_swap,
    } = load_apply_swap(storage, game_id, component_id, artifact_id)?;

    validate_artifact_sources_exist(&artifact)?;
    validate_apply_is_allowed(&component, &artifact)?;

    let target_dir = resolve_target_dir(&component)?;
    let planned = planned_target_files(&artifact, &target_dir, &component)?;

    // New active set = baseline files not overwritten by the package (kept) +
    // the package's installed files, minus any FSR member a unified downgrade drops.
    // Computed before any FS/DB mutation, against the baseline — the file set the
    // directory holds once a re-swap's revert has run.
    let removed = fsr_members_to_remove(&baseline, &artifact, &planned);
    // additive_active_files appends kept baseline files first, which would leave
    // e.g. a denoiser in front of the entry point — store representative-first
    // (mirroring detection) so files()[0] carries the right version until the
    // next rescan.
    let mut new_files = additive_active_files(&baseline, &planned, &removed);
    fsr::sort_representative_first(&mut new_files);
    let rebuilt = rebuild_component(&component, new_files);
    let next_components = full_component_set(storage, game_id, rebuilt)?;

    Ok(PreparedApplySwap {
        game_id: game_id.clone(),
        component_id: component_id.clone(),
        component,
        artifact,
        baseline,
        planned,
        removed,
        next_components,
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

fn load_apply_swap(
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
