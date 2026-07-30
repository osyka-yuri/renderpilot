//! Completeness checks for the catalog schema against the required contract.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_error;

use super::contract::{CONTRACT_TABLES, REQUIRED_INDEXES, REQUIRED_TABLES, REQUIRED_TRIGGERS};
use super::ddl::pending_file_mutations;
use super::objects::{SchemaObjectKind, object_exists};

pub(super) fn catalog_schema_is_valid(connection: &Connection) -> AppResult<bool> {
    Ok(validate_violations(connection)?.is_empty())
}

pub(super) fn validate_catalog_schema(connection: &Connection) -> AppResult<()> {
    let violations = validate_violations(connection)?;
    if violations.is_empty() {
        return Ok(());
    }

    Err(storage_error(format!(
        "sqlite catalog schema validation failed: {}",
        violations.join(", ")
    )))
}

/// Fails when SQLite reports relational or on-disk corruption.
pub(super) fn validate_database_integrity(connection: &Connection) -> AppResult<()> {
    let foreign_key_violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|error| storage_error(format!("could not run foreign_key_check: {error}")))?;
    if foreign_key_violations != 0 {
        return Err(storage_error(format!(
            "sqlite foreign_key_check reported {foreign_key_violations} violation(s)"
        )));
    }

    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| storage_error(format!("could not run integrity_check: {error}")))?;
    if integrity != "ok" {
        return Err(storage_error(format!(
            "sqlite integrity_check failed: {integrity}"
        )));
    }
    Ok(())
}

fn validate_violations(connection: &Connection) -> AppResult<Vec<String>> {
    let mut violations = Vec::new();

    collect_missing_object_violations(
        connection,
        SchemaObjectKind::Table,
        REQUIRED_TABLES,
        &mut violations,
    )?;
    collect_missing_object_violations(
        connection,
        SchemaObjectKind::Index,
        REQUIRED_INDEXES,
        &mut violations,
    )?;
    collect_missing_object_violations(
        connection,
        SchemaObjectKind::Trigger,
        REQUIRED_TRIGGERS,
        &mut violations,
    )?;

    violations.extend(physical_column_mismatches(connection)?);
    violations.extend(constraint_mismatches(connection)?);

    Ok(violations)
}

/// Exact physical-column contract: each contracted table must have precisely the
/// expected column set (no missing names, no unexpected extras).
pub(super) fn physical_column_mismatches(connection: &Connection) -> AppResult<Vec<String>> {
    let mut mismatches = Vec::new();
    for &(table_name, expected_columns) in CONTRACT_TABLES {
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

/// Semantic CHECK probes that column lists cannot express.
fn constraint_mismatches(connection: &Connection) -> AppResult<Vec<String>> {
    let mut mismatches = Vec::new();
    if !pending_file_mutations::allows_preparing(connection)? {
        mismatches.push(
            "pending_file_mutations.state must accept 'preparing' (CHECK constraint)".to_owned(),
        );
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
