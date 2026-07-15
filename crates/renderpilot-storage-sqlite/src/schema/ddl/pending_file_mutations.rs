//! Canonical DDL for `pending_file_mutations`.
//!
//! Single source of truth for baseline composition, v9→v10 upgrades, and
//! soft-heal of WIP catalogs that stamped v10 with a CHECK that rejected
//! `preparing`.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

use super::super::objects::{SchemaObjectKind, object_exists};
use super::common::MS_UNIXEPOCH_DEFAULT;

const TABLE_NAME: &str = "pending_file_mutations";
const TEMP_TABLE_NAME: &str = "pending_file_mutations_new";

/// Column definitions and CHECKs shared by all CREATE variants.
fn table_body() -> String {
    format!(
        r#"
    id             TEXT    PRIMARY KEY NOT NULL,
    game_id        TEXT    NOT NULL,
    feature        TEXT    NOT NULL,
    subject_id     TEXT,
    state          TEXT    NOT NULL,
    manifest_json  TEXT    NOT NULL,
    created_at     INTEGER NOT NULL DEFAULT (
        {default}
    ),
    updated_at     INTEGER NOT NULL DEFAULT (
        {default}
    ),
    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(feature)) > 0),
    CHECK (subject_id IS NULL OR length(trim(subject_id)) > 0),
    CHECK (state IN ('preparing', 'prepared', 'committed')),
    CHECK (json_valid(manifest_json)),
    CHECK (json_type(manifest_json) = 'object'),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at)
"#,
        default = MS_UNIXEPOCH_DEFAULT,
    )
}

fn create_table_sql(table_name: &str, if_not_exists: bool) -> String {
    let if_clause = if if_not_exists { "IF NOT EXISTS " } else { "" };
    format!(
        "CREATE TABLE {if_clause}{table_name} ({body}) STRICT",
        body = table_body()
    )
}

const CREATE_INDEX_SQL: &str = r#"
CREATE INDEX IF NOT EXISTS idx_pending_file_mutations_game_id
    ON pending_file_mutations(game_id)
"#;

/// Baseline fragment: IF NOT EXISTS table + index.
pub(super) fn baseline_sql() -> String {
    format!(
        "{};\n{};",
        create_table_sql(TABLE_NAME, true),
        CREATE_INDEX_SQL.trim()
    )
}

/// Ensures the live table accepts `preparing` (and exists).
///
/// - Missing → create full DDL.
/// - Present with correct CHECK → no-op.
/// - Present with legacy CHECK (`prepared`/`committed` only) → recreate and
///   copy surviving rows. Does not bump `user_version`.
pub(in crate::schema) fn ensure_correct_shape(connection: &Connection) -> AppResult<()> {
    if !object_exists(connection, SchemaObjectKind::Table, TABLE_NAME)? {
        return create_fresh(connection);
    }
    if allows_preparing(connection)? {
        return Ok(());
    }
    recreate_preserving_rows(connection)
}

/// Probe used by schema contract validation.
pub(in crate::schema) fn allows_preparing(connection: &Connection) -> AppResult<bool> {
    if !object_exists(connection, SchemaObjectKind::Table, TABLE_NAME)? {
        return Ok(false);
    }

    connection
        .execute_batch("SAVEPOINT probe_pending_preparing")
        .map_err(|error| storage_context("could not open pending_file_mutations probe", error))?;
    let probe = connection.execute(
        "
        INSERT INTO pending_file_mutations
            (id, game_id, feature, subject_id, state, manifest_json)
        VALUES
            ('__schema_probe_preparing__', 'probe:game', 'schema_probe', NULL,
             'preparing', '{\"snapshots\":[]}')
        ",
        [],
    );
    let _ = connection.execute_batch("ROLLBACK TO probe_pending_preparing");
    let _ = connection.execute_batch("RELEASE probe_pending_preparing");

    match probe {
        Ok(_) => Ok(true),
        Err(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Ok(false)
        }
        Err(error) => Err(storage_context(
            "could not probe pending_file_mutations preparing state",
            error,
        )),
    }
}

fn create_fresh(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(&format!(
            "{};\n{};",
            create_table_sql(TABLE_NAME, false),
            CREATE_INDEX_SQL
        ))
        .map_err(|error| storage_context("could not create pending_file_mutations", error))
}

fn recreate_preserving_rows(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(&format!("DROP TABLE IF EXISTS {TEMP_TABLE_NAME};"))
        .map_err(|error| {
            storage_context("could not prepare pending_file_mutations rebuild", error)
        })?;

    connection
        .execute_batch(&format!(
            "{create_temp};\n\
             INSERT INTO {TEMP_TABLE_NAME}\n\
                 (id, game_id, feature, subject_id, state, manifest_json, created_at, updated_at)\n\
             SELECT id, game_id, feature, subject_id, state, manifest_json, created_at, updated_at\n\
               FROM {TABLE_NAME}\n\
              WHERE state IN ('prepared', 'committed', 'preparing');\n\
             DROP TABLE {TABLE_NAME};\n\
             ALTER TABLE {TEMP_TABLE_NAME} RENAME TO {TABLE_NAME};\n\
             {CREATE_INDEX_SQL};",
            create_temp = create_table_sql(TEMP_TABLE_NAME, false),
        ))
        .map_err(|error| storage_context("could not rebuild pending_file_mutations", error))
}
