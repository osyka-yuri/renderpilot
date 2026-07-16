//! Loads and plans an apply before filesystem overlay.
//!
//! Source validation and stale-artifact recovery belong to the execution
//! boundary. This module remains a catalog/planning step once loaded inputs are
//! supplied.

use renderpilot_application::{
    AppError, AppResult, GameRepository, InstalledAddonRepository, build_swap_operation_plan,
};
use renderpilot_domain::{ArtifactId, ComponentId, GameId, GraphicsComponent, LibraryArtifact};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::catalog::swap::{require_artifact, require_component_for_game};

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
    let game = storage.require_game(game_id)?;
    let component = require_component_for_game(storage, game_id, component_id)?;
    let artifact = require_artifact(storage, artifact_id)?;

    let recorded_baseline = storage.get_component_backup(component_id)?;
    let first_swap = recorded_baseline.is_none();
    let installed_addon = storage.get_installed_addon(game_id)?;
    let managed_files = crate::coordinated_files::managed_files_of(installed_addon.as_ref());
    let component = crate::coordinated_files::current_component_snapshot(&component, managed_files)
        .map_err(|error| {
            AppError::invalid_input(format!(
                "component {} changed on disk since it was scanned: {error}",
                component_id.as_str()
            ))
        })?
        .into_component();
    let baseline = crate::coordinated_files::resolve_component_baseline(
        std::path::Path::new(game.install_path().as_str()),
        component.files(),
        recorded_baseline.as_deref(),
        managed_files,
    )
    .map_err(|error| {
        AppError::invalid_input(format!(
            "cannot resolve an immutable baseline for component {}: {error}",
            component_id.as_str()
        ))
    })?;

    Ok(LoadedApplySwap {
        component,
        artifact,
        baseline,
        first_swap,
    })
}
