use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::GameId;
use rusqlite::OptionalExtension;

use crate::error::storage_error;

use super::super::{
    SqliteStorage,
    observations::{self, CatalogReadiness},
};
use super::{
    binding::classify_catalog_binding_within_transaction,
    model::{
        CatalogBinding, PendingFileMutationState, PreparedRestoreCatalogBinding,
        PreparedRestoreFence,
    },
};

impl SqliteStorage {
    /// Acquires the authority fence required before any Prepared-row restore.
    pub fn fence_prepared_file_mutation_restore(
        &self,
        expected_game_id: &GameId,
        id: &str,
    ) -> AppResult<PreparedRestoreFence> {
        self.with_transaction(|transaction| {
            let row: Option<(String, String)> = transaction
                .query_row(
                    "SELECT game_id, state FROM pending_file_mutations WHERE id = ?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(storage_error)?;
            let Some((game_id, state)) = row else {
                return Err(AppError::storage_failed(format!(
                    "prepared file mutation '{id}' is missing"
                )));
            };
            if game_id != expected_game_id.as_str() {
                return Err(AppError::storage_failed(format!(
                    "prepared file mutation '{id}' does not belong to game {}",
                    expected_game_id.as_str()
                )));
            }
            if state != PendingFileMutationState::Prepared.as_str() {
                return Err(AppError::storage_failed(format!(
                    "file mutation '{id}' is not prepared"
                )));
            }
            let catalog_binding = match classify_catalog_binding_within_transaction(
                transaction,
                expected_game_id,
            )? {
                CatalogBinding::CatalogAbsent => PreparedRestoreCatalogBinding::CatalogAbsent,
                CatalogBinding::CatalogPresent(CatalogReadiness::Invalidated {
                    authority_epoch,
                    mutation_token: Some(token),
                    ..
                }) if token == id => {
                    PreparedRestoreCatalogBinding::CatalogInvalidated { authority_epoch }
                }
                CatalogBinding::CatalogPresent(_) => {
                    let repaired = observations::invalidate_game_authority_within_transaction(
                        transaction,
                        expected_game_id,
                        "recovery",
                        Some(id),
                    )?;
                    let CatalogReadiness::Invalidated {
                        authority_epoch,
                        mutation_token: Some(token),
                        ..
                    } = repaired
                    else {
                        return Err(AppError::storage_failed(
                            "prepared restore fence did not produce invalidated catalog authority",
                        ));
                    };
                    if token != id {
                        return Err(AppError::storage_failed(
                            "prepared restore fence produced a mismatched mutation token",
                        ));
                    }
                    PreparedRestoreCatalogBinding::CatalogInvalidated { authority_epoch }
                }
            };
            Ok(PreparedRestoreFence {
                game_id: expected_game_id.clone(),
                mutation_id: id.to_owned(),
                catalog_binding,
            })
        })
    }

    /// Removes a Prepared row only after restoration executed under its
    /// matching fence.
    pub fn complete_prepared_file_mutation_restore(
        &self,
        fence: &PreparedRestoreFence,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            match (
                &fence.catalog_binding,
                classify_catalog_binding_within_transaction(transaction, &fence.game_id)?,
            ) {
                (PreparedRestoreCatalogBinding::CatalogAbsent, CatalogBinding::CatalogAbsent) => {}
                (
                    PreparedRestoreCatalogBinding::CatalogInvalidated { authority_epoch },
                    CatalogBinding::CatalogPresent(CatalogReadiness::Invalidated {
                        authority_epoch: current_epoch,
                        mutation_token: Some(token),
                        ..
                    }),
                ) if *authority_epoch == current_epoch && token == fence.mutation_id => {}
                _ => {
                    return Err(AppError::storage_failed(
                        "prepared restore fence changed before restore completion",
                    ));
                }
            }
            let deleted = transaction
                .execute(
                    "DELETE FROM pending_file_mutations
                     WHERE id = ?1 AND game_id = ?2 AND state = 'prepared'",
                    [&fence.mutation_id, fence.game_id.as_str()],
                )
                .map_err(storage_error)?;
            if deleted != 1 {
                return Err(AppError::storage_failed(
                    "prepared file mutation changed before restore completion",
                ));
            }
            Ok(())
        })
    }
}
