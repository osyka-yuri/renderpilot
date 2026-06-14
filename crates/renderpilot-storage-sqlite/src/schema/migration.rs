//! Decides whether to keep, initialize, or rebuild the catalog schema, and
//! applies the bundled migration and version stamping.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

use super::objects::drop_user_schema_objects;
use super::validation::{catalog_schema_is_valid, validate_catalog_schema};
use super::{CURRENT_SCHEMA_VERSION, INITIAL_MIGRATION};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MigrationAction {
    Keep,
    ApplyInitial,
    Rebuild,
}

pub(super) fn determine_migration_action(connection: &Connection) -> AppResult<MigrationAction> {
    let schema_version = read_user_version(connection)?;

    match schema_version {
        CURRENT_SCHEMA_VERSION => {
            if catalog_schema_is_valid(connection)? {
                Ok(MigrationAction::Keep)
            } else {
                Ok(MigrationAction::Rebuild)
            }
        }
        0 => {
            if database_has_user_schema(connection)? {
                Ok(MigrationAction::Rebuild)
            } else {
                Ok(MigrationAction::ApplyInitial)
            }
        }
        _ => Ok(MigrationAction::Rebuild),
    }
}

pub(super) fn reset_catalog_schema(connection: &Connection) -> AppResult<()> {
    drop_user_schema_objects(connection)?;
    apply_initial_migration(connection)?;
    set_user_version(connection, CURRENT_SCHEMA_VERSION)?;
    validate_catalog_schema(connection)
}

pub(super) fn apply_initial_migration(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(INITIAL_MIGRATION)
        .map_err(|error| storage_context("could not apply sqlite initial migration", error))
}

fn read_user_version(connection: &Connection) -> AppResult<i32> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| storage_context("could not read sqlite catalog schema version", error))
}

pub(super) fn set_user_version(connection: &Connection, version: i32) -> AppResult<()> {
    debug_assert!(version >= 0);

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
