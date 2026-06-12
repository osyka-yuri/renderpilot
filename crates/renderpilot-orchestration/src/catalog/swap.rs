use renderpilot_application::{
    build_swap_operation_plan, find_replacement_candidates, AppError, AppResult,
    ArtifactRepository, ComponentRepository, GameRepository,
};
use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, GameId, GraphicsComponent, LibraryArtifact,
};

use crate::ServiceError;

use super::{CandidateCatalogResult, SwapPlanCatalogResult};

/// Returns replacement candidate groups for a game using a caller-provided storage connection.
pub fn find_candidates(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<CandidateCatalogResult, ServiceError> {
    let storage = context.storage();
    require_game(storage, game_id)?;

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
    let storage = context.storage();
    let (component, artifact) = require_swap_inputs(storage, game_id, component_id, artifact_id)?;

    let component_for_plan = component_with_backup_original(component);

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
    require_game(storage, game_id)?;

    let component = require_component_for_game(storage, game_id, component_id)?;
    let artifact = require_artifact(storage, artifact_id)?;

    Ok((component, artifact))
}

pub(super) fn require_game<S>(storage: &S, game_id: &GameId) -> AppResult<()>
where
    S: GameRepository,
{
    storage
        .find_game(game_id)?
        .map(|_| ())
        .ok_or_else(|| AppError::game_not_found(game_id.as_str()))
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

/// If a `.bak` sidecar exists for the component's target file, treat the
/// backup as the true original and use its version / SHA-256 for the swap
/// plan.  This keeps the original metadata stable across multiple swaps.
fn component_with_backup_original(component: GraphicsComponent) -> GraphicsComponent {
    let Some(target_file) = component.files().first() else {
        return component;
    };

    let backup_path = std::path::PathBuf::from(target_file.path().as_str().to_owned() + ".bak");
    if !backup_path.exists() {
        return component;
    }

    let sha256 = match renderpilot_detection::sha256_file(&backup_path) {
        Ok(hash) => hash,
        Err(_) => return component,
    };
    let version = renderpilot_detection::read_windows_file_version(&backup_path);

    let mut modified_file = ComponentFile::new(target_file.path().clone()).with_sha256(sha256);
    if let Some(version) = version {
        modified_file = modified_file.with_version(version);
    }

    GraphicsComponent::new(
        component.id().clone(),
        component.game_id().clone(),
        component.kind(),
        component.technology(),
        component.swappability(),
    )
    .with_file(modified_file)
}
