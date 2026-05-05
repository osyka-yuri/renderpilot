use std::path::PathBuf;

use renderpilot_application::{
    ArtifactRepository, ComponentFileReplacementCandidates, OperationPlan, OperationRecord,
};
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{
    ArtifactId, ComponentId, GameId, GraphicsTechnology, LibraryArtifact, OperationId,
};

use crate::{
    backup_manager::{
        ApplyOperationCatalogResult, BackupCatalogResult, RollbackOperationCatalogResult,
    },
    error::CliError,
};

use self::{
    operations::list_operations_impl,
    scan::scan_folder_impl,
    storage::open_catalog_storage,
    swap::{build_swap_plan_impl, find_candidates_impl},
};

mod operations;
mod scan;
mod storage;
mod swap;

#[cfg(test)]
pub(crate) use self::storage::CATALOG_DB_PATH_ENV;

pub(crate) struct ScanFolderCatalogResult {
    pub(crate) game: renderpilot_domain::GameInstallation,
    pub(crate) libraries: Vec<DetectedLibraryFile>,
}

pub(crate) struct CandidateCatalogResult {
    pub(crate) game_id: GameId,
    pub(crate) groups: Vec<ComponentFileReplacementCandidates>,
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

pub(crate) fn scan_folder(path: PathBuf) -> Result<ScanFolderCatalogResult, CliError> {
    scan_folder_impl(path)
}

pub(crate) fn list_artifacts(
    technology: Option<GraphicsTechnology>,
) -> Result<Vec<LibraryArtifact>, CliError> {
    let mut artifacts = open_catalog_storage()?.list_artifacts()?;

    if let Some(technology) = technology {
        artifacts.retain(|artifact| artifact.technology() == technology);
    }

    Ok(artifacts)
}

pub(crate) fn find_candidates(game_id: GameId) -> Result<CandidateCatalogResult, CliError> {
    find_candidates_impl(game_id)
}

pub(crate) fn list_operations(game_id: GameId) -> Result<OperationListCatalogResult, CliError> {
    list_operations_impl(game_id)
}

pub(crate) fn build_swap_plan(
    game_id: GameId,
    component_id: ComponentId,
    artifact_id: ArtifactId,
) -> Result<SwapPlanCatalogResult, CliError> {
    build_swap_plan_impl(game_id, component_id, artifact_id)
}

pub(crate) fn create_backup(
    operation_id: OperationId,
    app_version: &str,
) -> Result<BackupCatalogResult, CliError> {
    let storage = open_catalog_storage()?;

    crate::backup_manager::create_backup(&storage, operation_id, app_version).map_err(Into::into)
}

pub(crate) fn apply_operation(
    operation_id: OperationId,
) -> Result<ApplyOperationCatalogResult, CliError> {
    let storage = open_catalog_storage()?;

    crate::backup_manager::apply_operation(&storage, operation_id).map_err(Into::into)
}

pub(crate) fn rollback_operation(
    operation_id: OperationId,
) -> Result<RollbackOperationCatalogResult, CliError> {
    let storage = open_catalog_storage()?;

    crate::backup_manager::rollback_operation(&storage, operation_id).map_err(Into::into)
}