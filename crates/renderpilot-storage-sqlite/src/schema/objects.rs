//! Inspection of catalog schema objects via `sqlite_master`.

use renderpilot_application::AppResult;
use rusqlite::{Connection, OptionalExtension};

use crate::error::storage_context;

#[derive(Clone, Copy, Debug)]
pub(super) enum SchemaObjectKind {
    Trigger,
    Index,
    Table,
}

impl SchemaObjectKind {
    pub(super) fn sqlite_type(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::Index => "index",
            Self::Table => "table",
        }
    }
}

pub(super) fn object_exists(
    connection: &Connection,
    object_kind: SchemaObjectKind,
    object_name: &str,
) -> AppResult<bool> {
    let exists = connection
        .query_row(
            "
            SELECT 1
            FROM sqlite_master
            WHERE type = ?1
              AND name = ?2
            LIMIT 1
            ",
            [object_kind.sqlite_type(), object_name],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| storage_context("could not inspect sqlite catalog object", error))?
        .is_some();

    Ok(exists)
}
