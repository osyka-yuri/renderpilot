use std::time::{SystemTime, UNIX_EPOCH};

use renderpilot_application::{
    build_swap_operation_plan, find_replacement_candidates, AppError, AppResult,
    ArtifactRepository, ComponentRepository, GameRepository, OperationItemRecord, OperationKind,
    OperationRecord, OperationRepository, OperationStatus, UnixTimestampMillis,
};
use renderpilot_domain::{ArtifactId, ComponentId, GameId, GraphicsComponent, LibraryArtifact};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::{backup_manager::metadata_json_for_planned_item, error::CliError};

use super::{storage::open_catalog_storage, CandidateCatalogResult, SwapPlanCatalogResult};

pub(super) fn find_candidates_impl(game_id: GameId) -> Result<CandidateCatalogResult, CliError> {
    let storage = open_catalog_storage()?;
    ensure_game_exists(&storage, &game_id)?;
    let components = storage.list_components_for_game(&game_id)?;
    let artifacts = storage.list_artifacts()?;
    let groups = find_replacement_candidates(&components, &artifacts);

    Ok(CandidateCatalogResult { game_id, groups })
}

pub(super) fn build_swap_plan_impl(
    game_id: GameId,
    component_id: ComponentId,
    artifact_id: ArtifactId,
) -> Result<SwapPlanCatalogResult, CliError> {
    let storage = open_catalog_storage()?;
    ensure_game_exists(&storage, &game_id)?;

    let component = find_component_for_game(&storage, &game_id, &component_id)?;
    let artifact = find_artifact(&storage, &artifact_id)?;
    let plan = build_swap_operation_plan(&component, &artifact)?;

    persist_swap_operation_plan(&storage, &plan, &component)?;

    Ok(SwapPlanCatalogResult { plan })
}

fn ensure_game_exists(storage: &SqliteStorage, game_id: &GameId) -> AppResult<()> {
    storage
        .find_game(game_id)?
        .ok_or_else(|| AppError::invalid_input(format!("game not found: {}", game_id.as_str())))?;

    Ok(())
}

fn find_component_for_game(
    storage: &SqliteStorage,
    game_id: &GameId,
    component_id: &ComponentId,
) -> AppResult<GraphicsComponent> {
    storage
        .list_components_for_game(game_id)?
        .into_iter()
        .find(|component| component.id() == component_id)
        .ok_or_else(|| {
            AppError::invalid_input(format!(
                "component not found for game {}: {}",
                game_id.as_str(),
                component_id.as_str()
            ))
        })
}

fn find_artifact(storage: &SqliteStorage, artifact_id: &ArtifactId) -> AppResult<LibraryArtifact> {
    storage
        .list_artifacts()?
        .into_iter()
        .find(|artifact| artifact.id() == artifact_id)
        .ok_or_else(|| {
            AppError::invalid_input(format!("artifact not found: {}", artifact_id.as_str()))
        })
}

fn persist_swap_operation_plan(
    storage: &SqliteStorage,
    plan: &renderpilot_application::OperationPlan,
    component: &GraphicsComponent,
) -> AppResult<()> {
    let created_at = current_timestamp_millis()?;
    let operation = OperationRecord::new(
        plan.operation_id().clone(),
        plan.game_id().clone(),
        OperationKind::ReplaceComponent,
        OperationStatus::Planned,
        created_at,
    );
    let item = OperationItemRecord::new(
        plan.operation_id().clone(),
        component.id().clone(),
        plan.target_path().clone(),
        OperationStatus::Planned,
    )
    .with_artifact_id(plan.artifact_id().clone())
    .with_target_path(plan.replacement_path().clone())
    .with_metadata_json(metadata_json_for_planned_item(plan)?);

    storage.upsert_operation(&operation)?;
    storage.replace_operation_items(plan.operation_id(), &[item])
}

fn current_timestamp_millis() -> AppResult<UnixTimestampMillis> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::storage_failed("system clock is before Unix epoch"))?;
    let millis = i64::try_from(duration.as_millis())
        .map_err(|_| AppError::storage_failed("system clock is too large to persist"))?;

    UnixTimestampMillis::new(millis)
}
