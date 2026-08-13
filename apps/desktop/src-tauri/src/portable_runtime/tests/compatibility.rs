use rusqlite::Connection;

use super::{error_code, hash, temp_root};
use crate::portable_runtime::{
    app_catalog_migration::{CatalogClassification, classify_catalog},
    app_protocol::{
        AppControlMessage, AppStatusMessage, CatalogMigrationOperation, CatalogMigrationReport,
    },
    journal::{JournalPhase, read_entries},
    rpu::MAXIMUM_SCHEMA as PORTABLE_SCHEMA_VERSION,
    signature::sha256_file,
};

use super::compatibility_support::{
    PreparedMigrationHandshake, ScriptedMigrationTrial, catalog_v4_with_user_data,
    catalog_with_version, create_current_catalog, create_data_root, in_process_future_trial,
    migrate_through_protocol, portable_paths, user_version,
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
fn stable_supervisor_accepts_a_real_future_generation_migration() {
    let root = temp_root("future-generation-migration");
    let paths = portable_paths(root.path());
    create_data_root(&paths);
    create_current_catalog(&paths.catalog_db_path);
    Connection::open(&paths.catalog_db_path)
        .expect("open current catalog fixture")
        .execute(
            "INSERT INTO games (id, title, launcher, platform, runtime, install_path, install_key, root_authority, executable_candidates_json)
             VALUES (?1, ?2, 'manual', 'windows', 'native', ?3, ?4, 'user_confirmed', '[]')",
            ("future-migration-game", "Future Migration Game", "C:/Games/FutureMigration", "c:/games/futuremigration"),
        )
        .expect("insert user data before future migration");

    let catalog_before = sha256_file(&paths.catalog_db_path).expect("hash current catalog");
    let future_schema = PORTABLE_SCHEMA_VERSION + 1;
    let transaction = hash('8');
    let handshake = PreparedMigrationHandshake::new(&paths, &transaction, future_schema)
        .expect("prepare current supervisor for a future generation");
    let mut trial = in_process_future_trial(&handshake, &paths, PORTABLE_SCHEMA_VERSION);
    handshake
        .prepare_catalog(&paths, &mut trial, PORTABLE_SCHEMA_VERSION)
        .expect("current supervisor accepts the future App migration");
    let report = trial.last_report.expect("future App sends its report");
    assert_eq!(report.source_version, PORTABLE_SCHEMA_VERSION);
    assert_eq!(report.target_version, future_schema);
    assert_ne!(report.catalog_sha256, catalog_before);
    assert_eq!(user_version(&paths.catalog_db_path), future_schema);
    let migrated = Connection::open(&paths.catalog_db_path).expect("open future catalog");
    let (title, sort_title): (String, String) = migrated
        .query_row(
            "SELECT title, future_sort_title FROM games WHERE id = 'future-migration-game'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("read user data transformed by the future migration");
    assert_eq!(title, "Future Migration Game");
    assert_eq!(sort_title, "future migration game");
    drop(migrated);
    let transaction_root = paths.update_root.join("transactions").join(&transaction);
    let snapshot_catalog = transaction_root.join("snapshot/catalog.db");
    assert_eq!(user_version(&snapshot_catalog), PORTABLE_SCHEMA_VERSION);
    assert!(
        Connection::open(&snapshot_catalog)
            .expect("open pre-migration snapshot")
            .prepare("SELECT future_sort_title FROM games")
            .is_err(),
        "the future column must exist only in the migrated live catalog"
    );
    assert!(transaction_root.join("snapshot-receipt.json").is_file());
    assert!(transaction_root.join("migration-receipt.json").is_file());
    assert_eq!(
        read_entries(&handshake.journal)
            .expect("read accepted future migration journal")
            .last()
            .map(|entry| entry.phase),
        Some(JournalPhase::MigrationCommitted)
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
