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
    MigrateV8ToV9,
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
        8 => Ok(MigrationAction::MigrateV8ToV9),
        _ => Ok(MigrationAction::Rebuild),
    }
}

pub(super) fn migrate_v8_to_v9(connection: &Connection) -> AppResult<()> {
    ensure_installed_addons_column(
        connection,
        "host_kind",
        "ALTER TABLE installed_addons ADD COLUMN host_kind TEXT",
    )?;
    ensure_installed_addons_column(
        connection,
        "reshade_channel",
        "ALTER TABLE installed_addons ADD COLUMN reshade_channel TEXT",
    )?;
    ensure_installed_addons_column(
        connection,
        "registered_exe_path",
        "ALTER TABLE installed_addons ADD COLUMN registered_exe_path TEXT",
    )?;

    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS shared_artifacts (
                kind                 TEXT    PRIMARY KEY NOT NULL,
                install_dir          TEXT    NOT NULL,
                manifest_path        TEXT    NOT NULL,
                dll_path             TEXT    NOT NULL,
                source_url           TEXT,
                source_etag          TEXT,
                source_digest        TEXT,
                source_last_modified TEXT,
                channel              TEXT,
                origin               TEXT    NOT NULL,
                created_files_json   TEXT    NOT NULL DEFAULT '[]',
                created_at           INTEGER NOT NULL DEFAULT (
                    CAST(unixepoch('subsec') * 1000 AS INTEGER)
                ),
                updated_at           INTEGER NOT NULL DEFAULT (
                    CAST(unixepoch('subsec') * 1000 AS INTEGER)
                ),

                CHECK (length(trim(kind)) > 0),
                CHECK (length(trim(install_dir)) > 0),
                CHECK (instr(install_dir, char(0)) = 0),
                CHECK (length(trim(manifest_path)) > 0),
                CHECK (instr(manifest_path, char(0)) = 0),
                CHECK (length(trim(dll_path)) > 0),
                CHECK (instr(dll_path, char(0)) = 0),
                CHECK (source_url IS NULL OR length(trim(source_url)) > 0),
                CHECK (source_etag IS NULL OR length(trim(source_etag)) > 0),
                CHECK (source_digest IS NULL OR length(trim(source_digest)) > 0),
                CHECK (source_last_modified IS NULL OR length(trim(source_last_modified)) > 0),
                CHECK (channel IS NULL OR length(trim(channel)) > 0),
                CHECK (length(trim(origin)) > 0),
                CHECK (json_valid(created_files_json)),
                CHECK (json_type(created_files_json) = 'array'),
                CHECK (created_at >= 0),
                CHECK (updated_at >= created_at)
            ) STRICT;

            CREATE TRIGGER IF NOT EXISTS trg_shared_artifacts_touch_updated_at
            AFTER UPDATE ON shared_artifacts
            FOR EACH ROW
            WHEN NEW.updated_at = OLD.updated_at
            BEGIN
                UPDATE shared_artifacts
                   SET updated_at = max(
                       CAST(unixepoch('subsec') * 1000 AS INTEGER),
                       OLD.updated_at + 1
                   )
                 WHERE kind = NEW.kind;
            END;
            "#,
        )
        .map_err(|error| storage_context("could not migrate sqlite catalog schema to v9", error))?;

    set_user_version(connection, CURRENT_SCHEMA_VERSION)?;
    validate_catalog_schema(connection)
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

fn ensure_installed_addons_column(
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
