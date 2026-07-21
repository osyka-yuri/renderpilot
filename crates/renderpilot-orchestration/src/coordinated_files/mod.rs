//! Resolution and validation of immutable classic `.bak` baselines, plus
//! add-on-neutral plan execution for coordinated game files.
//!
//! - [`backup`] — persisted rollback availability projected onto disk
//! - [`baseline`] — resolve/verify classic sidecars and recorded baselines
//! - [`claim`] — catalog path ownership claims
//! - [`snapshot`] — active component freshness and post-rollback records
//! - [`plan`] — pure file plans and their executor

mod backup;
mod baseline;
mod claim;
mod observed_metadata;
mod plan;
mod snapshot;

#[cfg(test)]
mod tests;

pub(crate) use backup::{
    ComponentBackupAvailability, available_component_backup_ids, load_component_backup_availability,
};
pub(crate) use baseline::resolve_component_baseline;
pub(crate) use claim::{CatalogPathClaim, catalog_path_claim, managed_files_of};
pub(crate) use observed_metadata::{with_observed_inspection, with_observed_metadata};
pub(crate) use plan::{
    CoordinatedFilePlan, ExpectedLive, FilePlanBatchLog, OverlaySource, execute_file_plan,
    execute_file_plans, execute_restore_batch,
};
pub(crate) use snapshot::{current_component_snapshot, record_after_component_rollback};

#[cfg(test)]
pub(crate) use baseline::{BaselineConflict, BaselineResolver, ResolvedBaseline};
