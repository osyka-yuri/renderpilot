//! Completeness checks for the catalog schema against the required contract.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_error;

use super::contract::{CONTRACT_TABLES, REQUIRED_INDEXES, REQUIRED_TABLES, REQUIRED_TRIGGERS};
use super::ddl::pending_file_mutations;
use super::ddl::portable_path_tags;
use super::objects::{SchemaObjectKind, object_exists};
use super::physical;

pub(super) fn catalog_schema_is_valid(connection: &Connection) -> AppResult<bool> {
    if !validate_violations(connection, true, ConstraintValidation::Probe)?.is_empty() {
        return Ok(false);
    }

    portable_path_tags::validate(connection).map(|()| true)
}

pub(super) fn catalog_schema_is_valid_observational(connection: &Connection) -> AppResult<bool> {
    if !validate_violations(connection, true, ConstraintValidation::Observational)?.is_empty() {
        return Ok(false);
    }

    portable_path_tags::validate(connection).map(|()| true)
}

pub(super) fn validate_catalog_schema(connection: &Connection) -> AppResult<()> {
    validate_catalog_schema_with_portable_path_tags(connection, true, ConstraintValidation::Probe)
}

pub(in crate::schema) fn validate_catalog_schema_observational(
    connection: &Connection,
) -> AppResult<()> {
    validate_catalog_schema_with_portable_path_tags(
        connection,
        true,
        ConstraintValidation::Observational,
    )
}

/// Exactly validates the released v15/v16 catalog shape without mutating it.
///
/// Those versions predate owner-scoped observations and still contain the weak
/// file cache and source checkpoint tables. V16 additionally requires the
/// portable path-tag contract introduced by its own released migration.
pub(in crate::schema) fn validate_legacy_portable_catalog_observational(
    connection: &Connection,
    require_portable_path_tags: bool,
) -> AppResult<()> {
    const OBSERVATION_TABLES: &[&str] = &["catalog_scan_authority", "file_observations"];
    const OBSERVATION_INDEXES: &[&str] = &[
        "idx_catalog_scan_authority_readiness",
        "idx_file_observations_game_path",
        "idx_file_observations_artifact_path",
    ];
    const OBSERVATION_TRIGGERS: &[&str] = &["trg_games_create_scan_authority"];
    const LEGACY_TABLES: &[&str] = &["file_hash_cache", "scan_source_checkpoints"];
    const LEGACY_INDEXES: &[&str] = &["idx_file_hash_cache_updated_at"];
    const LEGACY_TRIGGERS: &[&str] = &[
        "trg_file_hash_cache_touch_updated_at",
        "trg_scan_source_checkpoints_touch_updated_at",
    ];

    let mut violations = Vec::new();
    for &table in REQUIRED_TABLES {
        if OBSERVATION_TABLES.contains(&table)
            || (!require_portable_path_tags && table == portable_path_tags::TABLE_NAME)
        {
            continue;
        }
        collect_missing_object_violations(
            connection,
            SchemaObjectKind::Table,
            &[table],
            true,
            &mut violations,
        )?;
    }
    collect_missing_object_violations(
        connection,
        SchemaObjectKind::Table,
        LEGACY_TABLES,
        true,
        &mut violations,
    )?;

    for &index in REQUIRED_INDEXES {
        if OBSERVATION_INDEXES.contains(&index) {
            continue;
        }
        collect_missing_object_violations(
            connection,
            SchemaObjectKind::Index,
            &[index],
            true,
            &mut violations,
        )?;
    }
    collect_missing_object_violations(
        connection,
        SchemaObjectKind::Index,
        LEGACY_INDEXES,
        true,
        &mut violations,
    )?;

    for &trigger in REQUIRED_TRIGGERS {
        if OBSERVATION_TRIGGERS.contains(&trigger) {
            continue;
        }
        collect_missing_object_violations(
            connection,
            SchemaObjectKind::Trigger,
            &[trigger],
            true,
            &mut violations,
        )?;
    }
    collect_missing_object_violations(
        connection,
        SchemaObjectKind::Trigger,
        LEGACY_TRIGGERS,
        true,
        &mut violations,
    )?;

    for &(table, columns) in CONTRACT_TABLES {
        if OBSERVATION_TABLES.contains(&table)
            || (!require_portable_path_tags && table == portable_path_tags::TABLE_NAME)
        {
            continue;
        }
        collect_physical_column_mismatches(connection, table, columns, &mut violations)?;
    }
    collect_physical_column_mismatches(
        connection,
        "file_hash_cache",
        physical::legacy_file_hash_cache::ALL,
        &mut violations,
    )?;
    collect_physical_column_mismatches(
        connection,
        "scan_source_checkpoints",
        physical::legacy_scan_source_checkpoints::ALL,
        &mut violations,
    )?;
    violations.extend(constraint_mismatches(
        connection,
        ConstraintValidation::Observational,
    )?);

    if !violations.is_empty() {
        return Err(storage_error(format!(
            "sqlite legacy portable catalog validation failed: {}",
            violations.join(", ")
        )));
    }
    if require_portable_path_tags {
        portable_path_tags::validate(connection)?;
    }
    Ok(())
}

