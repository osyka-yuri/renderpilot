//! Preparing and abandoning a reserved shared Vulkan mutation.

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::GameId;
use rusqlite::named_params;

use crate::{error::storage_error, repositories::observations, sqlite_clock};

use super::super::SqliteStorage;
use super::model::{PendingSharedVulkanMutationState, SharedVulkanMutationScope};
use super::queries::read_shared_row;
use super::validation::{
    delete_shared_state, ensure_exact_owner, validate_manifest, validate_owner,
};

impl SqliteStorage {
    /// Publishes the complete before-snapshot manifest and invalidates the
    /// owner game's catalog in the same transaction when that game exists.
    pub fn finish_preparing_shared_vulkan_mutation(
        &self,
        id: &str,
        scope: SharedVulkanMutationScope,
        game_id: Option<&GameId>,
        manifest_json: &str,
    ) -> AppResult<()> {
        validate_manifest(manifest_json)?;
        validate_owner(scope, game_id)?;
        self.with_transaction(|transaction| {
            let row = read_shared_row(transaction)?.ok_or_else(|| {
                AppError::storage_failed(format!(
                    "shared Vulkan mutation '{id}' is missing or is not preparing"
                ))
            })?;
            ensure_exact_owner(&row, id, scope, game_id)?;
            if row.state != PendingSharedVulkanMutationState::Preparing {
                return Err(AppError::storage_failed(format!(
                    "shared Vulkan mutation '{id}' is missing or is not preparing"
                )));
            }
            let now_ms = sqlite_clock::now_ms(transaction)?;
            let updated = transaction
                .execute(
                    "UPDATE pending_shared_vulkan_mutations
                     SET state = 'prepared', manifest_json = :manifest_json, updated_at = :now_ms
                     WHERE resource_key = :resource_key AND id = :id AND scope = :scope
                       AND state = 'preparing'
                       AND ((game_id IS NULL AND :game_id IS NULL) OR game_id = :game_id)",
                    named_params! {
                        ":resource_key": super::RESOURCE_KEY,
                        ":id": id,
                        ":scope": scope.as_str(),
                        ":game_id": game_id.map(GameId::as_str),
                        ":manifest_json": manifest_json,
                        ":now_ms": now_ms,
                    },
                )
                .map_err(storage_error)?;
            if updated != 1 {
                return Err(AppError::storage_failed(format!(
                    "shared Vulkan mutation '{id}' changed before preparation completed"
                )));
            }
            if let Some(game_id) = game_id
                && observations::catalog_exists_within_transaction(transaction, game_id)?
            {
                observations::invalidate_game_authority_within_transaction(
                    transaction,
                    game_id,
                    "prepared_shared_vulkan_mutation",
                    Some(id),
                )?;
            }
            Ok(())
        })
    }

    /// Deletes an abandoned preparation, guarded by exact id and state.
    pub fn abandon_shared_vulkan_mutation_preparation(&self, id: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            delete_shared_state(transaction, id, PendingSharedVulkanMutationState::Preparing)
        })
    }
}
