use renderpilot_application::{
    AppError, AppResult, ArtifactRepository, ComponentRepository, GameRepository,
    InstalledAddonRepository, build_swap_operation_plan, find_replacement_candidates,
};
use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, GameId, GameInstallation, GraphicsComponent,
    LibraryArtifact,
};

use crate::ServiceError;

use super::source_assessment::{
    ArtifactSourceAssessment, ArtifactSourceIssue, assess_artifact_runtime_metadata,
    assess_artifact_sources,
};
use super::{CandidateCatalogResult, SwapPlanCatalogResult};

/// Returns replacement candidate groups for a game using a caller-provided storage connection.
pub fn find_candidates(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<CandidateCatalogResult, ServiceError> {
    let storage = context.storage();
    let game = storage.require_game(game_id)?;
    let components = storage.list_components_for_game(game_id)?;
    let universe = super::load_replacement_universe(context)?;
    let candidate_context = universe
        .candidate_context
        .clone()
        .with_target_profile(super::runtime_compatibility::target_profile(context, &game));
    let matching_components =
        super::components_for_candidate_matching(context, game_id, &components)?;

    Ok(CandidateCatalogResult {
        game_id: game_id.clone(),
        groups: find_replacement_candidates(
            &matching_components,
            &universe.artifacts,
            &candidate_context,
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
    // Preview must serialize with mutations without running their recovery
    // preamble: pending-transaction recovery and legacy reconciliation can
    // change both game files and SQLite state.
    let _guard = crate::game_mutation_lock::blocking_lock(game_id);
    let preflight = match load_swap_preflight(context, game_id, component_id, artifact_id)? {
        SwapPreflight::Ready(preflight) => preflight,
        SwapPreflight::UnusableSource { .. } => {
            return Err(AppError::stale_replacement_source().into());
        }
    };
    Ok(SwapPlanCatalogResult {
        plan: preflight.operation_plan,
    })
}

/// Immutable inputs established by the common preview/apply preflight.
pub(super) struct ReadySwapPreflight {
    pub(super) game: GameInstallation,
    pub(super) component: GraphicsComponent,
    pub(super) artifact: LibraryArtifact,
    pub(super) baseline: Vec<ComponentFile>,
    pub(super) first_swap: bool,
    pub(super) operation_plan: renderpilot_application::OperationPlan,
}

/// Common preflight outcome. An unusable source remains data until the apply
/// mutation boundary decides whether to invalidate its catalog row.
pub(super) enum SwapPreflight {
    Ready(Box<ReadySwapPreflight>),
    UnusableSource {
        artifact_id: ArtifactId,
        issue: ArtifactSourceIssue,
    },
}

/// Loads fresh component state, resolves the immutable baseline, validates all
/// artifact bytes, and runs technology compatibility without mutating storage
/// or files.
pub(super) fn load_swap_preflight(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> Result<SwapPreflight, ServiceError> {
    let storage = context.storage();
    let game = storage.require_game(game_id)?;
    let component = require_component_for_game(storage, game_id, component_id)?;
    let artifact = require_artifact(storage, artifact_id)?;
    let recorded_baseline =
        crate::coordinated_files::load_component_backup_availability(storage, &component)?
            .into_available();
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
        component.technology(),
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

    let operation_plan = build_swap_operation_plan(&component, &artifact)?;
    if !operation_plan.blockers().is_empty() {
        return Ok(SwapPreflight::Ready(Box::new(ReadySwapPreflight {
            game,
            component,
            artifact,
            baseline,
            first_swap,
            operation_plan,
        })));
    }

    if let ArtifactSourceAssessment::Unusable(issue) = assess_artifact_sources(&artifact) {
        return Ok(SwapPreflight::UnusableSource {
            artifact_id: artifact.id().clone(),
            issue,
        });
    }

    super::runtime_compatibility::ensure_transition_compatible(
        context, &game, &component, &artifact,
    )?;
    if let ArtifactSourceAssessment::Unusable(issue) = assess_artifact_runtime_metadata(&artifact) {
        return Ok(SwapPreflight::UnusableSource {
            artifact_id: artifact.id().clone(),
            issue,
        });
    }

    Ok(SwapPreflight::Ready(Box::new(ReadySwapPreflight {
        game,
        component,
        artifact,
        baseline,
        first_swap,
        operation_plan,
    })))
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
