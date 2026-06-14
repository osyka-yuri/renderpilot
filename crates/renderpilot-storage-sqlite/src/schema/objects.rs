//! Inspection and dropping of catalog schema objects via `sqlite_master`.

use renderpilot_application::AppResult;
use rusqlite::{Connection, OptionalExtension};

use crate::error::storage_context;

#[derive(Clone, Copy, Debug)]
pub(super) enum SchemaObjectKind {
    Trigger,
    View,
    Index,
    Table,
}

impl SchemaObjectKind {
    pub(super) const DROP_ORDER: &'static [Self] =
        &[Self::Trigger, Self::View, Self::Index, Self::Table];

    pub(super) fn sqlite_type(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::View => "view",
            Self::Index => "index",
            Self::Table => "table",
        }
    }

    fn drop_keyword(self) -> &'static str {
        match self {
            Self::Trigger => "TRIGGER",
            Self::View => "VIEW",
            Self::Index => "INDEX",
            Self::Table => "TABLE",
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

pub(super) fn drop_user_schema_objects(connection: &Connection) -> AppResult<()> {
    for &object_kind in SchemaObjectKind::DROP_ORDER {
        drop_schema_objects_by_kind(connection, object_kind)?;
    }

    Ok(())
}

fn drop_schema_objects_by_kind(
    connection: &Connection,
    object_kind: SchemaObjectKind,
) -> AppResult<()> {
    let object_names = list_schema_object_names(connection, object_kind)?;

    for object_name in object_names {
        drop_schema_object(connection, object_kind, &object_name)?;
    }

    Ok(())
}

fn list_schema_object_names(
    connection: &Connection,
    object_kind: SchemaObjectKind,
) -> AppResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            "
            SELECT name
            FROM sqlite_master
            WHERE type = ?1
              AND name NOT LIKE 'sqlite_%'
            ORDER BY name
            ",
        )
        .map_err(|error| storage_context("could not enumerate sqlite schema objects", error))?;

    let rows = statement
        .query_map([object_kind.sqlite_type()], |row| row.get::<_, String>(0))
        .map_err(|error| storage_context("could not query sqlite schema objects", error))?;

    let mut object_names = Vec::new();

    for row in rows {
        object_names.push(
            row.map_err(|error| storage_context("could not parse sqlite object name", error))?,
        );
    }

    Ok(object_names)
}

fn drop_schema_object(
    connection: &Connection,
    object_kind: SchemaObjectKind,
    object_name: &str,
) -> AppResult<()> {
    let sql = format!(
        "DROP {} IF EXISTS {}",
        object_kind.drop_keyword(),
        quote_sql_identifier(object_name)
    );

    connection.execute_batch(&sql).map_err(|error| {
        storage_context(
            &format!(
                "could not drop sqlite catalog {} {object_name}",
                object_kind.sqlite_type()
            ),
            error,
        )
    })
}

fn quote_sql_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
