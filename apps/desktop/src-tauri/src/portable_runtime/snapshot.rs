#![expect(
    unsafe_code,
    reason = "the portable snapshot restore uses one atomic Windows replacement boundary"
)]

use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::{
    Foundation::GetLastError,
    Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
};

use super::{
    error::{PortableRuntimeError, Result},
    provenance::{self, SealDomain},
    signature::{sha256_file, sha256_hex},
    win32::process::path_wide_nul,
};

const SNAPSHOT_PROTOCOL: u16 = 1;

#[derive(Clone, Debug)]
pub struct SnapshotReceipt {
    pub backup_path: PathBuf,
    pub transaction_id: String,
    pub backup_sha256: String,
    pub catalog_sha256: String,
    pub receipt_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PersistedSnapshotReceiptV1 {
    protocol: u16,
    transaction_id: String,
    backup_sha256: String,
    catalog_sha256: String,
    receipt_sha256: String,
}

/// Creates an immutable backup and a separately durable, self-hashed receipt.
/// Recovery never treats the backup as authoritative until the journal binds
/// this receipt through `SnapshotCommitted`.
pub fn create(catalog: &Path, update_root: &Path, transaction_id: &str) -> Result<SnapshotReceipt> {
    if !catalog.is_file() {
        return Err(PortableRuntimeError::new(
            "portable_snapshot",
            "catalog database was absent before migration",
        ));
    }
    let transaction_root = update_root.join("transactions").join(transaction_id);
    let snapshot_root = transaction_root.join("snapshot");
    std::fs::create_dir_all(&snapshot_root)?;
    let backup = snapshot_root.join("catalog.db");
    let receipt_path = transaction_root.join("snapshot-receipt.json");

    if backup.exists() || receipt_path.exists() {
        return load_from_paths(&transaction_root, transaction_id);
    }
    let object_id = format!("snapshot:{transaction_id}");
    provenance::intent(SealDomain::Snapshot, &object_id, b"create-sqlite-backup")?;

    let source =
        rusqlite::Connection::open_with_flags(catalog, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| PortableRuntimeError::new("portable_snapshot", error.to_string()))?;
    source
        .backup(rusqlite::MAIN_DB, &backup, None)
        .map_err(|error| PortableRuntimeError::new("portable_snapshot", error.to_string()))?;
    drop(source);
    validate_database(&backup)?;
    OpenOptions::new().write(true).open(&backup)?.sync_all()?;
    let backup_hash = sha256_file(&backup)?;

    let mut persisted = PersistedSnapshotReceiptV1 {
        protocol: SNAPSHOT_PROTOCOL,
        transaction_id: transaction_id.to_owned(),
        backup_sha256: backup_hash.clone(),
        // The online backup is the complete logical catalog, including every
        // committed WAL frame. Restore publishes these exact standalone bytes.
        catalog_sha256: backup_hash,
        receipt_sha256: String::new(),
    };
    persisted.receipt_sha256 = receipt_digest(&persisted)?;
    let receipt_plaintext = serde_json::to_vec(&persisted).map_err(|error| {
        PortableRuntimeError::new("portable_snapshot_receipt", error.to_string())
    })?;
    let receipt_bytes = provenance::seal(SealDomain::Snapshot, &object_id, &receipt_plaintext)?;
    let mut receipt_file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&receipt_path)?;
    receipt_file.write_all(&receipt_bytes)?;
    receipt_file.sync_all()?;
    drop(receipt_file);
    provenance::observe(SealDomain::Snapshot, &object_id, &receipt_bytes)?;
    load_from_paths(&transaction_root, transaction_id)
}

/// Loads and verifies the receipt and immutable backup that were already
/// committed by the journal. A surviving partial backup cannot authenticate
/// itself by being hashed during recovery.
pub fn load_committed(
    transaction_root: &Path,
    transaction_id: &str,
    expected_journal_transcript_sha256: &str,
) -> Result<SnapshotReceipt> {
    let receipt = load_from_paths(transaction_root, transaction_id)?;
    if sha256_hex(receipt.receipt_sha256.as_bytes()) != expected_journal_transcript_sha256 {
        return Err(PortableRuntimeError::new(
            "portable_snapshot_receipt",
            "SnapshotCommitted did not bind the immutable snapshot receipt",
        ));
    }
    Ok(receipt)
}

