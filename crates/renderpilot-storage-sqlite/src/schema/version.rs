//! Catalog schema version (`PRAGMA user_version`).

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

pub(super) fn read(connection: &Connection) -> AppResult<i32> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| storage_context("could not read sqlite catalog schema version", error))
}

pub(super) fn write(connection: &Connection, version: i32) -> AppResult<()> {
    debug_assert!(version >= 0);
    connection
        .execute_batch(&format!("PRAGMA user_version = {version}"))
        .map_err(|error| storage_context("could not write sqlite catalog schema version", error))
}

pub(super) fn database_has_user_schema(connection: &Connection) -> AppResult<bool> {
    let object_count: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type IN ('table', 'index', 'trigger', 'view')
              AND name NOT LIKE 'sqlite_%'
            ",
            [],
            |row| row.get(0),
        )
        .map_err(|error| storage_context("could not inspect existing sqlite schema", error))?;

    Ok(object_count > 0)
}
