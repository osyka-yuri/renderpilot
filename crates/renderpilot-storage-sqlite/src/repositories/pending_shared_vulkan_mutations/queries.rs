//! Read-only queries and row decoding for the shared Vulkan reservation.

use renderpilot_application::AppResult;
use renderpilot_domain::GameId;
use rusqlite::{OptionalExtension, Row};

use crate::{error::storage_error, repositories::SqliteStorage};

use super::RESOURCE_KEY;
use super::model::PendingSharedVulkanMutationRow;

impl SqliteStorage {
    /// Returns the singleton reservation, if one exists.
    pub fn pending_shared_vulkan_mutation(
        &self,
    ) -> AppResult<Option<PendingSharedVulkanMutationRow>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, scope, game_id, feature, state, manifest_json,
                            root_capabilities_json
                     FROM pending_shared_vulkan_mutations
                     WHERE resource_key = ?1",
                    [RESOURCE_KEY],
                    |row| Ok(row_to_shared_row(row)),
                )
                .optional()
                .map_err(storage_error)?
                .transpose()
        })
    }

    /// Reads one shared reservation by its mutation token.
    pub fn get_pending_shared_vulkan_mutation(
        &self,
        id: &str,
    ) -> AppResult<Option<PendingSharedVulkanMutationRow>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, scope, game_id, feature, state, manifest_json,
                            root_capabilities_json
                     FROM pending_shared_vulkan_mutations
                     WHERE resource_key = ?1 AND id = ?2",
                    [RESOURCE_KEY, id],
                    |row| Ok(row_to_shared_row(row)),
                )
                .optional()
                .map_err(storage_error)?
                .transpose()
        })
    }
}

pub(super) fn read_shared_row(
    connection: &rusqlite::Connection,
) -> AppResult<Option<PendingSharedVulkanMutationRow>> {
    connection
        .query_row(
            "SELECT id, scope, game_id, feature, state, manifest_json,
                    root_capabilities_json
             FROM pending_shared_vulkan_mutations WHERE resource_key = ?1",
            [RESOURCE_KEY],
            |row| Ok(row_to_shared_row(row)),
        )
        .optional()
        .map_err(storage_error)?
        .transpose()
}

pub(super) fn row_to_shared_row(row: &Row<'_>) -> AppResult<PendingSharedVulkanMutationRow> {
    let scope = row
        .get::<_, String>("scope")
        .map_err(storage_error)?
        .parse()?;
    let game_id = row
        .get::<_, Option<String>>("game_id")
        .map_err(storage_error)?
        .map(|value| {
            GameId::new(value).map_err(|error| {
                renderpilot_application::AppError::storage_failed(error.to_string())
            })
        })
        .transpose()?;
    Ok(PendingSharedVulkanMutationRow {
        id: row.get("id").map_err(storage_error)?,
        scope,
        game_id,
        feature: row.get("feature").map_err(storage_error)?,
        state: row
            .get::<_, String>("state")
            .map_err(storage_error)?
            .parse()?,
        manifest_json: row.get("manifest_json").map_err(storage_error)?,
        root_capabilities_json: row.get("root_capabilities_json").map_err(storage_error)?,
    })
}
