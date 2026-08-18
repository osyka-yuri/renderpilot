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
        CatalogBinding, PendingFileMutationState, PreparedMutationResolutionFence,
        PreparedResolutionCatalogBinding,
    },
};

impl SqliteStorage {
    /// Acquires the authority fence required before resolving a Prepared row.
    pub fn fence_prepared_file_mutation_resolution(
        &self,
        expected_game_id: &GameId,
        id: &str,
    ) -> AppResult<PreparedMutationResolutionFence> {
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
                CatalogBinding::CatalogAbsent => PreparedResolutionCatalogBinding::CatalogAbsent,
                CatalogBinding::CatalogPresent(CatalogReadiness::Invalidated {
                    authority_epoch,
                    mutation_token: Some(token),
                    ..
                }) if token == id => {
                    PreparedResolutionCatalogBinding::CatalogInvalidated { authority_epoch }
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
                            "prepared resolution fence did not produce invalidated catalog authority",
                        ));
                    };
                    if token != id {
                        return Err(AppError::storage_failed(
                            "prepared resolution fence produced a mismatched mutation token",
                        ));
                    }
                    PreparedResolutionCatalogBinding::CatalogInvalidated { authority_epoch }
                }
            };
            Ok(PreparedMutationResolutionFence {
                game_id: expected_game_id.clone(),
                mutation_id: id.to_owned(),
                catalog_binding,
            })
        })
    }

    /// Removes a Prepared row only after restoration executed under its
    /// matching fence.
    pub fn complete_prepared_file_mutation_restored(
        &self,
        fence: PreparedMutationResolutionFence,
    ) -> AppResult<()> {
        self.complete_prepared_file_mutation_resolution(fence, "restore")
    }

    /// Removes a Prepared row after an authenticated cleanup-only resolution.
    ///
    /// This is intentionally distinct from restore completion: callers use it
    /// only when the durable manifest cannot prove which forward operations
    /// reached disk, so touching live paths would risk clobbering a foreign
    /// file. The catalog remains invalidated and must be rebuilt from disk.
    pub fn complete_prepared_file_mutation_without_restore(
        &self,
        fence: PreparedMutationResolutionFence,
    ) -> AppResult<()> {
        self.complete_prepared_file_mutation_resolution(fence, "cleanup-only recovery")
    }

    fn complete_prepared_file_mutation_resolution(
        &self,
        fence: PreparedMutationResolutionFence,
        resolution: &str,
    ) -> AppResult<()> {
        let PreparedMutationResolutionFence {
            game_id,
            mutation_id,
            catalog_binding,
        } = fence;
        self.with_transaction(|transaction| {
            match (
                &catalog_binding,
                classify_catalog_binding_within_transaction(transaction, &game_id)?,
            ) {
                (
                    PreparedResolutionCatalogBinding::CatalogAbsent,
                    CatalogBinding::CatalogAbsent,
                ) => {}
                (
                    PreparedResolutionCatalogBinding::CatalogInvalidated { authority_epoch },
                    CatalogBinding::CatalogPresent(CatalogReadiness::Invalidated {
                        authority_epoch: current_epoch,
                        mutation_token: Some(token),
                        ..
                    }),
                ) if *authority_epoch == current_epoch && token == mutation_id => {}
                _ => {
                    return Err(AppError::storage_failed(format!(
                        "prepared resolution fence changed before {resolution} completion"
                    )));
                }
            }
            let deleted = transaction
                .execute(
                    "DELETE FROM pending_file_mutations
                     WHERE id = ?1 AND game_id = ?2 AND state = 'prepared'",
                    [mutation_id.as_str(), game_id.as_str()],
                )
                .map_err(storage_error)?;
            if deleted != 1 {
                return Err(AppError::storage_failed(format!(
                    "prepared file mutation changed before {resolution} completion"
                )));
            }
            Ok(())
        })
    }
}
