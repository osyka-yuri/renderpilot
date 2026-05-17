use std::path::PathBuf;

use renderpilot_application::{
    AppError, ArtifactRepository, ComponentFileReplacementCandidates, ComponentRepository,
    GameRepository, OperationPlan, OperationRecord, OperationRepository, OperationStatus,
};
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{
    ArtifactId, ComponentId, GameId, GameInstallation, GraphicsComponent, GraphicsTechnology,
    LibraryArtifact, OperationId,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::{
    backup_manager::{
        planned_operation_confirmation_token, ApplyOperationCatalogResult, BackupCatalogResult,
        RollbackOperationCatalogResult,
    },
    error::CliError,
};

use self::{
    operations::list_operations_with_storage,
    scan::scan_folder_impl,
    swap::{build_swap_plan_with_storage, find_candidates_with_storage},
};

#[cfg(windows)]
pub(crate) mod auto_scan;
pub(crate) mod covers;
mod operations;
mod scan;
mod storage;
mod swap;

#[cfg(test)]
pub(crate) use self::storage::CATALOG_DB_PATH_ENV;

pub(crate) use self::storage::open_catalog_storage;

#[cfg(windows)]
pub(crate) use scan::prune_auto_scan_orphans;

pub(crate) struct ScanFolderCatalogResult {
    pub(crate) game: GameInstallation,
    pub(crate) libraries: Vec<DetectedLibraryFile>,
}

pub(crate) struct CandidateCatalogResult {
    pub(crate) game_id: GameId,
    pub(crate) groups: Vec<ComponentFileReplacementCandidates>,
}

pub(crate) struct GameDetailsCatalogResult {
    pub(crate) game: GameInstallation,
    pub(crate) components: Vec<GraphicsComponent>,
    pub(crate) candidate_groups: Vec<ComponentFileReplacementCandidates>,
    pub(crate) operations: OperationListCatalogResult,
}

pub(crate) struct SwapPlanCatalogResult {
    pub(crate) plan: OperationPlan,
}

pub(crate) struct OperationListCatalogResult {
    pub(crate) game_id: GameId,
    pub(crate) operations: Vec<OperationListCatalogEntry>,
}

pub(crate) struct OperationListCatalogEntry {
    pub(crate) operation: OperationRecord,
    pub(crate) item_count: usize,
    pub(crate) backup_count: usize,
}

pub(crate) fn scan_folder(path: PathBuf) -> Result<Vec<ScanFolderCatalogResult>, CliError> {
    scan_folder_impl(path)
}

pub(crate) fn list_games() -> Result<Vec<GameInstallation>, CliError> {
    with_catalog_storage(|storage| storage.list_games().map_err(Into::into))
}

pub(crate) fn get_game_details(game_id: GameId) -> Result<GameDetailsCatalogResult, CliError> {
    with_catalog_storage(|storage| get_game_details_with_storage(storage, game_id))
}

pub(crate) fn get_game_details_with_storage(
    storage: &SqliteStorage,
    game_id: GameId,
) -> Result<GameDetailsCatalogResult, CliError> {
    let game = require_game(storage, &game_id)?;
    let components = storage.list_components_for_game(&game_id)?;
    let candidate_groups = find_candidates_with_storage(storage, &game_id)?.groups;
    let operations = list_operations_with_storage(storage, &game_id)?;

    Ok(GameDetailsCatalogResult {
        game,
        components,
        candidate_groups,
        operations,
    })
}

pub(crate) fn list_artifacts(
    technology: Option<GraphicsTechnology>,
) -> Result<Vec<LibraryArtifact>, CliError> {
    with_catalog_storage(|storage| {
        let artifacts = storage.list_artifacts()?;
        Ok(filter_artifacts_by_technology(artifacts, technology))
    })
}

pub(crate) fn find_candidates(game_id: GameId) -> Result<CandidateCatalogResult, CliError> {
    with_catalog_storage(|storage| find_candidates_with_storage(storage, &game_id))
}

pub(crate) fn list_operations(game_id: GameId) -> Result<OperationListCatalogResult, CliError> {
    with_catalog_storage(|storage| list_operations_with_storage(storage, &game_id))
}

pub(crate) fn build_swap_plan(
    game_id: GameId,
    component_id: ComponentId,
    artifact_id: ArtifactId,
) -> Result<SwapPlanCatalogResult, CliError> {
    with_catalog_storage(|storage| {
        build_swap_plan_with_storage(storage, &game_id, &component_id, &artifact_id)
    })
}

pub(crate) fn create_backup(
    operation_id: OperationId,
    app_version: &str,
) -> Result<BackupCatalogResult, CliError> {
    with_catalog_storage(|storage| {
        crate::backup_manager::create_backup(storage, operation_id, app_version).map_err(Into::into)
    })
}

pub(crate) fn apply_operation(
    operation_id: OperationId,
) -> Result<ApplyOperationCatalogResult, CliError> {
    with_catalog_storage(|storage| {
        crate::backup_manager::apply_operation(storage, operation_id).map_err(Into::into)
    })
}

pub(crate) fn rollback_operation(
    operation_id: OperationId,
) -> Result<RollbackOperationCatalogResult, CliError> {
    with_catalog_storage(|storage| {
        crate::backup_manager::rollback_operation(storage, operation_id).map_err(Into::into)
    })
}

pub(crate) fn verify_confirmation_token(
    operation_id: &OperationId,
    token: &str,
) -> Result<(), CliError> {
    with_catalog_storage(|storage| {
        verify_confirmation_token_with_storage(storage, operation_id, token)
    })
}

pub(crate) fn with_catalog_storage<T>(
    operation: impl FnOnce(&SqliteStorage) -> Result<T, CliError>,
) -> Result<T, CliError> {
    let storage = open_catalog_storage()?;
    operation(&storage)
}

fn verify_confirmation_token_with_storage(
    storage: &SqliteStorage,
    operation_id: &OperationId,
    token: &str,
) -> Result<(), CliError> {
    let operation = require_planned_operation(storage, operation_id)?;
    let expected_token = planned_operation_confirmation_token(&operation)?;

    ensure_confirmation_token_matches(&expected_token, token)?;

    Ok(())
}

fn filter_artifacts_by_technology(
    artifacts: Vec<LibraryArtifact>,
    technology: Option<GraphicsTechnology>,
) -> Vec<LibraryArtifact> {
    match technology {
        Some(required_technology) => artifacts
            .into_iter()
            .filter(|artifact| artifact.technology() == required_technology)
            .collect(),
        None => artifacts,
    }
}

fn require_game<R>(repository: &R, game_id: &GameId) -> Result<GameInstallation, AppError>
where
    R: GameRepository + ?Sized,
{
    repository
        .find_game(game_id)?
        .ok_or_else(|| game_not_found(game_id))
}

fn require_operation<R>(
    repository: &R,
    operation_id: &OperationId,
) -> Result<OperationRecord, AppError>
where
    R: OperationRepository + ?Sized,
{
    repository
        .find_operation_entry(operation_id)?
        .map(|entry| entry.operation().clone())
        .ok_or_else(|| AppError::operation_not_found(operation_id.as_str()))
}

fn require_planned_operation<R>(
    repository: &R,
    operation_id: &OperationId,
) -> Result<OperationRecord, AppError>
where
    R: OperationRepository + ?Sized,
{
    let operation = require_operation(repository, operation_id)?;
    ensure_operation_is_planned(operation_id, &operation)?;

    Ok(operation)
}

fn ensure_operation_is_planned(
    operation_id: &OperationId,
    operation: &OperationRecord,
) -> Result<(), AppError> {
    if operation.status == OperationStatus::Planned {
        return Ok(());
    }

    Err(AppError::invalid_operation_state(
        operation_id.as_str(),
        operation.status.clone(),
    ))
}

fn ensure_confirmation_token_matches(
    expected_token: &str,
    provided_token: &str,
) -> Result<(), AppError> {
    if expected_token == provided_token {
        Ok(())
    } else {
        Err(AppError::confirmation_token_mismatch())
    }
}

fn game_not_found(game_id: &GameId) -> AppError {
    AppError::game_not_found(game_id.as_str())
}
