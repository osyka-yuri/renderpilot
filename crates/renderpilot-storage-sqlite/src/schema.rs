use renderpilot_application::AppResult;
use rusqlite::{Connection, Error as SqliteError, TransactionBehavior};

use crate::error::storage_context;

const INITIAL_MIGRATION: &str = include_str!("../migrations/0001_initial.sql");

const CURRENT_SCHEMA_VERSION: i32 = 1;
const GAMES_TABLE: &str = "games";

/// Applies the bundled catalog DDL.
///
/// There is no migration path from older on-disk schemas:
/// replace `catalog.db` if the schema version or shape does not match the
/// bundled initial schema.
pub(crate) fn apply(connection: &mut Connection) -> AppResult<()> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| storage_context("could not start sqlite migration transaction", error))?;

    match read_user_version(&transaction)? {
        CURRENT_SCHEMA_VERSION => {
            validate_catalog_schema(&transaction)?;
        }
        0 => {
            if database_has_user_schema(&transaction)? {
                return Err(storage_context(
                    "sqlite catalog contains an unversioned schema; replace catalog.db because migrations from older schemas are not supported",
                    SqliteError::InvalidQuery,
                ));
            }

            apply_initial_migration(&transaction)?;
            set_user_version(&transaction, CURRENT_SCHEMA_VERSION)?;
            validate_catalog_schema(&transaction)?;
        }
        _ => {
            return Err(storage_context(
                "sqlite catalog schema version is unsupported; replace catalog.db because migrations from older schemas are not supported",
                SqliteError::InvalidQuery,
            ));
        }
    }

    transaction
        .commit()
        .map_err(|error| storage_context("could not commit sqlite migration transaction", error))?;

    Ok(())
}

fn apply_initial_migration(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(INITIAL_MIGRATION)
        .map_err(|error| storage_context("could not apply sqlite initial migration", error))
}

fn read_user_version(connection: &Connection) -> AppResult<i32> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| storage_context("could not read sqlite catalog schema version", error))
}

fn set_user_version(connection: &Connection, version: i32) -> AppResult<()> {
    connection
        .execute_batch(&format!("PRAGMA user_version = {version}"))
        .map_err(|error| storage_context("could not write sqlite catalog schema version", error))
}

fn database_has_user_schema(connection: &Connection) -> AppResult<bool> {
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

fn validate_catalog_schema(connection: &Connection) -> AppResult<()> {
    if !table_exists(connection, GAMES_TABLE)? {
        return Err(storage_context(
            "sqlite catalog schema is missing the games table",
            SqliteError::InvalidQuery,
        ));
    }

    Ok(())
}

fn table_exists(connection: &Connection, table_name: &str) -> AppResult<bool> {
    let table_count: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = 'table'
              AND name = ?1
            ",
            [table_name],
            |row| row.get(0),
        )
        .map_err(|error| storage_context("could not inspect sqlite catalog table", error))?;

    Ok(table_count == 1)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{apply, CURRENT_SCHEMA_VERSION};

    #[test]
    fn apply_creates_catalog_schema() {
        let mut connection = Connection::open_in_memory().expect("sqlite should open");

        apply(&mut connection).expect("migration should succeed");

        assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
        assert!(games_column_count(&connection) > 0);
    }

    #[test]
    fn apply_is_idempotent() {
        let mut connection = Connection::open_in_memory().expect("sqlite should open");

        apply(&mut connection).expect("first migration should succeed");
        apply(&mut connection).expect("second migration should succeed");

        assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
        assert!(games_column_count(&connection) > 0);
    }

    #[test]
    fn apply_rejects_unversioned_existing_schema() {
        let mut connection = Connection::open_in_memory().expect("sqlite should open");

        connection
            .execute_batch("CREATE TABLE legacy_catalog_marker (id INTEGER PRIMARY KEY);")
            .expect("legacy schema should be created");

        assert!(apply(&mut connection).is_err());
        assert_eq!(user_version(&connection), 0);
    }

    #[test]
    fn apply_rejects_unknown_schema_version() {
        let mut connection = Connection::open_in_memory().expect("sqlite should open");

        connection
            .execute_batch("PRAGMA user_version = 999;")
            .expect("schema version should be set");

        assert!(apply(&mut connection).is_err());
        assert_eq!(user_version(&connection), 999);
    }

    fn user_version(connection: &Connection) -> i32 {
        connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("schema version should be readable")
    }

    fn games_column_count(connection: &Connection) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('games')",
                [],
                |row| row.get(0),
            )
            .expect("games table should exist")
    }
}
