use std::time::{SystemTime, UNIX_EPOCH};

use renderpilot_application::{
    build_swap_operation_plan, find_replacement_candidates, AppError, AppResult,
    ArtifactRepository, ComponentRepository, GameRepository, OperationItemRecord,
    OperationJournalEntry, OperationKind, OperationPlan, OperationPlanRiskLevel, OperationRecord,
    OperationRepository, OperationStatus, UnixTimestampMillis,
};
use renderpilot_domain::{ArtifactId, ComponentId, GameId, GraphicsComponent, LibraryArtifact};

use crate::{
    backup_manager::{metadata_json_for_planned_item, metadata_json_for_planned_operation},
    error::CliError,
};

use super::{CandidateCatalogResult, SwapPlanCatalogResult};

pub(super) fn find_candidates_with_storage<S>(
    storage: &S,
    game_id: &GameId,
) -> Result<CandidateCatalogResult, CliError>
where
    S: GameRepository + ComponentRepository + ArtifactRepository,
{
    require_game(storage, game_id)?;

    let components = storage.list_components_for_game(game_id)?;
    let artifacts = storage.list_artifacts()?;

    Ok(CandidateCatalogResult {
        game_id: game_id.clone(),
        groups: find_replacement_candidates(&components, &artifacts),
    })
}

pub(super) fn build_swap_plan_with_storage<S>(
    storage: &S,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> Result<SwapPlanCatalogResult, CliError>
where
    S: GameRepository + ComponentRepository + ArtifactRepository + OperationRepository,
{
    let (component, artifact) = require_swap_inputs(storage, game_id, component_id, artifact_id)?;

    let plan = build_swap_operation_plan(&component, &artifact)?;

    persist_planned_swap_if_allowed(storage, &plan, &component)?;

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

fn require_game<S>(storage: &S, game_id: &GameId) -> AppResult<()>
where
    S: GameRepository,
{
    storage
        .find_game(game_id)?
        .map(|_| ())
        .ok_or_else(|| AppError::game_not_found(game_id.as_str()))
}

fn require_component_for_game<S>(
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

fn require_artifact<S>(storage: &S, artifact_id: &ArtifactId) -> AppResult<LibraryArtifact>
where
    S: ArtifactRepository,
{
    find_required(
        storage.list_artifacts()?,
        |artifact| artifact.id() == artifact_id,
        || AppError::artifact_not_found(artifact_id.as_str()),
    )
}

fn find_required<T>(
    items: impl IntoIterator<Item = T>,
    predicate: impl FnMut(&T) -> bool,
    not_found: impl FnOnce() -> AppError,
) -> AppResult<T> {
    items.into_iter().find(predicate).ok_or_else(not_found)
}

fn persist_planned_swap_if_allowed<S>(
    storage: &S,
    plan: &OperationPlan,
    component: &GraphicsComponent,
) -> AppResult<()>
where
    S: OperationRepository,
{
    if !should_persist_planned_swap(plan) {
        return Ok(());
    }

    persist_planned_swap_operation(storage, plan, component)
}

/// Blocked plans describe incompatible pairings; persisting them would violate catalog triggers.
fn should_persist_planned_swap(plan: &OperationPlan) -> bool {
    plan.risk_level() != OperationPlanRiskLevel::Blocked
}

fn persist_planned_swap_operation<S>(
    storage: &S,
    plan: &OperationPlan,
    component: &GraphicsComponent,
) -> AppResult<()>
where
    S: OperationRepository,
{
    persist_planned_swap_operation_at(storage, plan, component, current_timestamp_millis()?)
}

fn persist_planned_swap_operation_at<S>(
    storage: &S,
    plan: &OperationPlan,
    component: &GraphicsComponent,
    created_at: UnixTimestampMillis,
) -> AppResult<()>
where
    S: OperationRepository,
{
    // Defensive check: keep the invariant local to the write path too,
    // not only at the call site.
    if !should_persist_planned_swap(plan) {
        return Err(AppError::storage_failed(
            "blocked swap plans must not be persisted",
        ));
    }

    let operation = planned_operation_record(plan, created_at)?;
    let item = planned_operation_item(plan, component)?;
    let entry = OperationJournalEntry::try_new(operation, vec![item])?;

    storage.save_operation_entry(&entry)
}

fn planned_operation_record(
    plan: &OperationPlan,
    created_at: UnixTimestampMillis,
) -> AppResult<OperationRecord> {
    let record = OperationRecord::new(
        plan.operation_id().clone(),
        plan.game_id().clone(),
        OperationKind::ReplaceComponent,
        OperationStatus::Planned,
        created_at,
    );

    Ok(record.with_metadata_json(metadata_json_for_planned_operation(plan)?))
}

fn planned_operation_item(
    plan: &OperationPlan,
    component: &GraphicsComponent,
) -> AppResult<OperationItemRecord> {
    let item = OperationItemRecord::new(
        plan.operation_id().clone(),
        component.id().clone(),
        plan.target_path().clone(),
        OperationStatus::Planned,
    );

    Ok(item
        .with_artifact_id(plan.artifact_id().clone())
        .with_target_path(plan.replacement_path().clone())
        .with_metadata_json(metadata_json_for_planned_item(plan)?))
}

fn current_timestamp_millis() -> AppResult<UnixTimestampMillis> {
    unix_timestamp_millis(SystemTime::now())
}

fn unix_timestamp_millis(time: SystemTime) -> AppResult<UnixTimestampMillis> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::storage_failed("system clock is before Unix epoch"))?;

    let millis = i64::try_from(duration.as_millis())
        .map_err(|_| AppError::storage_failed("system clock is too large to persist"))?;

    UnixTimestampMillis::new(millis)
}
