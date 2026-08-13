use std::fs;

use rusqlite::Connection;

use super::{error_code, temp_root};
use crate::portable_runtime::snapshot;

#[test]
fn snapshot_includes_committed_wal_rows_and_restores_a_standalone_database() {
    let root = temp_root("snapshot-wal");
    let catalog = root.path().join("catalog.db");
    let update = root.path().join("update");
    let connection = Connection::open(&catalog).expect("create WAL catalog");
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE fixture(value TEXT NOT NULL); PRAGMA wal_checkpoint(TRUNCATE);
             INSERT INTO fixture(value) VALUES ('committed-in-wal');",
        )
        .expect("write committed WAL fixture");
    assert!(std::path::PathBuf::from(format!("{}-wal", catalog.display())).is_file());
    let receipt = snapshot::create(&catalog, &update, "wal").expect("snapshot WAL catalog");
    drop(connection);
    Connection::open(&catalog)
        .expect("open live catalog")
        .execute("UPDATE fixture SET value = 'after-snapshot'", [])
        .expect("mutate live catalog");
    snapshot::restore(&receipt, &catalog).expect("restore online backup");
    let restored: String = Connection::open(&catalog)
        .expect("open restored catalog")
        .query_row("SELECT value FROM fixture", [], |row| row.get(0))
        .expect("read restored WAL value");
    assert_eq!(restored, "committed-in-wal");
}

#[test]
fn snapshot_restore_neutralizes_crash_surviving_wal_before_main_replacement() {
    let root = temp_root("snapshot-stale-wal");
    let catalog = root.path().join("catalog.db");
    let update = root.path().join("update");
    let wal = std::path::PathBuf::from({
        let mut value = catalog.as_os_str().to_owned();
        value.push("-wal");
        value
    });
    let connection = Connection::open(&catalog).expect("create WAL rollback fixture");
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE fixture(value TEXT NOT NULL); INSERT INTO fixture(value) VALUES ('snapshot');
             PRAGMA wal_checkpoint(TRUNCATE);",
        )
        .expect("create checkpointed snapshot value");
    let receipt =
        snapshot::create(&catalog, &update, "stale-wal").expect("snapshot checkpointed value");
    let main_before_failed_migration = fs::read(&catalog).expect("capture pre-WAL main bytes");
    connection
        .execute("UPDATE fixture SET value = 'failed-migration'", [])
        .expect("commit failed-migration WAL frame");
    let crash_wal = fs::read(&wal).expect("capture live WAL before simulated crash");
    assert!(crash_wal.len() > 32);
    drop(connection);
    fs::write(&catalog, main_before_failed_migration).expect("restore crash-time main file");
    fs::write(&wal, crash_wal).expect("restore crash-surviving WAL");
    snapshot::restore(&receipt, &catalog).expect("restore after neutralizing stale WAL");
    let restored: String = Connection::open(&catalog)
        .expect("open rolled-back catalog")
        .query_row("SELECT value FROM fixture", [], |row| row.get(0))
        .expect("read rolled-back value");
    assert_eq!(restored, "snapshot");
}

#[test]
fn snapshot_fixture_tampering_is_rejected_without_altering_its_catalog() {
    let root = temp_root("snapshot-tamper");
    let catalog = root.path().join("catalog.db");
    super::compatibility_support::catalog_with_version(&catalog, 15);
    let receipt = snapshot::create(&catalog, &root.path().join("backup"), "migration")
        .expect("create immutable backup receipt");
    fs::write(&receipt.backup_path, b"tampered backup").expect("tamper isolated fixture");
    assert_eq!(
        error_code(snapshot::verify_unchanged(&receipt)),
        "portable_migration_backup"
    );
    assert_eq!(super::compatibility_support::user_version(&catalog), 15);
}
