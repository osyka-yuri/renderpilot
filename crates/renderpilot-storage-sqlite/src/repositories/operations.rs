use renderpilot_application::{
    AppResult, OperationItemRecord, OperationRecord, OperationRepository,
};
use renderpilot_domain::{GameId, OperationId};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::storage_error;

use super::{
    row_mapping::{operation_from_row, operation_item_from_row},
    SqliteStorage,
};

impl OperationRepository for SqliteStorage {
    fn upsert_operation(&self, operation: &OperationRecord) -> AppResult<()> {
        let connection = self.connection()?;

        connection
            .execute(
                "INSERT INTO operations
                    (id, game_id, kind, status, created_at, completed_at, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    game_id = excluded.game_id,
                    kind = excluded.kind,
                    status = excluded.status,
                    created_at = excluded.created_at,
                    completed_at = excluded.completed_at,
                    metadata_json = excluded.metadata_json",
                params![
                    operation.id.as_str(),
                    operation.game_id.as_str(),
                    operation.kind.as_str(),
                    operation.status.as_str(),
                    operation.created_at.as_i64(),
                    operation.completed_at.map(|timestamp| timestamp.as_i64()),
                    operation.metadata_json.as_deref()
                ],
            )
            .map_err(storage_error)?;

        Ok(())
    }

    fn find_operation(&self, operation_id: &OperationId) -> AppResult<Option<OperationRecord>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, game_id, kind, status, created_at, completed_at, metadata_json
                 FROM operations
                 WHERE id = ?1",
            )
            .map_err(storage_error)?;

        statement
            .query_row([operation_id.as_str()], operation_from_row)
            .optional()
            .map_err(storage_error)?
            .transpose()
    }

    fn list_operations_for_game(&self, game_id: &GameId) -> AppResult<Vec<OperationRecord>> {
        self.query_list(
            "SELECT id, game_id, kind, status, created_at, completed_at, metadata_json
             FROM operations
             WHERE game_id = ?1
             ORDER BY created_at, id",
            [game_id.as_str()],
            operation_from_row,
        )
    }

    fn replace_operation_items(
        &self,
        operation_id: &OperationId,
        items: &[OperationItemRecord],
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            transaction
                .execute(
                    "DELETE FROM operation_items WHERE operation_id = ?1",
                    [operation_id.as_str()],
                )
                .map_err(storage_error)?;

            for item in items {
                insert_operation_item(transaction, item)?;
            }

            Ok(())
        })
    }

    fn list_operation_items(
        &self,
        operation_id: &OperationId,
    ) -> AppResult<Vec<OperationItemRecord>> {
        self.query_list(
            "SELECT operation_id, component_id, artifact_id, source_path, target_path, status,
                    metadata_json
             FROM operation_items
             WHERE operation_id = ?1
             ORDER BY id",
            [operation_id.as_str()],
            operation_item_from_row,
        )
    }
}

fn insert_operation_item(connection: &Connection, item: &OperationItemRecord) -> AppResult<()> {
    connection
        .execute(
            "INSERT INTO operation_items
                (operation_id, component_id, artifact_id, source_path, target_path, status,
                 metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                item.operation_id.as_str(),
                item.component_id.as_str(),
                item.artifact_id.as_ref().map(|id| id.as_str()),
                item.source_path.as_str(),
                item.target_path.as_ref().map(|path| path.as_str()),
                item.status.as_str(),
                item.metadata_json.as_deref()
            ],
        )
        .map_err(storage_error)?;

    Ok(())
}
