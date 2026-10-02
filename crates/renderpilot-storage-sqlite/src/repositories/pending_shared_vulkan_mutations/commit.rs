use renderpilot_application::AppResult;
use rusqlite::named_params;

use crate::{error::storage_error, sqlite_clock};

use super::super::game_mutations::InstalledAddonMutation;
use super::super::{
    SqliteStorage, installed_addons, peer_aggregate_reservations, shared_artifacts,
};
use super::RESOURCE_KEY;
use super::model::{
    PendingSharedVulkanMutationState, SharedArtifactMutation, SharedVulkanMutationCommit,
};
use super::validation::validate_prepared_shared_vulkan_mutation_commit_within_transaction;

impl SqliteStorage {
    /// Commits shared artifact provenance and game add-on lifecycle state with
    /// the exact prepared shared mutation row in one SQLite transaction.
    pub fn commit_shared_vulkan_mutation(
        &self,
        commit: SharedVulkanMutationCommit<'_>,
    ) -> AppResult<()> {
        let SharedVulkanMutationCommit {
            id,
            scope,
            game_id,
            addon,
            shared_artifact,
        } = commit;
        self.with_transaction(|transaction| {
            if let Some(game_id) = game_id {
                peer_aggregate_reservations::ensure_no_peer_aggregate_reservation_within_transaction(
                    transaction,
                    game_id,
                )?;
            }
            let row = validate_prepared_shared_vulkan_mutation_commit_within_transaction(
                transaction,
                &SharedVulkanMutationCommit {
                    id,
                    scope,
                    game_id,
                    addon,
                    shared_artifact,
                },
            )?;
            if let Some(game_id) = game_id {
                match addon {
                    InstalledAddonMutation::Keep => {}
                    InstalledAddonMutation::Upsert(addon) => {
                        installed_addons::upsert_within_transaction(transaction, addon)?;
                    }
                    InstalledAddonMutation::Delete(kind) => {
                        installed_addons::delete_within_transaction(transaction, game_id, kind)?;
                    }
                    InstalledAddonMutation::ReplaceExpected {
                        expected,
                        replacement,
                    } => {
                        installed_addons::replace_expected_within_transaction(
                            transaction,
                            game_id,
                            expected,
                            replacement,
                        )?;
                    }
                    InstalledAddonMutation::OptiScaler(_) => {
                        return Err(renderpilot_application::AppError::invalid_input(
                            "shared Vulkan mutations cannot commit an OptiScaler aggregate",
                        ));
                    }
                }
            }
            match shared_artifact {
                SharedArtifactMutation::Keep => {}
                SharedArtifactMutation::Upsert(record) => {
                    shared_artifacts::upsert_within_transaction(transaction, record)?;
                }
                SharedArtifactMutation::Delete(kind) => {
                    shared_artifacts::delete_within_transaction(transaction, kind)?;
                }
            }
            let now_ms = sqlite_clock::now_ms(transaction)?;
            let updated = transaction
                .execute(
                    "UPDATE pending_shared_vulkan_mutations
                     SET state = 'committed', updated_at = :now_ms
                     WHERE resource_key = :resource_key AND id = :id
                       AND scope = :scope AND state = 'prepared'
                       AND ((game_id IS NULL AND :game_id IS NULL) OR game_id = :game_id)",
                    named_params! {
                        ":resource_key": RESOURCE_KEY,
                        ":id": id,
                        ":scope": scope.as_str(),
                        ":game_id": game_id.map(renderpilot_domain::GameId::as_str),
                        ":now_ms": now_ms,
                    },
                )
                .map_err(storage_error)?;
            if updated != 1 || row.state != PendingSharedVulkanMutationState::Prepared {
                return Err(renderpilot_application::AppError::storage_failed(format!(
                    "shared Vulkan mutation '{id}' changed before commit"
                )));
            }
            Ok(())
        })
    }

    /// Deletes a committed row after the caller removed its app-owned
    /// snapshots. The exact id and state fence prevent deleting a replacement
    /// reservation.
    pub fn cleanup_committed_shared_vulkan_mutation(&self, id: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            let deleted = transaction
                .execute(
                    "DELETE FROM pending_shared_vulkan_mutations
                     WHERE resource_key = ?1 AND id = ?2 AND state = 'committed'",
                    [RESOURCE_KEY, id],
                )
                .map_err(storage_error)?;
            if deleted != 1 {
                return Err(renderpilot_application::AppError::storage_failed(format!(
                    "shared Vulkan mutation '{id}' is missing or is not committed"
                )));
            }
            Ok(())
        })
    }
}
