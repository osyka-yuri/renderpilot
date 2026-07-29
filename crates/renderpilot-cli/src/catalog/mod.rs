// Thin re-export — implementation lives in renderpilot-orchestration.
pub(crate) use renderpilot_orchestration::catalog::{
    AddGameCatalogAction, AddGameDecision, AddGameDisposition, AddGameOption, AddGameRequest,
    AddGameRootChoice, AddGameUnavailableReason, AddGameWarning, OperationListCatalogResult,
    RollbackPlan, add_game, apply_swap_confirmed, build_rollback_plan, build_swap_plan,
    find_candidates, inspect_game_install, list_artifacts, list_operations, rollback_component,
};
