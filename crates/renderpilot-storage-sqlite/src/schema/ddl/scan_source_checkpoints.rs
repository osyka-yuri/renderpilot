use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

const SQL: &str = r#"
CREATE TABLE IF NOT EXISTS scan_source_checkpoints (
    source_key  TEXT    PRIMARY KEY NOT NULL,
    fingerprint TEXT    NOT NULL,
    updated_at  INTEGER NOT NULL DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER)),
    CHECK (length(trim(source_key)) > 0),
    CHECK (length(trim(fingerprint)) > 0),
    CHECK (updated_at >= 0)
) STRICT;
CREATE TRIGGER IF NOT EXISTS trg_scan_source_checkpoints_touch_updated_at
AFTER UPDATE ON scan_source_checkpoints
FOR EACH ROW WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE scan_source_checkpoints
       SET updated_at = max(CAST(unixepoch('subsec') * 1000 AS INTEGER), OLD.updated_at + 1)
     WHERE source_key = NEW.source_key;
END;
"#;

pub(crate) fn apply(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(SQL)
        .map_err(|error| storage_context("could not create scan source checkpoints", error))
}
