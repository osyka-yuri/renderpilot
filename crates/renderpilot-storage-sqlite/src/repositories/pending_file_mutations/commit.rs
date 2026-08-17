use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::GameId;
use rusqlite::{OptionalExtension, Transaction, named_params};

use crate::{error::storage_error, sqlite_clock};

use super::super::observations::{self, CatalogReadiness};
use super::{
    binding::classify_catalog_binding_within_transaction,
    model::{CatalogBinding, PendingFileMutationState, PreparedMutationCommitBinding},
};

#[cfg(test)]
use super::super::SqliteStorage;

#[cfg(test)]
impl SqliteStorage {
    /// Test-only direct committed transition. Production commits flow through
    /// `GameMutationCommit`, which checks matching invalidated authority.
    pub(crate) fn mark_file_mutation_committed(&self, id: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            mark_file_mutation_committed_within_transaction(transaction, id)
        })
    }
}

pub(in crate::repositories) fn mark_file_mutation_committed_within_transaction(
    transaction: &Transaction<'_>,
    id: &str,
) -> AppResult<()> {
    let now_ms = sqlite_clock::now_ms(transaction)?;
    let updated = transaction
        .execute(
            "
            UPDATE pending_file_mutations
            SET state = 'committed', updated_at = :now_ms
            WHERE id = :id AND state = 'prepared'
            ",
            named_params! { ":id": id, ":now_ms": now_ms },
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(AppError::storage_failed(format!(
            "pending file mutation `{id}` is missing or is not prepared"
        )));
    }
    Ok(())
}

/// Validates the exact durable/catalog binding required before a feature
/// transaction writes its database half. This is intentionally owned here so
/// generic observation operations never need to interpret pending mutation
/// state.
pub(in crate::repositories) fn validate_prepared_mutation_commit_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    mutation_id: &str,
    component_count: Option<usize>,
    has_baseline_mutations: bool,
) -> AppResult<PreparedMutationCommitBinding> {
    let state: Option<String> = transaction
        .query_row(
            "SELECT state FROM pending_file_mutations WHERE id = ?1 AND game_id = ?2",
            [mutation_id, game_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    if state.as_deref() != Some(PendingFileMutationState::Prepared.as_str()) {
        return Err(AppError::storage_failed(format!(
            "file mutation '{mutation_id}' is not prepared for game {}",
            game_id.as_str()
        )));
    }

    match classify_catalog_binding_within_transaction(transaction, game_id)? {
        CatalogBinding::CatalogAbsent => {
            if component_count.is_some_and(|count| count != 0) || has_baseline_mutations {
                return Err(AppError::storage_failed(format!(
                    "pre-catalog file mutation '{mutation_id}' cannot write component or baseline state"
                )));
            }
            Ok(PreparedMutationCommitBinding::CatalogAbsent)
        }
        CatalogBinding::CatalogPresent(CatalogReadiness::Invalidated {
            mutation_token: Some(token),
            ..
        }) if token == mutation_id => Ok(PreparedMutationCommitBinding::CatalogInvalidated),
        // A game can be inserted after the durable row became Prepared. Its
        // trigger creates NeverCompleted; promote it to this mutation's
        // matching invalidation in the same commit before feature rows change.
        CatalogBinding::CatalogPresent(CatalogReadiness::NeverCompleted { .. }) => {
            let repaired = observations::invalidate_game_authority_within_transaction(
                transaction,
                game_id,
                "prepared_file_mutation",
                Some(mutation_id),
            )?;
            if matches!(
                repaired,
                CatalogReadiness::Invalidated {
                    mutation_token: Some(ref token),
                    ..
                } if token == mutation_id
            ) {
                Ok(PreparedMutationCommitBinding::CatalogInvalidated)
            } else {
                Err(AppError::storage_failed(
                    "late catalog binding did not produce matching invalidated authority",
                ))
            }
        }
        CatalogBinding::CatalogPresent(_) => Err(AppError::storage_failed(format!(
            "file mutation '{mutation_id}' has no matching invalidated scan authority"
        ))),
    }
}