pub fn restore(receipt: &SnapshotReceipt, catalog: &Path) -> Result<()> {
    if sha256_file(&receipt.backup_path)? != receipt.backup_sha256 {
        return Err(PortableRuntimeError::new(
            "portable_snapshot",
            "backup receipt did not match backup bytes",
        ));
    }
    let object_id = format!("snapshot:{}", receipt.transaction_id);
    provenance::intent(SealDomain::Snapshot, &object_id, b"restore-sqlite-backup")?;
    let parent = catalog.parent().ok_or_else(|| {
        PortableRuntimeError::new("portable_snapshot", "catalog had no parent directory")
    })?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".renderpilot-restore-{}.tmp",
        receipt.transaction_id
    ));
    if temp.exists() {
        return Err(PortableRuntimeError::new(
            "portable_snapshot_restore",
            "existing restore nonce file was retained; no raw-path cleanup is authorized",
        ));
    }
    let bytes = std::fs::read(&receipt.backup_path)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    if sha256_file(&temp)? != receipt.catalog_sha256 {
        return Err(PortableRuntimeError::new(
            "portable_snapshot",
            "prepared restore did not match the original catalog",
        ));
    }
    // A crash after SQLite commit but before its final checkpoint may leave a
    // valid WAL whose frames belong to the failed migration. Checkpoint and
    // truncate it through SQLite before replacing the main database, so those
    // frames cannot replay over the restored snapshot. SHM is retained; no raw
    // sidecar path becomes deletion authority.
    neutralize_wal_before_restore(catalog)?;
    let from = path_wide_nul(&temp);
    let to = path_wide_nul(catalog);
    // SAFETY: both NUL-terminated paths remain live for the call. The source is
    // a synced app-owned temp file in the destination directory.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        let error = unsafe { GetLastError() };
        return Err(PortableRuntimeError::new(
            "portable_snapshot_restore",
            format!("atomic catalog restore failed: {error}"),
        ));
    }
    if sha256_file(catalog)? != receipt.catalog_sha256 {
        return Err(PortableRuntimeError::new(
            "portable_snapshot_restore",
            "restored catalog did not match the committed snapshot receipt",
        ));
    }
    provenance::observe(
        SealDomain::Snapshot,
        &object_id,
        receipt.catalog_sha256.as_bytes(),
    )?;
    Ok(())
}

fn neutralize_wal_before_restore(catalog: &Path) -> Result<()> {
    let connection = rusqlite::Connection::open_with_flags(
        catalog,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| PortableRuntimeError::new("portable_snapshot_restore", error.to_string()))?;
    let (busy, _, _): (i64, i64, i64) = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|error| {
            PortableRuntimeError::new("portable_snapshot_restore", error.to_string())
        })?;
    if busy != 0 {
        return Err(PortableRuntimeError::new(
            "portable_snapshot_restore",
            "SQLite WAL remained busy during rollback",
        ));
    }
    drop(connection);

    let wal = sqlite_sidecar(catalog, "-wal");
    if std::fs::metadata(&wal).is_ok_and(|metadata| metadata.len() != 0) {
        return Err(PortableRuntimeError::new(
            "portable_snapshot_restore",
            "SQLite WAL retained migration frames after rollback checkpoint",
        ));
    }
    Ok(())
}

fn sqlite_sidecar(catalog: &Path, suffix: &str) -> PathBuf {
    let mut path = catalog.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

fn validate_database(path: &Path) -> Result<()> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| PortableRuntimeError::new("portable_snapshot", error.to_string()))?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| PortableRuntimeError::new("portable_snapshot", error.to_string()))?;
    if integrity != "ok" {
        return Err(PortableRuntimeError::new(
            "portable_snapshot",
            "SQLite backup failed integrity_check",
        ));
    }
    Ok(())
}

fn load_from_paths(transaction_root: &Path, transaction_id: &str) -> Result<SnapshotReceipt> {
    let backup = transaction_root.join("snapshot").join("catalog.db");
    let receipt_path = transaction_root.join("snapshot-receipt.json");
    if !backup.is_file() || !receipt_path.is_file() {
        return Err(PortableRuntimeError::new(
            "portable_snapshot_receipt",
            "snapshot backup and receipt were not both durable",
        ));
    }
    let object_id = format!("snapshot:{transaction_id}");
    let persisted: PersistedSnapshotReceiptV1 = serde_json::from_slice(&provenance::open(
        SealDomain::Snapshot,
        &object_id,
        &std::fs::read(&receipt_path)?,
    )?)
    .map_err(|error| PortableRuntimeError::new("portable_snapshot_receipt", error.to_string()))?;
    let expected_receipt_sha256 = receipt_digest(&persisted)?;
    if persisted.protocol != SNAPSHOT_PROTOCOL
        || persisted.transaction_id != transaction_id
        || persisted.receipt_sha256 != expected_receipt_sha256
        || sha256_file(&backup)? != persisted.backup_sha256
        || persisted.backup_sha256 != persisted.catalog_sha256
    {
        return Err(PortableRuntimeError::new(
            "portable_snapshot_receipt",
            "snapshot receipt or immutable backup was invalid",
        ));
    }
    Ok(SnapshotReceipt {
        backup_path: backup,
        transaction_id: persisted.transaction_id,
        backup_sha256: persisted.backup_sha256,
        catalog_sha256: persisted.catalog_sha256,
        receipt_sha256: persisted.receipt_sha256,
    })
}

fn receipt_digest(receipt: &PersistedSnapshotReceiptV1) -> Result<String> {
    let mut unsigned = receipt.clone();
    unsigned.receipt_sha256.clear();
    serde_json::to_vec(&unsigned)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| PortableRuntimeError::new("portable_snapshot_receipt", error.to_string()))
}
