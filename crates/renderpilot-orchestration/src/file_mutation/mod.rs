//! Crash-recoverable transactions for game and external add-on file roots.
//!
//! ## Contract
//!
//! 1. Hold [`crate::game_mutation_lock::GameMutationGuard`].
//! 2. [`recover_pending`] runs (also from [`DurableFileTransaction::prepare`]).
//!    Boundary entry already recovers; prepare re-runs it idempotently so
//!    hand-rolled multi-step flows cannot skip recovery.
//! 3. Snapshot every path the feature may touch (over-inclusive is correct).
//! 4. Mutate files, then feature-commit DB with the reserved `mutation_id`.
//! 5. On success clean snapshots; on failure restore exact before-state.
//!
//! Prefer [`run_durable_mutation`] at call sites. Hand-rolled prepare/finish is
//! reserved for multi-step flows that open an engine sentinel first.

mod manifest;
mod recover;
mod scope;
mod transaction;

#[cfg(test)]
mod tests;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) use recover::recover_pending;
pub(crate) use scope::MutationScope;
pub(crate) use transaction::{DurableFileTransaction, DurableMutation, run_durable_mutation};

pub(super) fn remove_dir_if_exists(directory: &Path) -> Result<(), crate::ServiceError> {
    match fs::remove_dir_all(directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(crate::failed(format!(
            "failed to remove file transaction directory {}: {error}",
            directory.display()
        ))),
    }
}

pub(super) fn canonical_candidate(path: &Path) -> Result<PathBuf, crate::ServiceError> {
    crate::paths::canonical_candidate(path).map_err(|error| crate::failed(error.to_string()))
}
