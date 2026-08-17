use rusqlite::Connection;

use super::{error_code, hash, temp_root};
use crate::portable_runtime::{
    app_catalog_migration::{CatalogClassification, classify_catalog},
    app_protocol::{
        AppControlMessage, AppStatusMessage, CatalogMigrationOperation, CatalogMigrationReport,
    },
    rpu::MAXIMUM_SCHEMA as PORTABLE_SCHEMA_VERSION,
    signature::sha256_file,
};

use super::compatibility_support::{
    PreparedMigrationHandshake, ScriptedMigrationTrial, catalog_v4_with_user_data,
    catalog_with_version, create_current_catalog, create_data_root, migrate_through_protocol,
    portable_paths, user_version,
};

#[test]
fn migration_handshake_refuses_scripted_send_receive_and_wrong_ack_failures() {
    let root = temp_root("scripted-migration-handshake");
    let paths = portable_paths(root.path());
    create_data_root(&paths);
    create_current_catalog(&paths.catalog_db_path);

    let handshake = PreparedMigrationHandshake::new(&paths, &hash('9'), PORTABLE_SCHEMA_VERSION)
        .expect("prepare current-schema handshake");
    let report = CatalogMigrationReport {
        source_version: PORTABLE_SCHEMA_VERSION,
        target_version: PORTABLE_SCHEMA_VERSION,
        catalog_sha256: sha256_file(&paths.catalog_db_path).expect("hash current catalog"),
    };
    let mut wrong_ack = ScriptedMigrationTrial {
        fail_send: false,
        fail_receive: false,
        response: Some(AppStatusMessage::migration_ack(
            report,
            None,
            hash('8'),
            handshake.supervisor_session.transcript_sha256().to_owned(),
        )),
        sent: None,
    };
    assert_eq!(
        error_code(handshake.prepare_catalog(&paths, &mut wrong_ack, PORTABLE_SCHEMA_VERSION)),
        "portable_migration_contract"
    );
    assert!(matches!(
        wrong_ack.sent.as_ref(),
        Some(AppControlMessage::MigrationPermit(permit))
            if matches!(permit.operation, CatalogMigrationOperation::ValidateCurrent(_))
    ));

    let mut send_failure = ScriptedMigrationTrial {
        fail_send: true,
        fail_receive: false,
        response: None,
        sent: None,
    };
    assert_eq!(
        error_code(handshake.prepare_catalog(&paths, &mut send_failure, PORTABLE_SCHEMA_VERSION)),
        "portable_migration_test"
    );

    let mut receive_failure = ScriptedMigrationTrial {
        fail_send: false,
        fail_receive: true,
        response: None,
        sent: None,
    };
    assert_eq!(
        error_code(handshake.prepare_catalog(
            &paths,
            &mut receive_failure,
            PORTABLE_SCHEMA_VERSION
        )),
        "portable_migration_test"
    );
}

#[test]
fn fresh_catalog_classification_rejects_zero_version_user_objects_before_trial_ready() {
    let root = temp_root("fresh-catalog-classification");
    let absent = root.path().join("absent.db");
    assert!(matches!(
        classify_catalog(&absent).expect("absent catalog is fresh"),
        CatalogClassification::Fresh
    ));

    let empty = root.path().join("empty.db");
    drop(Connection::open(&empty).expect("create zero-version SQLite catalog"));
    assert!(matches!(
        classify_catalog(&empty).expect("empty zero-version catalog is fresh"),
        CatalogClassification::Fresh
    ));

    let contaminated = root.path().join("contaminated.db");
    let connection = Connection::open(&contaminated).expect("create contaminated SQLite catalog");
    connection
        .execute_batch("CREATE TABLE user_state (id INTEGER PRIMARY KEY)")
        .expect("create user SQLite object");
    drop(connection);
    assert_eq!(
        error_code(classify_catalog(&contaminated)),
        "portable_fresh_catalog"
    );
}

#[test]
fn schema_15_to_current_runs_the_wire_protocol_and_writes_exact_path_tags() {
    let root = temp_root("schema-15-current");
    let paths = portable_paths(root.path());
    create_data_root(&paths);
    catalog_with_version(&paths.catalog_db_path, 15);
    let transaction = hash('3');

    let first = migrate_through_protocol(&paths, 15, &transaction)
        .expect("migrate supported schema through the exact wire protocol");
    assert_eq!(first.source_version, 15);
    assert_eq!(first.target_version, PORTABLE_SCHEMA_VERSION);
    assert!(
        paths
            .update_root
            .join("transactions")
            .join(&transaction)
            .join("migration-receipt.json")
            .is_file()
    );
    assert_eq!(
        user_version(&paths.catalog_db_path),
        PORTABLE_SCHEMA_VERSION
    );
    let tags = Connection::open(&paths.catalog_db_path)
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
}

