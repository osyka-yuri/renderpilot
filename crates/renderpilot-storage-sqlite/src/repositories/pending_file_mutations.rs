//! Persistence for crash-recoverable game-file mutation manifests.

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::GameId;
use rusqlite::Transaction;

mod binding;
mod commit;
mod lifecycle;
mod model;
mod restore;

#[cfg(test)]
mod tests;

pub(crate) use binding::{
    PreparedOptiScalerCatalogBinding, prepare_optiscaler_catalog_binding,
    validate_optiscaler_catalog_binding,
};
pub(crate) use commit::mark_file_mutation_committed_within_transaction;
pub(crate) use commit::validate_prepared_mutation_commit_within_transaction;
pub(super) use commit::{
    OptiScalerAggregateBinding, OptiScalerAuxiliaryPreservation, OptiScalerBoundPath,
    OptiScalerBoundTransition, OptiScalerOwnedPreservation, ReusedAcquisitionMode,
    parse_optiscaler_journal_for_recovery, validate_optiscaler_binding_within_transaction,
    validate_optiscaler_directory_receipt_metadata, validate_optiscaler_file_receipt_metadata,
    validate_optiscaler_journal_for_begin, validate_optiscaler_journal_for_cas,
    validate_optiscaler_journal_for_committed_terminal, validate_optiscaler_journal_for_prepared,
    validate_optiscaler_journal_for_rollback_terminal,
};
pub(super) use model::PreparedMutationCommitBinding;
pub use model::{
    BeginFileMutationPreparation, PendingFileMutationRow, PendingFileMutationState,
    PreparedMutationResolutionFence,
};

/// Reads the exact catalog binding that a Prepared OptiScaler row must carry.
///
/// This is deliberately read-only. It reuses the canonical catalog
/// classifier and returns only the durable epoch/token needed by the recovery
/// proof; no authority is invalidated or advanced during restart acquisition.
pub(crate) fn read_optiscaler_catalog_binding_for_recovery(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    operation_id: &str,
) -> AppResult<Option<(u64, String)>> {
    match binding::classify_catalog_binding_within_transaction(transaction, game_id)? {
        model::CatalogBinding::CatalogAbsent => Ok(None),
        model::CatalogBinding::CatalogPresent(
            super::observations::CatalogReadiness::Invalidated {
                authority_epoch,
                mutation_token: Some(token),
                ..
            },
        ) if token == operation_id => Ok(Some((authority_epoch, token))),
        model::CatalogBinding::CatalogPresent(_) => Err(AppError::storage_failed(
            "Prepared OptiScaler recovery catalog binding is absent or mismatched",
        )),
    }
}
