use std::fs;

use renderpilot_orchestration::portable::RuntimePathsV1;
use renderpilot_storage_sqlite::SqliteStorage;
use rusqlite::Connection;

use super::{error_code, supervisor_session, temp_root};
use crate::portable_runtime::{
    journal::journal_path,
    migration::{PORTABLE_SCHEMA_VERSION, migrate_to_current},
    snapshot,
    supervisor_activation::migrate_if_existing,
};

const RELEASED_V4_SCHEMA: &str = include_str!(
    "../../../../../../crates/renderpilot-storage-sqlite/tests/fixtures/catalog-v4.sql"
);

fn catalog_with_version(path: &std::path::Path, version: u32) {
    if version == 15 {
        let storage = SqliteStorage::open(path).expect("create exact current catalog fixture");
        drop(storage);
        Connection::open(path)
            .expect("open current catalog for v15 fixture")
            .execute_batch("DROP TABLE portable_path_tags; PRAGMA user_version = 15;")
            .expect("reduce current catalog to the exact v15 boundary");
        return;
    }
    let connection = Connection::open(path).expect("create SQLite fixture");
    connection
        .execute_batch(&format!(
            "PRAGMA user_version = {version}; CREATE TABLE legacy(id INTEGER);"
        ))
        .expect("write legacy schema fixture");
}

fn user_version(path: &std::path::Path) -> u32 {
    let connection = Connection::open(path).expect("open SQLite fixture");
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read schema version")
}

fn catalog_v4_with_user_data(path: &std::path::Path) {
    let connection = Connection::open(path).expect("create released v4 catalog");
    connection
        .execute_batch(RELEASED_V4_SCHEMA)
        .expect("apply released v4 schema");
    connection
        .execute_batch(
            "
            PRAGMA user_version = 4;
            INSERT INTO games (
                id,
                title,
                launcher,
                platform,
                runtime,
                install_path,
                executable_candidates_json
            ) VALUES (
                'preserved-game',
                'Preserved game',
                'manual',
                'windows',
                'native',
                'C:/Games/Preserved',
                '[]'
            );
            ",
        )
        .expect("stamp v4 and insert user data");
}

#[test]
fn runtime_paths_stay_under_the_stable_portable_root_after_unicode_move() {
    let root = temp_root("unicode-move");
    let portable_root = root.path().join("РендерПилот-移動");
    let generation = portable_root
        .join(".renderpilot-generations/v1/objects")
        .join("a".repeat(64));
    let app = generation.join("renderpilot-app.exe");
    let paths = RuntimePathsV1::from_portable_root(portable_root.clone(), &generation, &app)
        .expect("derive moved portable paths");
    paths
        .validate()
        .expect("all durable paths remain contained");
    assert_eq!(paths.data_root, portable_root.join("data"));
    assert!(paths.catalog_db_path.starts_with(&portable_root));
    assert!(paths.webview2_root.starts_with(&portable_root));
    assert_eq!(paths.selected_app_executable, app);

    let source = include_str!("../runtime_paths.rs");
    let install = source
        .find("install_runtime_paths(paths)")
        .expect("typed path install");
    let ambient = source
        .find("std::env::set_var")
        .expect("ambient replacement");
    assert!(
        install < ambient,
        "typed paths install before ambient compatibility projection"
    );
    assert!(source.contains("WEBVIEW2_USER_DATA_FOLDER"));
    assert!(source.contains("RENDERPILOT_DB_PATH"));
}

#[test]
fn schema_15_to_current_writes_exact_path_tags_backup_and_idempotent_receipt() {
    let root = temp_root("schema-15-16");
    let catalog = root.path().join("catalog.db");
    let update = root.path().join("update");
    catalog_with_version(&catalog, 15);
    let snapshot = snapshot::create(&catalog, &update, "migration").expect("create backup receipt");
    let receipt = root.path().join("migration/receipt.json");

    let first =
        migrate_to_current(&catalog, &snapshot, &receipt).expect("migrate supported schema");
    assert_eq!(first.source_version, 15);
    assert_eq!(first.target_version, PORTABLE_SCHEMA_VERSION);
    assert!(receipt.is_file());
    assert_eq!(user_version(&catalog), 16);
    let tags = Connection::open(&catalog)
        .expect("open migrated catalog")
        .prepare("SELECT tag, kind, value FROM portable_path_tags ORDER BY tag")
        .expect("read tags")
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .expect("query tags")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("collect tags");
    assert_eq!(
        tags,
        vec![
            (
                "game_install_paths".to_owned(),
                "external_absolute".to_owned(),
                "preserved".to_owned()
            ),
            (
                "portable-v1".to_owned(),
                "portable_root_relative".to_owned(),
                "data".to_owned()
            ),
            (
                "ui_virtual_paths".to_owned(),
                "virtual".to_owned(),
                "not_persisted".to_owned()
            ),
        ]
    );
    let second =
        migrate_to_current(&catalog, &snapshot, &receipt).expect("schema 16 replay is idempotent");
    assert_eq!(second.target_version, PORTABLE_SCHEMA_VERSION);
}

