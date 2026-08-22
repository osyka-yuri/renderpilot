use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::GameId;
use rusqlite::{OptionalExtension, Row, named_params};

use crate::{error::storage_error, sqlite_clock};

use super::super::{SqliteStorage, observations};
use super::{
    binding::classify_catalog_binding_within_transaction,
    model::{
        BeginFileMutationPreparation, CatalogBinding, PendingFileMutationRow,
        PendingFileMutationState,
    },
};

impl SqliteStorage {
    /// Reserves a mutation before its first game-folder write.
    ///
    /// This always inserts literal `preparing`. The initial manifest may be
    /// incomplete but must be a JSON object; finishing requires the full
    /// before-snapshot manifest and atomically invalidates catalog authority.
    pub fn begin_file_mutation_preparation(
        &self,
        begin: &BeginFileMutationPreparation,
    ) -> AppResult<()> {
        validate_begin_preparation(begin)?;
        self.with_immediate_transaction(|transaction| {
            super::super::pending_shared_vulkan_mutations::assert_no_shared_mutation_id_within_transaction(
                transaction,
                &begin.id,
            )?;
            super::super::pending_shared_vulkan_mutations::assert_no_shared_mutation_for_game_within_transaction(
                transaction,
                &begin.game_id,
            )?;
            let now_ms = sqlite_clock::now_ms(transaction)?;
            transaction
                .execute(
                    "
                    INSERT INTO pending_file_mutations
                        (id, game_id, feature, subject_id, state, manifest_json,
                         created_at, updated_at)
                    VALUES
                        (:id, :game_id, :feature, :subject_id, :state, :manifest_json,
                         :now_ms, :now_ms)
                    ",
                    named_params! {
                        ":id": begin.id.as_str(),
                        ":game_id": begin.game_id.as_str(),
                        ":feature": begin.feature.as_str(),
                        ":subject_id": begin.subject_id.as_deref(),
                        ":state": PendingFileMutationState::Preparing.as_str(),
                        ":manifest_json": begin.initial_manifest_json.as_str(),
                        ":now_ms": now_ms,
                    },
                )
                .map_err(storage_error)?;
            Ok(())
        })
    }

    /// Lists unfinished or not-yet-cleaned mutations for one game oldest first.
    pub fn pending_file_mutations_for_game(
        &self,
        game_id: &GameId,
    ) -> AppResult<Vec<PendingFileMutationRow>> {
        self.query_list(
            "
            SELECT id, game_id, feature, subject_id, state, manifest_json
            FROM pending_file_mutations
            WHERE game_id = ?1
            ORDER BY created_at, id
            ",
            [game_id.as_str()],
            |row| Ok(row_to_pending_mutation(row)),
        )
    }

    /// Returns every reserved transaction id for orphan-directory sweeping.
    pub fn all_pending_file_mutation_ids(&self) -> AppResult<Vec<String>> {
        self.query_list(
            "SELECT id FROM pending_file_mutations ORDER BY created_at, id",
            [],
            |row| Ok(row.get::<_, String>(0).map_err(storage_error)),
        )
    }

    /// Publishes the completed before-snapshot manifest and opens the game-file
    /// mutation phase.
    pub fn finish_preparing_file_mutation(&self, id: &str, manifest_json: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            let game_id: String = transaction
                .query_row(
                    "SELECT game_id FROM pending_file_mutations
                     WHERE id = ?1 AND state = 'preparing'",
                    [id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(storage_error)?
                .ok_or_else(|| {
                    AppError::storage_failed(format!(
                        "pending file mutation '{id}' is missing or is not preparing"
                    ))
                })?;
            validate_prepared_manifest(manifest_json)?;
            let game_id = GameId::new(game_id)
                .map_err(|error| AppError::storage_failed(error.to_string()))?;
            let binding = classify_catalog_binding_within_transaction(transaction, &game_id)?;
            let now_ms = sqlite_clock::now_ms(transaction)?;
            let updated = transaction
                .execute(
                    "
                    UPDATE pending_file_mutations
                    SET state = 'prepared', manifest_json = :manifest_json, updated_at = :now_ms
                    WHERE id = :id AND state = 'preparing'
                    ",
                    named_params! {
                        ":id": id,
                        ":manifest_json": manifest_json,
                        ":now_ms": now_ms,
                    },
                )
                .map_err(storage_error)?;
            if updated != 1 {
                return Err(AppError::storage_failed(format!(
                    "pending file mutation `{id}` is missing or is not preparing"
                )));
            }
            if matches!(binding, CatalogBinding::CatalogPresent(_)) {
                observations::invalidate_game_authority_within_transaction(
                    transaction,
                    &game_id,
                    "prepared_file_mutation",
                    Some(id),
                )?;
            }
            Ok(())
        })
    }

    /// Deletes only an unfinished preparation after its app-owned snapshots
    /// were cleaned without touching game paths.
    pub fn abandon_file_mutation_preparation(&self, id: &str) -> AppResult<()> {
        delete_pending_file_mutation_in_state(self, id, PendingFileMutationState::Preparing)
    }

    /// Deletes a committed row after app-owned snapshot cleanup.
    pub fn cleanup_committed_file_mutation(&self, id: &str) -> AppResult<()> {
        delete_pending_file_mutation_in_state(self, id, PendingFileMutationState::Committed)
    }

