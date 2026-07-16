use renderpilot_application::{
    AppError, AppResult, ArtifactRepository, ComponentRepository, GameRepository,
    InstalledAddonRepository, build_swap_operation_plan, find_replacement_candidates,
};
use renderpilot_domain::{ArtifactId, ComponentId, GameId, GraphicsComponent, LibraryArtifact};

use crate::ServiceError;

use super::{CandidateCatalogResult, SwapPlanCatalogResult};

/// Returns replacement candidate groups for a game using a caller-provided storage connection.
pub fn find_candidates(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<CandidateCatalogResult, ServiceError> {
    let storage = context.storage();
    let _ = storage.require_game(game_id)?;

    let components = storage.list_components_for_game(game_id)?;
    let artifacts = storage.list_artifacts()?;

    Ok(CandidateCatalogResult {
        game_id: game_id.clone(),
        groups: find_replacement_candidates(
            &components,
            &artifacts,
            &renderpilot_application::CandidateContext::empty(),
        ),
    })
}

/// Builds a swap plan for the specified component and artifact using a caller-provided storage connection.
pub fn build_swap_plan(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> Result<SwapPlanCatalogResult, ServiceError> {
    let _guard = crate::game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    let storage = context.storage();
    let (component, artifact) = require_swap_inputs(storage, game_id, component_id, artifact_id)?;
    let game = storage.require_game(game_id)?;
    let recorded = storage.get_component_backup(component_id)?;
    let installed_addon = storage.get_installed_addon(game_id)?;
    let managed_files = crate::coordinated_files::managed_files_of(installed_addon.as_ref());
    let baseline = crate::coordinated_files::resolve_component_baseline(
        std::path::Path::new(game.install_path().as_str()),
        component.files(),
        recorded.as_deref(),
        managed_files,
    )
    .map_err(AppError::from)?;
    let component_for_plan = component.rebuild_with_files(baseline);

    let plan = build_swap_operation_plan(&component_for_plan, &artifact)?;

    Ok(SwapPlanCatalogResult { plan })
}

fn require_swap_inputs<S>(
    storage: &S,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> AppResult<(GraphicsComponent, LibraryArtifact)>
where
    S: GameRepository + ComponentRepository + ArtifactRepository,
{
    let _ = storage.require_game(game_id)?;

    let component = require_component_for_game(storage, game_id, component_id)?;
    let artifact = require_artifact(storage, artifact_id)?;

    Ok((component, artifact))
}

pub(super) fn require_component_for_game<S>(
    storage: &S,
    game_id: &GameId,
    component_id: &ComponentId,
) -> AppResult<GraphicsComponent>
where
    S: ComponentRepository,
{
    find_required(
        storage.list_components_for_game(game_id)?,
        |component| component.id() == component_id,
        || AppError::component_not_found(component_id.as_str()),
    )
}

pub(super) fn require_artifact<S>(
    storage: &S,
    artifact_id: &ArtifactId,
) -> AppResult<LibraryArtifact>
where
    S: ArtifactRepository,
{
    find_required(
        storage.list_artifacts()?,
        |artifact| artifact.id() == artifact_id,
        || AppError::artifact_not_found(artifact_id.as_str()),
    )
}

pub(super) fn find_required<T>(
    items: impl IntoIterator<Item = T>,
    predicate: impl FnMut(&T) -> bool,
    not_found: impl FnOnce() -> AppError,
) -> AppResult<T> {
    items.into_iter().find(predicate).ok_or_else(not_found)
}
