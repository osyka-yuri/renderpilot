//! Stable-supervisor migration authority.
//!
//! This module deliberately contains no App schema transition or exact catalog
//! inspection. The signed App owns those release-specific operations; the
//! stable supervisor owns only observation, snapshots, journal intent, and
//! sealed receipts that bind its own rollback authority to the App report.

use std::{io::Write, path::Path};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use super::{
    app_protocol::CatalogMigrationReport,
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    signature::sha256_file,
    snapshot::SnapshotReceipt,
};

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct SupervisorMigrationReceipt<'a> {
    protocol: u16,
    snapshot_receipt_sha256: &'a str,
    snapshot_catalog_sha256: &'a str,
    snapshot_backup_sha256: &'a str,
    app_report: &'a CatalogMigrationReport,
}

/// Reads only SQLite's public schema marker. This generic observation lets a
/// stable supervisor admit later App generations without owning their schema
/// chain.
pub fn read_catalog_schema(path: &Path) -> Result<u32> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))?;
    connection
        .execute_batch("PRAGMA query_only = ON;")
        .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))?;
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))?;
    u32::try_from(version).map_err(|_| {
        PortableRuntimeError::new(
            "portable_schema_unsupported",
            "catalog schema version was negative",
        )
    })
}

pub fn begin_supervised_migration(
    transaction_id: &str,
    source_schema: u32,
    target_schema: u32,
) -> Result<()> {
    provenance::intent(
        SealDomain::Migration,
        &migration_object_id(transaction_id),
        format!("upgrade-v{source_schema}-to-v{target_schema}-after-snapshot").as_bytes(),
    )
}

/// Verifies the App's opaque report against generic SQLite and digest facts.
/// Exact table/trigger semantics remain in the App-generation boundary.
pub fn verify_generation_report(
    catalog: &Path,
    source_schema: u32,
    target_schema: u32,
    report: &CatalogMigrationReport,
) -> Result<()> {
    if report.source_version != source_schema
        || report.target_version != target_schema
        || !is_sha256(&report.catalog_sha256)
        || read_catalog_schema(catalog)? != target_schema
        || sha256_file(catalog)? != report.catalog_sha256
    {
        return Err(PortableRuntimeError::new(
            "portable_migration_validation",
            "App migration report did not match the supervised catalog transition",
        ));
    }
    validate_database_integrity(catalog)
}

/// Seals a supervisor-owned receipt that records both rollback digests and the
/// independently verified App report. The App never receives these digests.
pub fn commit_supervised_migration(
    receipt_path: &Path,
    transaction_id: &str,
    snapshot: &SnapshotReceipt,
    report: &CatalogMigrationReport,
) -> Result<()> {
    let object_id = migration_object_id(transaction_id);
    let receipt = SupervisorMigrationReceipt {
        protocol: 1,
        snapshot_receipt_sha256: &snapshot.receipt_sha256,
        snapshot_catalog_sha256: &snapshot.catalog_sha256,
        snapshot_backup_sha256: &snapshot.backup_sha256,
        app_report: report,
    };
    write_sealed_receipt(receipt_path, &object_id, &receipt)?;
    provenance::observe(
        SealDomain::Migration,
        &object_id,
        report.catalog_sha256.as_bytes(),
    )
}

fn validate_database_integrity(path: &Path) -> Result<()> {
    let connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|error| {
            PortableRuntimeError::new("portable_migration_validation", error.to_string())
        })?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| {
            PortableRuntimeError::new("portable_migration_validation", error.to_string())
        })?;
    if integrity != "ok" {
        return Err(PortableRuntimeError::new(
            "portable_migration_validation",
            format!("migrated catalog failed integrity_check: {integrity}"),
        ));
    }
    Ok(())
}

fn migration_object_id(transaction_id: &str) -> String {
    format!("migration:{transaction_id}")
}

fn write_sealed_receipt(
    receipt_path: &Path,
    object_id: &str,
    receipt: &impl Serialize,
) -> Result<()> {
    let plaintext = serde_json::to_vec(receipt).map_err(|error| {
        PortableRuntimeError::new("portable_migration_receipt", error.to_string())
    })?;
    let bytes = provenance::seal(SealDomain::Migration, object_id, &plaintext)?;
    let parent = receipt_path.parent().ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_migration_receipt",
            "migration receipt had no parent",
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(receipt_path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