    /// Reads one mutation by id for tests and recovery diagnostics.
    pub fn get_pending_file_mutation(&self, id: &str) -> AppResult<Option<PendingFileMutationRow>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "
                    SELECT id, game_id, feature, subject_id, state, manifest_json
                    FROM pending_file_mutations WHERE id = ?1
                    ",
                    [id],
                    |row| Ok(row_to_pending_mutation(row)),
                )
                .optional()
                .map_err(storage_error)?
                .transpose()
        })
    }

    /// Test-only malformed-state fixture. Production callers cannot insert a
    /// caller-selected state; they must use `begin_file_mutation_preparation`.
    #[cfg(test)]
    pub(crate) fn prepare_file_mutation(&self, row: &PendingFileMutationRow) -> AppResult<()> {
        self.with_immediate_transaction(|transaction| {
            super::super::pending_shared_vulkan_mutations::assert_no_shared_mutation_id_within_transaction(
                transaction,
                &row.id,
            )?;
            super::super::pending_shared_vulkan_mutations::assert_no_shared_mutation_for_game_within_transaction(
                transaction,
                &row.game_id,
            )?;
            let now_ms = sqlite_clock::now_ms(transaction)?;
            transaction
                .execute(
                    "INSERT INTO pending_file_mutations
                     (id, game_id, feature, subject_id, state, manifest_json, created_at, updated_at)
                     VALUES (:id, :game_id, :feature, :subject_id, :state, :manifest_json, :now_ms, :now_ms)",
                    named_params! {
                        ":id": row.id.as_str(),
                        ":game_id": row.game_id.as_str(),
                        ":feature": row.feature.as_str(),
                        ":subject_id": row.subject_id.as_deref(),
                        ":state": row.state.as_str(),
                        ":manifest_json": row.manifest_json.as_str(),
                        ":now_ms": now_ms,
                    },
                )
                .map_err(storage_error)?;
            Ok(())
        })
    }
}

fn validate_begin_preparation(begin: &BeginFileMutationPreparation) -> AppResult<()> {
    for (field, value) in [
        ("id", begin.id.as_str()),
        ("feature", begin.feature.as_str()),
    ] {
        if value.trim().is_empty() || value.contains('\0') {
            return Err(AppError::storage_failed(format!(
                "pending file mutation {field} is invalid"
            )));
        }
    }
    if begin
        .subject_id
        .as_deref()
        .is_some_and(|value| value.trim().is_empty() || value.contains('\0'))
    {
        return Err(AppError::storage_failed(
            "pending file mutation subject id is invalid",
        ));
    }
    let manifest: serde_json::Value =
        serde_json::from_str(&begin.initial_manifest_json).map_err(|error| {
            AppError::storage_failed(format!("invalid initial file mutation manifest: {error}"))
        })?;
    if !manifest.is_object() {
        return Err(AppError::storage_failed(
            "initial file mutation manifest must be a JSON object",
        ));
    }
    Ok(())
}

fn delete_pending_file_mutation_in_state(
    storage: &SqliteStorage,
    id: &str,
    state: PendingFileMutationState,
) -> AppResult<()> {
    storage.with_connection(|connection| {
        let deleted = connection
            .execute(
                "DELETE FROM pending_file_mutations WHERE id = ?1 AND state = ?2",
                [id, state.as_str()],
            )
            .map_err(storage_error)?;
        if deleted != 1 {
            return Err(AppError::storage_failed(format!(
                "pending file mutation '{id}' is not {}",
                state.as_str()
            )));
        }
        Ok(())
    })
}

fn row_to_pending_mutation(row: &Row<'_>) -> AppResult<PendingFileMutationRow> {
    Ok(PendingFileMutationRow {
        id: row.get("id").map_err(storage_error)?,
        game_id: GameId::new(row.get::<_, String>("game_id").map_err(storage_error)?)
            .map_err(|error| AppError::storage_failed(error.to_string()))?,
        feature: row.get("feature").map_err(storage_error)?,
        subject_id: row.get("subject_id").map_err(storage_error)?,
        state: row
            .get::<_, String>("state")
            .map_err(storage_error)?
            .parse::<PendingFileMutationState>()?,
        manifest_json: row.get("manifest_json").map_err(storage_error)?,
    })
}

/// Storage validates only the durable boundary shape. Orchestration remains the
/// authority for roots and snapshot locations, but malformed JSON must never
/// transition a row to Prepared or invalidate a catalog.
fn validate_prepared_manifest(manifest_json: &str) -> AppResult<()> {
    let manifest: serde_json::Value = serde_json::from_str(manifest_json).map_err(|error| {
        AppError::storage_failed(format!("invalid pending file mutation manifest: {error}"))
    })?;
    let snapshots = manifest
        .as_object()
        .and_then(|object| object.get("snapshots"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            AppError::storage_failed(
                "pending file mutation manifest must be an object with snapshots array",
            )
        })?;
    for snapshot in snapshots {
        let path = snapshot
            .as_object()
            .and_then(|object| object.get("path"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                AppError::storage_failed(
                    "pending file mutation manifest snapshots require a string path",
                )
            })?;
        if path.trim().is_empty() || path.contains('\0') {
            return Err(AppError::storage_failed(
                "pending file mutation manifest contains an invalid target path",
            ));
        }
    }
    Ok(())
}
