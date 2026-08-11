use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    signature::sha256_file,
    snapshot::SnapshotReceipt,
};

pub const PORTABLE_SCHEMA_VERSION: u32 = 16;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MigrationReceipt {
    pub source_version: u32,
    pub target_version: u32,
    pub catalog_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_sha256: Option<String>,
    /// The portable data root is represented relative to the moved root; game
    /// installation values remain external and UI-only values remain virtual.
    pub portable_data_path_tag: String,
    pub external_paths_preserved: bool,
    pub virtual_paths_omitted: bool,
}

/// The supervisor owns the bounded released-v1.x→current migration. It delegates every
/// schema inspection, DDL, checkpoint, and validation operation to the
/// storage crate's approved crate-root API; portable orchestration never
/// performs private SQL migrations.
pub fn migrate_to_current(
    catalog: &Path,
    snapshot: &SnapshotReceipt,
    receipt_path: &Path,
) -> Result<MigrationReceipt> {
    let source = read_schema_version(catalog)?;
    if source == PORTABLE_SCHEMA_VERSION {
        return validate_current_schema_with_backup(catalog, Some(&snapshot.backup_sha256));
    }
    if sha256_file(&snapshot.backup_path)? != snapshot.backup_sha256 {
        return Err(PortableRuntimeError::new(
            "portable_migration_backup",
            "backup changed before migration",
        ));
    }
    let object_id = format!("migration:{}", snapshot.transaction_id);
    let migration_intent =
        format!("upgrade-v{source}-to-v{PORTABLE_SCHEMA_VERSION}-after-snapshot");
    provenance::intent(
        SealDomain::Migration,
        &object_id,
        migration_intent.as_bytes(),
    )?;
    let report = renderpilot_storage_sqlite::transition_portable_catalog_schema(
        catalog,
        renderpilot_storage_sqlite::PortableCatalogSchemaTransition::UpgradeToCurrentAfterSnapshot,
    )
    .map_err(|error| storage_error(&error))?;
    let receipt = receipt_from_report(catalog, report, Some(snapshot.backup_sha256.clone()))?;
    if receipt.target_version != PORTABLE_SCHEMA_VERSION {
        return Err(PortableRuntimeError::new(
            "portable_migration_validation",
            "storage migration did not report schema v16",
        ));
    }
    write_sealed_receipt(receipt_path, &object_id, &receipt)?;
    provenance::observe(
        SealDomain::Migration,
        &object_id,
        receipt.catalog_sha256.as_bytes(),
    )?;
    Ok(receipt)
}

pub fn read_schema_version(path: &Path) -> Result<u32> {
    renderpilot_storage_sqlite::inspect_portable_catalog_schema(path)
        .map_err(|error| storage_error(&error))
}

/// Validates an already-current catalog without allocating rollback state.
/// A schema-16 launch is observational and therefore has no backup receipt.
pub fn validate_current_schema(catalog: &Path) -> Result<MigrationReceipt> {
    validate_current_schema_with_backup(catalog, None)
}

fn validate_current_schema_with_backup(
    catalog: &Path,
    backup_sha256: Option<&str>,
) -> Result<MigrationReceipt> {
    let report = renderpilot_storage_sqlite::transition_portable_catalog_schema(
        catalog,
        renderpilot_storage_sqlite::PortableCatalogSchemaTransition::ValidateCurrent,
    )
    .map_err(|error| storage_error(&error))?;
    let receipt = receipt_from_report(catalog, report, backup_sha256.map(str::to_owned))?;
    if receipt.source_version != PORTABLE_SCHEMA_VERSION
        || receipt.target_version != PORTABLE_SCHEMA_VERSION
    {
        return Err(PortableRuntimeError::new(
            "portable_schema_unsupported",
            "storage validation did not report exact portable schema v16",
        ));
    }
    Ok(receipt)
}

fn receipt_from_report(
    catalog: &Path,
    report: renderpilot_storage_sqlite::PortableCatalogSchemaReport,
    backup_sha256: Option<String>,
) -> Result<MigrationReceipt> {
    Ok(MigrationReceipt {
        source_version: report.source_version,
        target_version: report.target_version,
        catalog_sha256: sha256_file(catalog)?,
        backup_sha256,
        portable_data_path_tag: report.portable_data_path_tag,
        external_paths_preserved: report.external_paths_preserved,
        virtual_paths_omitted: report.virtual_paths_omitted,
    })
}

fn write_sealed_receipt(
    receipt_path: &Path,
    object_id: &str,
    receipt: &MigrationReceipt,
) -> Result<()> {
    let plaintext = serde_json::to_vec(receipt).map_err(|error| {
        PortableRuntimeError::new("portable_migration_receipt", error.to_string())
    })?;
    let bytes = provenance::seal(SealDomain::Migration, object_id, &plaintext)?;
    let parent = receipt_path.parent().ok_or_else(|| {
        PortableRuntimeError::new("portable_migration_receipt", "receipt had no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(receipt_path)?;
    use std::io::Write as _;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn storage_error(
    error: &renderpilot_storage_sqlite::PortableCatalogSchemaError,
) -> PortableRuntimeError {
    let code = match error.kind() {
        renderpilot_storage_sqlite::PortableCatalogSchemaErrorKind::UnsupportedVersion => {
            "portable_schema_unsupported"
        }
        _ => "portable_schema_storage",
    };
    PortableRuntimeError::new(code, error.to_string())
}
