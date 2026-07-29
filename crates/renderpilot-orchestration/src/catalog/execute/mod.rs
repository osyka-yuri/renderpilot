//! Swap execution facade: apply overlays, direct rollback, and managed cleanup.

use renderpilot_application::{
    AppError, AppErrorKind, AppResult, ArtifactRepository, ComponentRepository, GameRepository,
    InstalledAddonRepository, OperationKind,
};
use renderpilot_domain::{
    ArtifactId, ComponentId, ComponentRollbackBaseline, D3d12ExecutableBaseline,
    D3d12ExecutableIdentity, GameId, PathRef, component_version_report, fsr,
};
use renderpilot_storage_sqlite::{
    ComponentBaselineMutation, GameMutationCommit, InstalledAddonMutation, SqliteStorage,
};

use crate::ServiceError;
use crate::catalog::swap::require_component_for_game;

mod apply;
mod fs_ops;
mod journal;
mod managed_rollback;
mod mutation_guard;
mod planning;
mod prepare;
mod rollback;
mod source_integrity;
#[cfg(test)]
mod test_hooks;
mod types;

#[cfg(test)]
mod tests;

pub use apply::{apply_swap, apply_swap_confirmed};
pub use rollback::{build_rollback_plan, rollback_component};
pub use types::{
    D3d12ExecutableActionResult, D3d12ExecutableActionResultKind, OperationMetadata, RollbackPlan,
    RollbackResult, SwapResult,
};

#[cfg(test)]
use apply::apply_mutation_paths;
pub(crate) use fs_ops::revert_to_baseline_fs;
use fs_ops::{perform_apply_fs, release_baseline_sidecars, restore_baseline_preserving_sidecars};
pub(crate) use journal::{
    JournalEntryItem, JournalEntryParams, component_file_item_count,
    journal_item_is_component_file, record_operation_journal_entry,
};
pub(crate) use managed_rollback::{
    ManagedComponentRollbackPlan, build_managed_rollback_plan_locked,
    orphaned_rollback_affected_files, release_redundant_component_baseline_locked,
    rollback_managed_component_locked,
};
use planning::rebuild_component_set_after_overlay;
use prepare::prepare_apply_swap;
pub(crate) use rollback::{
    ROLLBACK_TARGET_LABEL, build_rollback_plan_locked, mutation_paths_from_component_files,
    rollback_component_locked,
};
use rollback::{rollback_executable_assessment, rollback_mutation_paths};
use source_integrity::rebind_planned_files_for_technology;
#[cfg(test)]
use test_hooks::{
    D3d12ApplyFailurePoint, D3d12RollbackFailurePoint, inject_d3d12_apply_failure,
    inject_d3d12_rollback_failure, run_before_copy_hook, set_before_copy_hook,
    set_d3d12_apply_failure_point, set_d3d12_rollback_failure_point,
};
