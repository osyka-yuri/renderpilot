// Thin re-export — implementation lives in renderpilot-orchestration.
pub(crate) use renderpilot_orchestration::catalog::{
    OperationListCatalogResult, RollbackPlan, ScanFolderCatalogResult, apply_swap_confirmed,
    build_rollback_plan, build_swap_plan, find_candidates, list_artifacts, list_operations,
    rollback_component, scan_folder,
};

#[cfg(test)]
pub(crate) use renderpilot_orchestration::catalog::list_games;
