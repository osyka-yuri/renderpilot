//! Persistence for crash-recoverable game-file mutation manifests.

mod binding;
mod commit;
mod lifecycle;
mod model;
mod restore;

#[cfg(test)]
mod tests;

pub(super) use commit::{
    mark_file_mutation_committed_within_transaction,
    validate_prepared_mutation_commit_within_transaction,
};
pub(super) use model::PreparedMutationCommitBinding;
pub use model::{
    BeginFileMutationPreparation, PendingFileMutationRow, PendingFileMutationState,
    PreparedRestoreFence,
};
