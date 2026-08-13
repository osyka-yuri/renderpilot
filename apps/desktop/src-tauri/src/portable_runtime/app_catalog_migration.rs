//! App-owned portable catalog classification and exact schema transition.
//!
//! The stable supervisor supplies authenticated permits and preserves recovery
//! authority. This signed App module alone links the exact storage migration
//! chain, including the narrow fresh-catalog classification rule.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use super::{
    app_protocol::{CatalogMigrationOperation, CatalogMigrationReport},
    error::{PortableRuntimeError, Result},
    rpu::MAXIMUM_SCHEMA,
    signature::sha256_file,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogClassification {
    Fresh,
    Existing { schema: u32 },
}

impl CatalogClassification {
    /// The public SQLite schema marker observed during the read-only trial.
    #[must_use]
    pub const fn schema_observed(self) -> u32 {
        match self {
            Self::Fresh => 0,
            Self::Existing { schema } => schema,
        }
    }
}

/// Classifies a catalog without mutation. A `user_version = 0` database is
/// fresh only when SQLite exposes no user table, index, view, or trigger.
pub fn classify_catalog(path: &Path) -> Result<CatalogClassification> {
    if !path.exists() {
        return Ok(CatalogClassification::Fresh);
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))?;
    connection
        .execute_batch("PRAGMA query_only = ON;")
        .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))?;
    let raw_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))?;
    let schema = u32::try_from(raw_version).map_err(|_| {
        PortableRuntimeError::new(
            "portable_schema_unsupported",
            "catalog schema version was negative",
        )
    })?;
    if schema != 0 {
        inspect_catalog_schema(path)?;
        return Ok(CatalogClassification::Existing { schema });
    }
    let user_objects: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type IN ('table', 'index', 'view', 'trigger') \
             AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))?;
    if user_objects != 0 {
        return Err(PortableRuntimeError::new(
            "portable_fresh_catalog",
            "user_version zero catalog contained user SQLite objects",
        ));
    }
    Ok(CatalogClassification::Fresh)
}

/// Executes the exact storage transition compiled into this signed App after
/// checking the supervisor's opaque permit shape and authenticated target.
pub fn execute_generation_migration(
    catalog: &Path,
    source_schema: u32,
    target_schema: u32,
    operation: &CatalogMigrationOperation,
) -> Result<CatalogMigrationReport> {
    if target_schema != MAXIMUM_SCHEMA {
        return Err(PortableRuntimeError::new(
            "portable_migration_contract",
            "migration target did not match the signed App generation",
        ));
    }
    let observed = inspect_catalog_schema(catalog)?;
    if observed != source_schema {
        return Err(PortableRuntimeError::new(
            "portable_migration_contract",
            "catalog schema changed after TrialReady",
        ));
    }
    let transition = match operation {
        CatalogMigrationOperation::ValidateCurrent(_) if source_schema == target_schema => {
            renderpilot_storage_sqlite::PortableCatalogSchemaTransition::ValidateCurrent
        }
        CatalogMigrationOperation::UpgradeAfterSnapshot(operation)
            if source_schema < target_schema && is_sha256(&operation.snapshot_receipt_sha256) =>
        {
            renderpilot_storage_sqlite::PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot
        }
        _ => {
            return Err(PortableRuntimeError::new(
                "portable_migration_contract",
                "migration operation did not match the observed schema transition",
            ));
        }
    };
    let report =
        renderpilot_storage_sqlite::transition_portable_catalog_schema(catalog, transition)
            .map_err(|error| storage_error(&error))?;
    if report.source_version != source_schema || report.target_version != target_schema {
        return Err(PortableRuntimeError::new(
            "portable_migration_validation",
            "storage migration did not report the permitted schema transition",
        ));
    }
    Ok(CatalogMigrationReport {
        source_version: report.source_version,
        target_version: report.target_version,
        catalog_sha256: sha256_file(catalog)?,
    })
}

fn inspect_catalog_schema(path: &Path) -> Result<u32> {
    renderpilot_storage_sqlite::inspect_portable_catalog_schema(path)
        .map_err(|error| storage_error(&error))
}

fn storage_error(
    error: &renderpilot_storage_sqlite::PortableCatalogSchemaError,
) -> PortableRuntimeError {
    PortableRuntimeError::new("portable_storage_schema", error.to_string())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
