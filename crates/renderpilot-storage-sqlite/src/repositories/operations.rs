use renderpilot_application::{
    AppResult, OperationItemRecord, OperationJournalEntry, OperationRecord, OperationRepository,
};
use renderpilot_domain::{GameId, OperationId};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    error::{storage_context, storage_error},
    sqlite_clock,
};

use super::{
    catalog_select_sql::{
        SELECT_OPERATIONS_FOR_GAME_SQL, SELECT_OPERATION_ITEMS_SQL, SELECT_OPERATION_SQL,
    },
    row_mapping::{collect_rows, operation_from_row, operation_item_from_row},
    SqliteStorage,
};

const UPSERT_OPERATION_SQL: &str = "
    INSERT INTO operations
        (id, game_id, kind, status, created_at, completed_at, metadata_json, updated_at)
    VALUES
        (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
    ON CONFLICT(id) DO UPDATE SET
        game_id = excluded.game_id,
        kind = excluded.kind,
        status = excluded.status,
        created_at = excluded.created_at,
        completed_at = excluded.completed_at,
        metadata_json = excluded.metadata_json,
        updated_at = excluded.updated_at
";

const DELETE_OPERATION_ITEMS_SQL: &str = "
    DELETE FROM operation_items
    WHERE operation_id = ?1
";

const INSERT_OPERATION_ITEM_SQL: &str = "
    INSERT INTO operation_items
        (
            operation_id,
            game_id,
            component_id,
            artifact_id,
            source_path,
            target_path,
            status,
            metadata_json
        )
    VALUES
        (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
";

const COUNT_OPERATION_ITEMS_SQL: &str = "
    SELECT COUNT(*)
    FROM operation_items
    WHERE operation_id = ?1
";

impl OperationRepository for SqliteStorage {
    fn save_operation_entry(&self, entry: &OperationJournalEntry) -> AppResult<()> {
        entry.validate()?;

        self.with_transaction(|transaction| {
            upsert_operation_within_transaction(transaction, entry.operation())?;

            replace_operation_items_within_transaction(
                transaction,
                entry.operation(),
                entry.items(),
            )
        })
    }

    fn find_operation_entry(
        &self,
        operation_id: &OperationId,
    ) -> AppResult<Option<OperationJournalEntry>> {
        self.with_connection_mut(|connection| {
            let tx = connection.transaction().map_err(|error| {
                storage_context(
                    "failed to begin read transaction for operation journal entry",
                    error,
                )
            })?;

            let result = find_operation_entry_in_transaction(&tx, operation_id);

            // Do not commit this transaction; dropping it rolls back. Avoid `rollback()?`
            // so a rollback failure cannot mask the primary error in `result`.
            drop(tx);

            result
        })
    }

    fn list_operation_headers_for_game(&self, game_id: &GameId) -> AppResult<Vec<OperationRecord>> {
        self.query_list(
            SELECT_OPERATIONS_FOR_GAME_SQL,
            [game_id.as_str()],
            operation_from_row,
        )
    }

    fn count_operation_items(&self, operation_id: &OperationId) -> AppResult<usize> {
        self.with_connection(|connection| {
            let count: i64 = connection
                .query_row(COUNT_OPERATION_ITEMS_SQL, [operation_id.as_str()], |row| {
                    row.get(0)
                })
                .map_err(storage_error)?;

            usize::try_from(count).map_err(|_| {
                storage_error("operation item count returned by sqlite does not fit usize")
            })
        })
    }
}

fn find_operation_on_connection(
    connection: &Connection,
    operation_id: &OperationId,
) -> AppResult<Option<OperationRecord>> {
    let mut statement = connection
        .prepare_cached(SELECT_OPERATION_SQL)
        .map_err(storage_error)?;

    statement
        .query_row([operation_id.as_str()], operation_from_row)
        .optional()
        .map_err(storage_error)?
        .transpose()
}

fn list_operation_items_on_connection(
    connection: &Connection,
    operation_id: &OperationId,
) -> AppResult<Vec<OperationItemRecord>> {
    let mut statement = connection
        .prepare_cached(SELECT_OPERATION_ITEMS_SQL)
        .map_err(storage_error)?;

    let rows = statement
        .query_map([operation_id.as_str()], operation_item_from_row)
        .map_err(storage_error)?;

    collect_rows(rows)
}

fn find_operation_entry_in_transaction(
    connection: &Connection,
    operation_id: &OperationId,
) -> AppResult<Option<OperationJournalEntry>> {
    let Some(operation) = find_operation_on_connection(connection, operation_id)? else {
        return Ok(None);
    };

    let items = list_operation_items_on_connection(connection, operation_id)?;
    OperationJournalEntry::try_new(operation, items).map(Some)
}

fn upsert_operation_within_transaction(
    transaction: &Transaction<'_>,
    operation: &OperationRecord,
) -> AppResult<()> {
    let sqlite_now_ms = sqlite_clock::now_ms(transaction)?;
    let updated_at_ms =
        sqlite_clock::operation_updated_at_ms(sqlite_now_ms, operation.created_at.as_i64());

    transaction
        .execute(
            UPSERT_OPERATION_SQL,
            params![
                operation.id.as_str(),
                operation.game_id.as_str(),
                operation.kind.as_str(),
                operation.status.as_str(),
                operation.created_at.as_i64(),
                operation.completed_at.map(|timestamp| timestamp.as_i64()),
                operation.metadata_json.as_deref(),
                updated_at_ms,
            ],
        )
        .map_err(storage_error)?;

    Ok(())
}

fn replace_operation_items_within_transaction(
    transaction: &Transaction<'_>,
    operation: &OperationRecord,
    items: &[OperationItemRecord],
) -> AppResult<()> {
    delete_operation_items_within_transaction(transaction, &operation.id)?;
    insert_operation_items_within_transaction(transaction, operation, items)
}

fn delete_operation_items_within_transaction(
    transaction: &Transaction<'_>,
    operation_id: &OperationId,
) -> AppResult<()> {
    transaction
        .execute(DELETE_OPERATION_ITEMS_SQL, [operation_id.as_str()])
        .map_err(storage_error)?;

    Ok(())
}

fn insert_operation_items_within_transaction(
    transaction: &Transaction<'_>,
    operation: &OperationRecord,
    items: &[OperationItemRecord],
) -> AppResult<()> {
    let mut statement = transaction
        .prepare_cached(INSERT_OPERATION_ITEM_SQL)
        .map_err(storage_error)?;

    for item in items {
        statement
            .execute(params![
                operation.id.as_str(),
                operation.game_id.as_str(),
                item.component_id.as_str(),
                item.artifact_id.as_ref().map(|id| id.as_str()),
                item.source_path.as_str(),
                item.target_path.as_ref().map(|path| path.as_str()),
                item.status.as_str(),
                item.metadata_json.as_deref(),
            ])
            .map_err(storage_error)?;
    }

    Ok(())
}
