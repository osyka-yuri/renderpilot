//! Canonical DDL for `shared_artifacts` (baseline + v8→v9 upgrade).

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

use super::common::{MS_UNIXEPOCH_DEFAULT, touch_updated_at_trigger};

/// Full CREATE for greenfield / upgrade (idempotent).
pub(in crate::schema) fn create_table_sql() -> String {
    format!(
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
        {default}
    ),
    updated_at           INTEGER NOT NULL DEFAULT (
        {default}
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
"#,
        default = MS_UNIXEPOCH_DEFAULT,
    )
}

pub(in crate::schema) fn touch_trigger_sql() -> String {
    touch_updated_at_trigger(
        "trg_shared_artifacts_touch_updated_at",
        "shared_artifacts",
        "kind",
    )
}

/// Applies table + touch trigger (used by v8→v9).
pub(in crate::schema) fn apply(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(&format!("{}\n{}", create_table_sql(), touch_trigger_sql()))
        .map_err(|error| storage_context("could not apply shared_artifacts DDL", error))
}