#[test]
fn released_v16_catalog_migrates_through_the_app_session_fail_closed() {
    let root = temp_root("schema-16-current");
    let paths = portable_paths(root.path());
    create_data_root(&paths);
    catalog_with_version(&paths.catalog_db_path, 16);
    Connection::open(&paths.catalog_db_path)
        .expect("open released v16 catalog")
        .execute(
            "INSERT INTO games (id, title, launcher, platform, runtime, install_path, install_key, root_authority, executable_candidates_json)
             VALUES (?1, ?2, 'manual', 'windows', 'native', ?3, ?4, 'user_confirmed', '[]')",
            (
                "v16-game",
                "V16 game",
                "C:/Games/V16",
                "c:/games/v16",
            ),
        )
        .expect("insert released v16 game");
    let transaction = hash('5');

    let report = migrate_through_protocol(&paths, 16, &transaction)
        .expect("migrate released v16 through the authenticated App session");
    assert_eq!(report.source_version, 16);
    assert_eq!(report.target_version, PORTABLE_SCHEMA_VERSION);
    assert_eq!(
        user_version(&paths.catalog_db_path),
        PORTABLE_SCHEMA_VERSION
    );

    let migrated = Connection::open(&paths.catalog_db_path).expect("open migrated v17 catalog");
    let (readiness, epoch): (String, i64) = migrated
        .query_row(
            "SELECT readiness, authority_epoch FROM catalog_scan_authority WHERE game_id = 'v16-game'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read fail-closed authority seeded for the v16 game");
    assert_eq!(readiness, "never_completed");
    assert_eq!(epoch, 0);
    for obsolete in ["file_hash_cache", "scan_source_checkpoints"] {
        let exists: bool = migrated
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [obsolete],
                |row| row.get(0),
            )
            .expect("inspect obsolete v16 scan table");
        assert!(!exists, "{obsolete} must not survive the v17 migration");
    }
    drop(migrated);

    let snapshot_catalog = paths
        .update_root
        .join("transactions")
        .join(transaction)
        .join("snapshot/catalog.db");
    assert_eq!(user_version(&snapshot_catalog), 16);
}

#[test]
fn released_v4_catalog_migrates_through_the_supervisor_boundary() {
    let root = temp_root("schema-4-current");
    let paths = portable_paths(root.path());
    create_data_root(&paths);
    catalog_v4_with_user_data(&paths.catalog_db_path);
    let receipt = migrate_through_protocol(&paths, 4, &hash('4'))
        .expect("migrate released v4 through the exact wire protocol");
    assert_eq!(receipt.source_version, 4);
    assert_eq!(receipt.target_version, PORTABLE_SCHEMA_VERSION);
    assert_eq!(
        Connection::open(&paths.catalog_db_path)
            .expect("open migrated catalog")
            .query_row(
                "SELECT title FROM games WHERE id = 'preserved-game'",
                [],
                |row| row.get::<_, String>(0)
            )
            .expect("read preserved v4 game"),
        "Preserved game"
    );
}

#[test]
fn current_schema_launch_does_not_allocate_a_rollback_snapshot() {
    let root = temp_root("schema-current-no-snapshot");
    let paths = portable_paths(root.path());
    create_data_root(&paths);
    create_current_catalog(&paths.catalog_db_path);
    let transaction = hash('6');
    let report = migrate_through_protocol(&paths, PORTABLE_SCHEMA_VERSION, &transaction)
        .expect("validate current schema through the exact wire protocol");
    assert_eq!(report.source_version, PORTABLE_SCHEMA_VERSION);
    assert_eq!(report.target_version, PORTABLE_SCHEMA_VERSION);
    assert!(
        !paths
            .update_root
            .join("transactions")
            .join(&transaction)
            .join("snapshot")
            .exists(),
        "ordinary current-schema launches must not accumulate full catalog copies"
    );
    assert!(
        !paths
            .update_root
            .join("transactions")
            .join(&transaction)
            .join("migration-receipt.json")
            .exists(),
        "observational validation must not create a migration receipt"
    );
}

#[test]
fn native_epoch_rejects_a_future_generation_schema_capability() {
    let root = temp_root("future-generation-migration");
    let paths = portable_paths(root.path());
    create_data_root(&paths);
    let handshake =
        PreparedMigrationHandshake::new(&paths, &hash('8'), PORTABLE_SCHEMA_VERSION + 1)
            .expect("construct crossed-epoch startup evidence");

    assert_eq!(
        error_code(handshake.startup.validate()),
        "portable_startup_invalid",
        "a successor cannot turn the native exact schema epoch into a range"
    );
}

#[test]
fn unsupported_schema_fails_closed_without_inference() {
    let root = temp_root("schema-closed");
    let paths = portable_paths(root.path());
    create_data_root(&paths);
    catalog_with_version(&paths.catalog_db_path, 14);
    assert_eq!(
        error_code(migrate_through_protocol(&paths, 14, &hash('7'))),
        "portable_storage_schema"
    );
    assert_eq!(user_version(&paths.catalog_db_path), 14);
    assert!(
        !Connection::open(&paths.catalog_db_path)
            .expect("open unchanged catalog")
            .prepare("SELECT tag FROM portable_path_tags")
            .is_ok(),
        "unsupported legacy inference must not create tagged authority"
    );
}
