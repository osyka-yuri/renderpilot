// Thin re-export — implementation lives in renderpilot-orchestration.
pub(crate) use renderpilot_orchestration::catalog::{
    apply_swap, build_swap_plan, find_candidates, list_artifacts, list_operations,
    rollback_component, scan_folder, OperationListCatalogResult, ScanFolderCatalogResult,
};

#[cfg(test)]
pub(crate) use renderpilot_orchestration::catalog::list_games;

#[cfg(test)]
pub(crate) use renderpilot_orchestration::storage::CATALOG_DB_PATH_ENV;
