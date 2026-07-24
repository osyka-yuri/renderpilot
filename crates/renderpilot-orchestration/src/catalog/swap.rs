use renderpilot_application::{
    AppError, AppErrorKind, AppResult, ArtifactRepository, ComponentRepository, GameRepository,
    InstalledAddonRepository, build_swap_operation_plan, d3d12_confirmation_token,
    find_replacement_candidates, replacement_executable_action,
};
use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, GameId, GameInstallation, GraphicsComponent,
    GraphicsTechnology, LibraryArtifact,
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
    let d3d12_component = components
        .iter()
        .find(|component| component.technology() == GraphicsTechnology::D3D12Agility);
    let target =
        super::runtime_compatibility::presentation_target_profile(context, &game, d3d12_component)?;
    let candidate_context = universe
        .candidate_context
        .clone()
        .with_target_profile(target.profile);
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
    let preflight = match load_swap_preflight(
        context,
        game_id,
        component_id,
        artifact_id,
        SwapPreflightMode::Preview,
    )? {
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
    pub(super) rollback_baseline: Option<renderpilot_domain::ComponentRollbackBaseline>,
    pub(super) first_swap: bool,
    pub(super) operation_plan: renderpilot_application::OperationPlan,
    pub(super) target_profile: super::runtime_compatibility::TargetProfileAssessment,
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

/// Named policy for the shared preview/apply preflight.
#[derive(Debug, Clone, Copy)]
pub(super) enum SwapPreflightMode {
    /// Read-only planning may include remote catalog artifacts and reports stale state directly.
    Preview,
    /// Mutation preflight uses only materialized artifacts and converts stale state
    /// into a confirmation mismatch once the caller supplied a token.
    Apply { confirmation_supplied: bool },
}

impl SwapPreflightMode {
    const fn include_catalog_artifacts(self) -> bool {
        matches!(self, Self::Preview)
    }

    const fn stale_state_is_confirmation_mismatch(self) -> bool {
        matches!(
            self,
            Self::Apply {
                confirmation_supplied: true
            }
        )
    }

    fn map_assessment_error(self, error: AppError) -> AppError {
        if self.stale_state_is_confirmation_mismatch()
            && matches!(
                error.kind(),
                AppErrorKind::InvalidInput | AppErrorKind::DetectionFailed
            )
        {
            AppError::confirmation_token_mismatch()
        } else {
            error
        }
    }
}

/// Loads fresh component state, resolves the immutable baseline, validates all
/// artifact bytes, and runs technology compatibility without mutating storage
/// or files.
pub(super) fn load_swap_preflight(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
    mode: SwapPreflightMode,
) -> Result<SwapPreflight, ServiceError> {
    let stale_confirmation_is_mismatch = mode.stale_state_is_confirmation_mismatch();
    let storage = context.storage();
    let game = storage.require_game(game_id)?;
    let component = require_component_for_game(storage, game_id, component_id)?;
    let (artifact, validate_materialized_source) = if mode.include_catalog_artifacts() {
        require_preview_artifact(context, artifact_id)?
    } else {
        (require_artifact(storage, artifact_id)?, true)
    };
    let (recorded_baseline, first_swap) =
        match crate::coordinated_files::load_component_backup_availability(storage, &component)? {
            crate::coordinated_files::ComponentBackupAvailability::NotRecorded => (None, true),
            crate::coordinated_files::ComponentBackupAvailability::Available(baseline) => {
                (Some(baseline), false)
            }
            crate::coordinated_files::ComponentBackupAvailability::Unavailable(_) => {
                if stale_confirmation_is_mismatch {
                    return Err(AppError::confirmation_token_mismatch().into());
                }
                return Err(AppError::invalid_input(format!(
                    "rollback baseline for component {} is incomplete; verify game files and scan again",
                    component_id.as_str()
                ))
                .into());
            }
        };
    let installed_addon = storage.get_installed_addon(game_id)?;
    let managed_files = crate::coordinated_files::managed_files_of(installed_addon.as_ref());
    let component = crate::coordinated_files::current_component_snapshot(&component, managed_files)
        .map_err(|error| {
            if stale_confirmation_is_mismatch {
                AppError::confirmation_token_mismatch()
            } else {
                AppError::invalid_input(format!(
                    "component {} changed on disk since it was scanned: {error}",
                    component_id.as_str()
                ))
            }
        })?
        .into_component();
    let baseline = crate::coordinated_files::resolve_component_baseline(
        std::path::Path::new(game.install_path().as_str()),
        component.technology(),
        component.files(),
        recorded_baseline.as_ref().map(|baseline| baseline.files()),
        managed_files,
    )
    .map_err(|error| {
        if stale_confirmation_is_mismatch {
            AppError::confirmation_token_mismatch()
        } else {
            AppError::invalid_input(format!(
                "cannot resolve an immutable baseline for component {}: {error}",
                component_id.as_str()
            ))
        }
    })?;

    let target_profile = super::runtime_compatibility::target_profile(
        context,
        &game,
        (component.technology() == GraphicsTechnology::D3D12Agility).then_some(&component),
    )
    .map_err(|error| mode.map_assessment_error(error))?;
    let mut operation_plan = build_swap_operation_plan(&component, &artifact)?;
    if let Some(action) = replacement_executable_action(&artifact, &target_profile.profile)
        .map_err(|error| {
            AppError::invalid_input(format!("runtime artifact is incompatible: {error}"))
        })
        .map_err(|error| mode.map_assessment_error(error))?
    {
        let state = target_profile
            .d3d12
            .as_ref()
            .ok_or_else(|| AppError::invalid_input("D3D12 executable assessment is unavailable"))
            .map_err(|error| mode.map_assessment_error(error))?;
        let confirmation_token =
            d3d12_confirmation_token(&component, &artifact, &target_profile.profile, &action)
                .ok_or_else(|| AppError::invalid_input("D3D12 confirmation state is unavailable"))
                .map_err(|error| mode.map_assessment_error(error))?;
        operation_plan = operation_plan.with_d3d12_executable_action(
            action,
            confirmation_token,
            state.current_sha256.clone(),
            None,
        );
    }
    if !operation_plan.blockers().is_empty() {
        return Ok(SwapPreflight::Ready(Box::new(ReadySwapPreflight {
            game,
            component,
            artifact,
            baseline,
            rollback_baseline: recorded_baseline,
            first_swap,
            operation_plan,
            target_profile,
        })));
    }

    if validate_materialized_source
        && let ArtifactSourceAssessment::Unusable(issue) = assess_artifact_sources(&artifact)
    {
        return Ok(SwapPreflight::UnusableSource {
            artifact_id: artifact.id().clone(),
            issue,
        });
    }

    super::runtime_compatibility::ensure_transition_compatible(
        &component,
        &artifact,
        &target_profile,
    )
    .map_err(|error| mode.map_assessment_error(error))?;
    if validate_materialized_source
        && let ArtifactSourceAssessment::Unusable(issue) =
            assess_artifact_runtime_metadata(&artifact)
    {
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
        rollback_baseline: recorded_baseline,
        first_swap,
        operation_plan,
        target_profile,
    })))
}

fn require_preview_artifact(
    context: &crate::Context,
    artifact_id: &ArtifactId,
) -> Result<(LibraryArtifact, bool), ServiceError> {
    if let Some(artifact) = context
        .storage()
        .list_artifacts()?
        .into_iter()
        .find(|artifact| artifact.id() == artifact_id)
    {
        return Ok((artifact, true));
    }
    crate::libraries::catalog_packages_as_artifacts()?
        .into_parts()
        .0
        .into_iter()
        .find(|artifact| artifact.id() == artifact_id)
        .map(|artifact| (artifact, false))
        .ok_or_else(|| AppError::artifact_not_found(artifact_id.as_str()).into())
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