fn validate_catalog_schema_with_portable_path_tags(
    connection: &Connection,
    require_portable_path_tags: bool,
    constraint_validation: ConstraintValidation,
) -> AppResult<()> {
    let violations = validate_violations(
        connection,
        require_portable_path_tags,
        constraint_validation,
    )?;
    if violations.is_empty() {
        return if require_portable_path_tags {
            portable_path_tags::validate(connection)
        } else {
            Ok(())
        };
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

fn validate_violations(
    connection: &Connection,
    require_portable_path_tags: bool,
    constraint_validation: ConstraintValidation,
) -> AppResult<Vec<String>> {
    let mut violations = Vec::new();

    collect_missing_object_violations(
        connection,
        SchemaObjectKind::Table,
        REQUIRED_TABLES,
        require_portable_path_tags,
        &mut violations,
    )?;
    collect_missing_object_violations(
        connection,
        SchemaObjectKind::Index,
        REQUIRED_INDEXES,
        true,
        &mut violations,
    )?;
    collect_missing_object_violations(
        connection,
        SchemaObjectKind::Trigger,
        REQUIRED_TRIGGERS,
        true,
        &mut violations,
    )?;

    violations.extend(physical_column_mismatches_for(
        connection,
        require_portable_path_tags,
    )?);
    violations.extend(constraint_mismatches(connection, constraint_validation)?);

    Ok(violations)
}

/// Exact physical-column contract: each contracted table must have precisely the
/// expected column set (no missing names, no unexpected extras).
#[cfg(test)]
pub(super) fn physical_column_mismatches(connection: &Connection) -> AppResult<Vec<String>> {
    physical_column_mismatches_for(connection, true)
}

fn physical_column_mismatches_for(
    connection: &Connection,
    require_portable_path_tags: bool,
) -> AppResult<Vec<String>> {
    let mut mismatches = Vec::new();
    for &(table_name, expected_columns) in CONTRACT_TABLES {
        if !require_portable_path_tags && table_name == portable_path_tags::TABLE_NAME {
            continue;
        }
        collect_physical_column_mismatches(
            connection,
            table_name,
            expected_columns,
            &mut mismatches,
        )?;
    }
    Ok(mismatches)
}

fn collect_physical_column_mismatches(
    connection: &Connection,
    table_name: &str,
    expected_columns: &[&str],
    mismatches: &mut Vec<String>,
) -> AppResult<()> {
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
    Ok(())
}

#[derive(Clone, Copy)]
enum ConstraintValidation {
    Probe,
    Observational,
}

/// Semantic CHECK probes that column lists cannot express.
fn constraint_mismatches(
    connection: &Connection,
    constraint_validation: ConstraintValidation,
) -> AppResult<Vec<String>> {
    let mut mismatches = Vec::new();
    let allows_preparing = match constraint_validation {
        ConstraintValidation::Probe => pending_file_mutations::allows_preparing(connection)?,
        ConstraintValidation::Observational => {
            pending_file_mutations::allows_preparing_observational(connection)?
        }
    };
    if !allows_preparing {
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
    require_portable_path_tags: bool,
    violations: &mut Vec<String>,
) -> AppResult<()> {
    for &object_name in object_names {
        if !require_portable_path_tags && object_name == portable_path_tags::TABLE_NAME {
            continue;
        }
        if !object_exists(connection, object_kind, object_name)? {
            violations.push(format!("{} {object_name}", object_kind.sqlite_type()));
        }
    }

    Ok(())
}
