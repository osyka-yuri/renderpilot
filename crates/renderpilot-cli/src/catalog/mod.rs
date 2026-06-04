use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use renderpilot_application::{
    find_replacement_candidates, AppError, ArtifactRepository, ComponentReplacementCandidates,
    ComponentRepository, GameRepository, OperationPlan, OperationRecord,
};
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{
    ArtifactId, ComponentId, GameId, GameInstallation, GraphicsComponent, GraphicsTechnology,
    LibraryArtifact,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::error::CliError;

use self::{
    operations::list_operations_with_storage,
    scan::scan_folder_impl,
    swap::{build_swap_plan_with_storage, find_candidates_with_storage},
};

#[cfg(windows)]
pub(crate) mod auto_scan;
pub(crate) mod covers;
pub(crate) mod execute;
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
    pub(crate) groups: Vec<ComponentReplacementCandidates>,
}

pub(crate) struct GameDetailsCatalogResult {
    pub(crate) game: GameInstallation,
    pub(crate) components: Vec<GraphicsComponent>,
    pub(crate) candidate_groups: Vec<ComponentReplacementCandidates>,
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
    pub(crate) component_ids: Vec<String>,
}

pub(crate) fn scan_folder(path: PathBuf) -> Result<Vec<ScanFolderCatalogResult>, CliError> {
    scan_folder_impl(path)
}

pub(crate) fn list_games() -> Result<Vec<GameInstallation>, CliError> {
    with_catalog_storage(|storage| storage.list_games().map_err(Into::into))
}

pub(crate) fn get_game_details_with_storage(
    storage: &SqliteStorage,
    game_id: GameId,
) -> Result<GameDetailsCatalogResult, CliError> {
    let game = require_game(storage, &game_id)?;
    let components = storage.list_components_for_game(&game_id)?;

    let local_artifacts = storage.list_artifacts()?;
    let mut all_artifacts = local_artifacts.clone();

    let downloaded_ids: HashSet<_> = local_artifacts.iter().map(|a| a.id().clone()).collect();
    let mut manifest_entry_ids = HashMap::new();

    if let Ok((manifest_artifacts, entry_ids)) =
        crate::desktop::libraries::manifest_entries_as_artifacts()
    {
        for artifact in manifest_artifacts {
            if !downloaded_ids.contains(artifact.id()) {
                all_artifacts.push(artifact);
            }
        }
        manifest_entry_ids = entry_ids;
    }

    let candidate_groups = find_replacement_candidates(
        &components,
        &all_artifacts,
        &renderpilot_application::CandidateContext::new(downloaded_ids, manifest_entry_ids),
    );

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

pub(crate) fn with_catalog_storage<T>(
    operation: impl FnOnce(&SqliteStorage) -> Result<T, CliError>,
) -> Result<T, CliError> {
    let storage = open_catalog_storage()?;
    operation(&storage)
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
        .ok_or_else(|| AppError::game_not_found(game_id.as_str()))
}
