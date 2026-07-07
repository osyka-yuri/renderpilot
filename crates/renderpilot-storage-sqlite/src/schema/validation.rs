//! Completeness checks for the catalog schema against the required objects.

use renderpilot_application::AppResult;
use rusqlite::{Connection, Error as SqliteError};

use crate::error::storage_context;

use super::REQUIRED_SCHEMA_OBJECT_GROUPS;
use super::objects::{SchemaObjectKind, object_exists};
use super::physical;

pub(super) fn catalog_schema_is_valid(connection: &Connection) -> AppResult<bool> {
    for &(object_kind, object_names) in REQUIRED_SCHEMA_OBJECT_GROUPS {
        if !required_objects_exist(connection, object_kind, object_names)? {
            return Ok(false);
        }
    }

    if !physical_column_mismatches(connection)?.is_empty() {
        return Ok(false);
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
    let mut violations = Vec::new();

    for &(object_kind, object_names) in REQUIRED_SCHEMA_OBJECT_GROUPS {
        collect_missing_object_violations(connection, object_kind, object_names, &mut violations)?;
    }

    violations.extend(physical_column_mismatches(connection)?);

    if violations.is_empty() {
        return Ok(());
    }

    Err(storage_context(
        &format!(
            "sqlite catalog schema validation failed: {}",
            violations.join(", ")
        ),
        SqliteError::InvalidQuery,
    ))
}

/// Exact physical-column contract: each contracted table must have precisely the
/// expected column set (no missing names, no unexpected extras).
pub(super) fn physical_column_mismatches(connection: &Connection) -> AppResult<Vec<String>> {
    let mut mismatches = Vec::new();
    for &(table_name, expected_columns) in physical::CONTRACT_TABLES {
        let live = super::pragma_column_names(connection, table_name)?;

        for column in expected_columns {
            if !live.contains(*column) {
                mismatches.push(format!("column {table_name}.{column}"));
            }
        }
        for column in &live {
            if !expected_columns.contains(&column.as_str()) {
                mismatches.push(format!("unexpected column {table_name}.{column}"));
            }
        }
    }
    Ok(mismatches)
}

fn collect_missing_object_violations(
    connection: &Connection,
    object_kind: SchemaObjectKind,
    object_names: &[&str],
    violations: &mut Vec<String>,
) -> AppResult<()> {
    for &object_name in object_names {
        if !object_exists(connection, object_kind, object_name)? {
            violations.push(format!("{} {object_name}", object_kind.sqlite_type()));
        }
    }

    Ok(())
}