#[test]
fn released_v4_catalog_migrates_through_the_supervisor_boundary() {
    let root = temp_root("schema-4-current");
    let catalog = root.path().join("catalog.db");
    let update = root.path().join("update");
    catalog_v4_with_user_data(&catalog);
    let snapshot = snapshot::create(&catalog, &update, "migration").expect("create v4 backup");
    let receipt_path = root.path().join("migration/receipt.json");

    let receipt =
        migrate_to_current(&catalog, &snapshot, &receipt_path).expect("migrate released v4");

    assert_eq!(receipt.source_version, 4);
    assert_eq!(receipt.target_version, PORTABLE_SCHEMA_VERSION);
    assert_eq!(
        receipt.backup_sha256.as_deref(),
        Some(snapshot.backup_sha256.as_str())
    );
    assert!(receipt_path.is_file());
    assert_eq!(
        Connection::open(&catalog)
            .expect("open migrated catalog")
            .query_row(
                "SELECT title FROM games WHERE id = 'preserved-game'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("read preserved v4 game"),
        "Preserved game"
    );
}

#[test]
fn current_schema_launch_does_not_allocate_a_rollback_snapshot() {
    let root = temp_root("schema-16-no-snapshot");
    let portable_root = root.path().join("portable");
    let generation = portable_root.join("generation");
    let app = generation.join("renderpilot-app.exe");
    let paths = RuntimePathsV1::from_portable_root(portable_root, &generation, &app)
        .expect("derive portable paths");
    fs::create_dir_all(&paths.data_root).expect("create data root");
    catalog_with_version(&paths.catalog_db_path, 15);
    let initial = snapshot::create(&paths.catalog_db_path, &paths.update_root, "initial")
        .expect("create migration backup");
    migrate_to_current(
        &paths.catalog_db_path,
        &initial,
        &paths
            .update_root
            .join("transactions/initial/migration.json"),
    )
    .expect("migrate fixture to current schema");

    migrate_if_existing(
        &journal_path(&paths.update_root, "current"),
        &paths,
        "current",
        &"a".repeat(64),
        None,
        &supervisor_session('1'),
    )
    .expect("validate current schema without migration");

    assert!(
        !paths
            .update_root
            .join("transactions/current/snapshot")
            .exists(),
        "ordinary schema-16 launches must not accumulate full catalog copies"
    );
}

#[test]
fn snapshot_includes_committed_wal_rows_and_restores_a_standalone_database() {
    let root = temp_root("snapshot-wal");
    let catalog = root.path().join("catalog.db");
    let update = root.path().join("update");
    let connection = Connection::open(&catalog).expect("create WAL catalog");
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE fixture(value TEXT NOT NULL);
             PRAGMA wal_checkpoint(TRUNCATE);
             INSERT INTO fixture(value) VALUES ('committed-in-wal');",
        )
        .expect("write committed WAL fixture");
    assert!(
        std::path::PathBuf::from(format!("{}-wal", catalog.display())).is_file(),
        "fixture must exercise a live WAL"
    );

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
            "PRAGMA journal_mode = WAL;
             PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE fixture(value TEXT NOT NULL);
             INSERT INTO fixture(value) VALUES ('snapshot');
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
    assert!(
        crash_wal.len() > 32,
        "fixture must contain committed WAL frames"
    );
    drop(connection);

    // Recreate the durable state left by a process crash: the old main file
    // plus committed migration frames that never reached the final checkpoint.
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
fn unsupported_and_malformed_schemas_fail_closed_without_inference() {
    let root = temp_root("schema-closed");
    for (name, version, expected) in [
        ("pre-v1", 3, "portable_schema_unsupported"),
        ("malformed-supported", 14, "portable_schema_storage"),
        ("future", 17, "portable_schema_unsupported"),
    ] {
        let catalog = root.path().join(format!("{name}.db"));
        catalog_with_version(&catalog, version);
        let snapshot = snapshot::create(&catalog, &root.path().join(name), "migration")
            .expect("create backup for unsupported schema");
        assert_eq!(
            error_code(migrate_to_current(
                &catalog,
                &snapshot,
                &root.path().join(format!("{name}.json"))
            )),
            expected
        );
        assert_eq!(user_version(&catalog), version);
    }

    let catalog = root.path().join("backup-mismatch.db");
    catalog_with_version(&catalog, 15);
    let snapshot = snapshot::create(&catalog, &root.path().join("backup"), "migration")
        .expect("create immutable backup receipt");
    fs::write(&snapshot.backup_path, b"tampered backup").expect("tamper isolated fixture");
    assert_eq!(
        error_code(migrate_to_current(
            &catalog,
            &snapshot,
            &root.path().join("bad-receipt.json")
        )),
        "portable_migration_backup"
    );
    assert_eq!(user_version(&catalog), 15);
    assert!(
        !Connection::open(&catalog)
            .expect("open unchanged catalog")
            .prepare("SELECT tag FROM portable_path_tags")
            .is_ok(),
        "unsupported legacy inference must not create tagged authority"
    );
}
