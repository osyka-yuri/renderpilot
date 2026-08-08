use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

pub(in crate::schema) const TABLE_NAME: &str = "portable_path_tags";
pub(in crate::schema) const PORTABLE_DATA_PATH_TAG: &str = "portable-v1";

const SQL: &str = "
CREATE TABLE IF NOT EXISTS portable_path_tags (
    tag TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('portable_root_relative', 'external_absolute', 'virtual')),
    value TEXT NOT NULL,
    CHECK (length(trim(tag)) > 0),
    CHECK (length(trim(value)) > 0)
) STRICT;
INSERT OR IGNORE INTO portable_path_tags(tag, kind, value) VALUES
    ('portable-v1', 'portable_root_relative', 'data'),
    ('game_install_paths', 'external_absolute', 'preserved'),
    ('ui_virtual_paths', 'virtual', 'not_persisted');
";

const EXACT_ROWS: &[(&str, &str, &str)] = &[
    ("game_install_paths", "external_absolute", "preserved"),
    (PORTABLE_DATA_PATH_TAG, "portable_root_relative", "data"),
    ("ui_virtual_paths", "virtual", "not_persisted"),
];

pub(super) fn baseline_sql() -> &'static str {
    SQL
}

pub(in crate::schema) fn apply(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(SQL)
        .map_err(|error| storage_context("could not create portable path tags", error))
}

pub(in crate::schema) fn validate(connection: &Connection) -> AppResult<()> {
    let mut statement = connection
        .prepare("SELECT tag, kind, value FROM portable_path_tags ORDER BY tag")
        .map_err(|error| {
            storage_context("could not prepare portable path-tag validation", error)
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| storage_context("could not query portable path tags", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| storage_context("could not read portable path tags", error))?;
    let expected = EXACT_ROWS
        .iter()
        .map(|(tag, kind, value)| ((*tag).to_owned(), (*kind).to_owned(), (*value).to_owned()))
        .collect::<Vec<_>>();

    if rows == expected {
        Ok(())
    } else {
        Err(storage_context(
            "portable path tags do not match the canonical schema contract",
            "unexpected rows",
        ))
    }
}
