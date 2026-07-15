//! Shared helpers for additive upgrade steps.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

pub(super) fn ensure_installed_addons_column(
    connection: &Connection,
    column_name: &str,
    alter_sql: &str,
) -> AppResult<()> {
    if table_has_column(connection, "installed_addons", column_name)? {
        return Ok(());
    }

    connection
        .execute_batch(alter_sql)
        .map_err(|error| storage_context("could not add installed_addons metadata column", error))
}

fn table_has_column(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
) -> AppResult<bool> {
    connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM pragma_table_info(?1)
            WHERE name = ?2
            ",
            [table_name, column_name],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(|error| storage_context("could not inspect sqlite table columns", error))
}
