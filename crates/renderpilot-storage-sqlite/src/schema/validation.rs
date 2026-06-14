//! Completeness checks for the catalog schema against the required objects.

use renderpilot_application::AppResult;
use rusqlite::{Connection, Error as SqliteError};

use crate::error::storage_context;

use super::objects::{object_exists, SchemaObjectKind};
use super::REQUIRED_SCHEMA_OBJECT_GROUPS;

pub(super) fn catalog_schema_is_valid(connection: &Connection) -> AppResult<bool> {
    for &(object_kind, object_names) in REQUIRED_SCHEMA_OBJECT_GROUPS {
        if !required_objects_exist(connection, object_kind, object_names)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn required_objects_exist(
    connection: &Connection,
    object_kind: SchemaObjectKind,
    object_names: &[&str],
) -> AppResult<bool> {
    for &object_name in object_names {
        if !object_exists(connection, object_kind, object_name)? {
            return Ok(false);
        }
    }

    Ok(true)
}

pub(super) fn validate_catalog_schema(connection: &Connection) -> AppResult<()> {
    let mut missing_objects = Vec::new();

    for &(object_kind, object_names) in REQUIRED_SCHEMA_OBJECT_GROUPS {
        collect_missing_objects(connection, object_kind, object_names, &mut missing_objects)?;
    }

    if missing_objects.is_empty() {
        return Ok(());
    }

    Err(storage_context(
        &format!(
            "sqlite catalog schema is incomplete; missing required objects: {}",
            missing_objects.join(", ")
        ),
        SqliteError::InvalidQuery,
    ))
}

fn collect_missing_objects(
    connection: &Connection,
    object_kind: SchemaObjectKind,
    object_names: &[&str],
    missing_objects: &mut Vec<String>,
) -> AppResult<()> {
    for &object_name in object_names {
        if !object_exists(connection, object_kind, object_name)? {
            missing_objects.push(format!("{} {object_name}", object_kind.sqlite_type()));
        }
    }

    Ok(())
}
