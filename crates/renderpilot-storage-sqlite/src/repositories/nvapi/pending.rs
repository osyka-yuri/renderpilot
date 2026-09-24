use renderpilot_application::AppResult;
use rusqlite::params;

use super::NvapiPendingOperationRow;
use crate::{
    SqliteStorage,
    error::{storage_context, storage_error},
};
impl SqliteStorage {
    /// Lists unresolved DRS operations in stable order.
    pub fn list_pending_nvapi_operations(&self) -> AppResult<Vec<NvapiPendingOperationRow>> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(
                "SELECT op_id, game_id, target_id, kind, before_json, after_json, observed_json, phase,
                        created_at, updated_at
                   FROM pending_drs_operations ORDER BY created_at, op_id",
            ).map_err(|error| storage_context("could not prepare pending DRS operation list", error))?;
            let rows = statement.query_map([], |row| Ok(NvapiPendingOperationRow {
                op_id: row.get(0)?,
                game_id: row.get(1)?,
                target_id: row.get(2)?,
                kind: row.get(3)?,
                before_json: row.get(4)?,
                after_json: row.get(5)?,
                observed_json: row.get(6)?,
                phase: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })).map_err(|error| storage_context("could not read pending DRS operations", error))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| storage_context("could not map pending DRS operations", error))
        })
    }
}

impl SqliteStorage {
    /// Cancels a non-setting intent after DRS was observed in its recorded
    /// pre-mutation state.
    pub fn cancel_nvapi_operation(&self, op_id: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            let removed = transaction
                .execute(
                    "DELETE FROM pending_drs_operations WHERE op_id = ?1 AND phase != 'conflict'",
                    [op_id],
                )
                .map_err(storage_error)?;
            if removed != 1 {
                return Err(renderpilot_application::AppError::storage_failed(
                    "pending NVIDIA operation cannot be safely cancelled",
                ));
            }
            Ok(())
        })
    }

    /// Starts a durable mutation intent atomically, refusing to overlap any
    /// unresolved driver operation.
    pub fn begin_nvapi_operation(
        &self,
        op_id: &str,
        game_id: Option<&str>,
        target_id: Option<&str>,
        kind: &str,
        before_json: &str,
        after_json: &str,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            ensure_no_overlapping_nvapi_operations(
                transaction,
                target_id,
                before_json,
                after_json,
            )?;
            transaction
                .execute(
                    "INSERT INTO pending_drs_operations
                    (op_id, game_id, target_id, kind, before_json, after_json, phase)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'intent')",
                    params![op_id, game_id, target_id, kind, before_json, after_json],
                )
                .map_err(|error| storage_context("could not persist DRS mutation intent", error))?;
            Ok(())
        })
    }

    /// Marks a pending mutation as conflicting while preserving both snapshots.
    pub fn mark_nvapi_operation_conflict(&self, op_id: &str, observed_json: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            let changed = transaction
                .execute(
                    "UPDATE pending_drs_operations
                    SET phase = 'conflict', observed_json = ?2,
                        updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
                  WHERE op_id = ?1",
                    params![op_id, observed_json],
                )
                .map_err(|error| storage_context("could not mark DRS operation conflict", error))?;
            if changed != 1 {
                return Err(renderpilot_application::AppError::storage_failed(
                    "pending DRS operation receipt disappeared",
                ));
            }
            Ok(())
        })
    }
}

pub(super) fn ensure_no_overlapping_nvapi_operations(
    transaction: &rusqlite::Transaction<'_>,
    target_id: Option<&str>,
    before_json: &str,
    after_json: &str,
) -> AppResult<()> {
    let mut paths = Vec::new();
    for snapshot in [before_json, after_json] {
        let value: serde_json::Value = serde_json::from_str(snapshot)
            .map_err(|error| storage_context("invalid DRS operation scope receipt", error))?;
        for key in ["binding_path", "executable_path", "old_path", "new_path"] {
            if let Some(path) = value.get(key).and_then(serde_json::Value::as_str)
                && !path.trim().is_empty()
            {
                let mut path = path.replace('\\', "/");
                path.make_ascii_lowercase();
                paths.push(path);
            }
        }
    }
    // Match SQLite's built-in `lower()` for this transient EXISTS scope.
    paths.sort_unstable();
    paths.dedup();
    let paths_json = serde_json::to_string(&paths)
        .map_err(|error| storage_context("could not encode DRS operation scope", error))?;
    let pending: bool = transaction
        .query_row(
            "SELECT EXISTS(
            SELECT 1 FROM pending_drs_operations pending
             WHERE (?1 IS NOT NULL AND pending.target_id = ?1)
                OR EXISTS (
                    SELECT 1 FROM json_each(?2) requested
                     WHERE lower(requested.value) IN (
                         lower(json_extract(pending.before_json, '$.binding_path')),
                         lower(json_extract(pending.before_json, '$.executable_path')),
                         lower(json_extract(pending.before_json, '$.old_path')),
                         lower(json_extract(pending.before_json, '$.new_path')),
                         lower(json_extract(pending.after_json, '$.binding_path')),
                         lower(json_extract(pending.after_json, '$.executable_path')),
                         lower(json_extract(pending.after_json, '$.old_path')),
                         lower(json_extract(pending.after_json, '$.new_path'))
                     )
                )
        )",
            rusqlite::params![target_id, paths_json],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if pending {
        return Err(renderpilot_application::AppError::storage_failed(
            "an unresolved NVIDIA driver operation overlaps this profile or executable; inspect and recover it first",
        ));
    }
    Ok(())
}

pub(super) fn delete_pending_nvapi_operation(
    transaction: &rusqlite::Transaction<'_>,
    op_id: &str,
    kind: &str,
    game_id: Option<&str>,
    target_id: Option<&str>,
) -> AppResult<()> {
    require_pending_operation(transaction, op_id, kind, game_id, target_id)?;
    let deleted = transaction
        .execute(
            "DELETE FROM pending_drs_operations
          WHERE op_id = ?1 AND kind = ?2 AND phase != 'conflict'
            AND game_id IS ?3 AND target_id IS ?4",
            params![op_id, kind, game_id, target_id],
        )
        .map_err(storage_error)?;
    if deleted != 1 {
        return Err(renderpilot_application::AppError::storage_failed(
            "pending NVIDIA driver operation could not be finalized",
        ));
    }
    Ok(())
}

pub(super) fn require_pending_operation(
    transaction: &rusqlite::Transaction<'_>,
    op_id: &str,
    kind: &str,
    game_id: Option<&str>,
    target_id: Option<&str>,
) -> AppResult<()> {
    let found: bool = transaction
        .query_row(
            "SELECT EXISTS(
            SELECT 1 FROM pending_drs_operations
             WHERE op_id = ?1 AND kind = ?2 AND phase != 'conflict'
               AND game_id IS ?3 AND target_id IS ?4
        )",
            params![op_id, kind, game_id, target_id],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if !found {
        return Err(renderpilot_application::AppError::storage_failed(
            "pending NVIDIA operation does not match the requested action",
        ));
    }
    Ok(())
}

// -----------------------------------------------------------------------------
