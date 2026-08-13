//! Canonical DDL for `pending_file_mutations`.
//!
//! Single source of truth for baseline composition and the released v9→v10
//! upgrade.

use renderpilot_application::AppResult;
use rusqlite::{Connection, OptionalExtension};

use crate::error::storage_context;

use super::super::objects::{SchemaObjectKind, object_exists};
use super::common::MS_UNIXEPOCH_DEFAULT;

const TABLE_NAME: &str = "pending_file_mutations";

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

/// Creates the canonical table and index for the released v9→v10 transition.
///
/// Released v9 catalogs do not contain this table. Any other shape is handled
/// by the general schema policy rather than repaired at this historical edge.
/// A preexisting table is left untouched so post-upgrade validation can decide
/// whether the whole catalog needs its backed reset.
pub(in crate::schema) fn create_for_released_v9_to_v10(connection: &Connection) -> AppResult<()> {
    create_canonical(connection)
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

/// Read-only form of [`allows_preparing`] for validators that must never write.
pub(in crate::schema) fn allows_preparing_observational(
    connection: &Connection,
) -> AppResult<bool> {
    let Some(sql) = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [TABLE_NAME],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| storage_context("could not read pending_file_mutations DDL", error))?
        .flatten()
    else {
        return Ok(false);
    };
    let normalized = sql
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();

    Ok(normalized.contains("check(statein('preparing','prepared','committed'))"))
}

fn create_canonical(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(&format!(
            "{};\n{};",
            create_table_sql(TABLE_NAME, true),
            CREATE_INDEX_SQL
        ))
        .map_err(|error| storage_context("could not create pending_file_mutations", error))
}
