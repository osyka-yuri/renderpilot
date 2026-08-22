//! Singleton reservation of the shared Vulkan resource.

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::GameId;
use rusqlite::named_params;

use crate::{error::storage_error, sqlite_clock};

use super::super::SqliteStorage;
use super::RESOURCE_KEY;
use super::model::{BeginSharedVulkanMutation, SharedVulkanMutationReservation};
use super::queries::read_shared_row;
use super::validation::{
    assert_no_pending_file_mutation_id_within_transaction,
    assert_no_pending_file_mutation_within_transaction, validate_begin,
};

impl SqliteStorage {
    /// Atomically reserves the singleton shared Vulkan layer.
    ///
    /// An occupied singleton is a normal result and never overwrites the
    /// owner. The immediate transaction makes the read/insert decision one
    /// SQLite write reservation.
    pub fn try_begin_shared_vulkan_mutation(
        &self,
        begin: &BeginSharedVulkanMutation,
    ) -> AppResult<SharedVulkanMutationReservation> {
        validate_begin(begin)?;
        self.with_immediate_transaction(|transaction| {
            if let Some(row) = read_shared_row(transaction)? {
                return Ok(SharedVulkanMutationReservation::Occupied(row));
            }
            assert_no_pending_file_mutation_id_within_transaction(transaction, &begin.id)?;
            if let Some(game_id) = begin.game_id.as_ref() {
                assert_no_pending_file_mutation_within_transaction(transaction, game_id)?;
            }

            let now_ms = sqlite_clock::now_ms(transaction)?;
            transaction
                .execute(
                    "INSERT INTO pending_shared_vulkan_mutations
                        (resource_key, id, scope, game_id, feature, state,
                         manifest_json, root_capabilities_json, created_at, updated_at)
                     VALUES
                        (:resource_key, :id, :scope, :game_id, :feature, 'preparing',
                         :manifest_json, :root_capabilities_json, :now_ms, :now_ms)",
                    named_params! {
                        ":resource_key": RESOURCE_KEY,
                        ":id": begin.id.as_str(),
                        ":scope": begin.scope.as_str(),
                        ":game_id": begin.game_id.as_ref().map(GameId::as_str),
                        ":feature": begin.feature.as_str(),
                        ":manifest_json": begin.initial_manifest_json.as_str(),
                        ":root_capabilities_json": begin.root_capabilities_json.as_str(),
                        ":now_ms": now_ms,
                    },
                )
                .map_err(storage_error)?;
            Ok(SharedVulkanMutationReservation::Reserved(
                read_shared_row(transaction)?.ok_or_else(|| {
                    AppError::storage_failed("shared Vulkan reservation disappeared before commit")
                })?,
            ))
        })
    }
}
