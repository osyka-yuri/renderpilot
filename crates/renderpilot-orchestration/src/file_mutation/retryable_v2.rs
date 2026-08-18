//! Retryable, non-destructive-on-recovery file transactions for DLSS-Fix.
//!
//! Version 2 records immutable preimages and exact observations for each
//! forward operation. Synchronous rollback may use those records, while crash
//! recovery deliberately cleans a `Prepared` row without touching live files.

mod io;
mod model;
mod recovery;
#[cfg(test)]
mod test_support;
mod transaction;

pub(crate) use io::observe;
pub(crate) use model::{RetryableFileOperation, RetryableFilePlan, V2DiskObservation};
pub(super) use recovery::{is_v2_manifest, recover_pending_v2};
pub(crate) use transaction::RetryableFileMutationV2;

#[cfg(test)]
pub(crate) use test_support::{
    corrupt_next_preimage_snapshot_for_test, drift_next_absent_reservation_for_test,
    fail_next_absent_publish_for_test, fail_next_reservation_flush_for_test,
    fail_next_restore_snapshot_for_test,
};
