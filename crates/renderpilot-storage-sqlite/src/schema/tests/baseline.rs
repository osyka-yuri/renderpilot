use super::*;

#[test]
fn apply_creates_catalog_schema() {
    let mut connection = open_test_connection();

    apply(&mut connection).expect("migration should succeed");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert_catalog_schema_exists(&connection);
    assert_preparing_state_is_accepted(&connection);
}

#[test]
fn apply_is_idempotent() {
    let mut connection = open_test_connection();

    apply(&mut connection).expect("first migration should succeed");
    apply(&mut connection).expect("second migration should succeed");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert_catalog_schema_exists(&connection);
}

#[test]
fn g_obs_01_healthy_current_apply_is_authorizer_observational() {
    let dir = std::env::temp_dir().join(format!(
        "renderpilot-schema-healthy-current-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    let db_path = dir.join("catalog.db");

    {
        let mut connection = Connection::open(&db_path).expect("open file db");
        apply(&mut connection).expect("initial");
        connection
            .execute(
                "INSERT INTO settings (key, value) VALUES ('keep_marker', 'alive')",
                [],
            )
            .expect("marker");

        let denied = install_observational_authorizer(&connection);
        apply(&mut connection).expect("healthy keep must not execute a mutation");
        assert_eq!(
            denied.load(Ordering::Relaxed),
            0,
            "healthy current validation must not attempt a denied mutation"
        );

        let marker: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'keep_marker'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(marker, 1);
        assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    }

    let backups: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("list temp")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "bak"))
        .collect();
    assert!(backups.is_empty(), "healthy current keep must not back up");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_resets_unversioned_existing_schema() {
    let mut connection = open_test_connection();

    connection
        .execute_batch(
            r#"
            CREATE TABLE legacy_catalog_marker (id INTEGER PRIMARY KEY);
            CREATE INDEX idx_legacy_catalog_marker_id ON legacy_catalog_marker (id);
            CREATE VIEW legacy_catalog_view AS SELECT id FROM legacy_catalog_marker;
            CREATE TRIGGER trg_legacy_catalog_marker_insert
            AFTER INSERT ON legacy_catalog_marker
            BEGIN
                SELECT NEW.id;
            END;
            "#,
        )
        .expect("legacy schema should be created");

    apply(&mut connection).expect("legacy schema should be rebuilt");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert_catalog_schema_exists(&connection);

    assert!(!schema_object_exists(
        &connection,
        "table",
        "legacy_catalog_marker"
    ));
    assert!(!schema_object_exists(
        &connection,
        "index",
        "idx_legacy_catalog_marker_id"
    ));
    assert!(!schema_object_exists(
        &connection,
        "view",
        "legacy_catalog_view"
    ));
    assert!(!schema_object_exists(
        &connection,
        "trigger",
        "trg_legacy_catalog_marker_insert"
    ));
}

#[test]
fn apply_rebuilds_stale_v2_schema_with_old_artifact_shape() {
    let mut connection = open_test_connection();

    // Simulate a pre-bundle v2 catalog: `library_artifacts` with the OLD scalar
    // columns and no `component_backups` table.
    connection
        .execute_batch(
            r#"
            CREATE TABLE games (id TEXT PRIMARY KEY);
            CREATE TABLE library_artifacts (
                id TEXT PRIMARY KEY, library TEXT, file_name TEXT,
                file_path TEXT, version TEXT, sha256 TEXT
            );
            PRAGMA user_version = 2;
            "#,
        )
        .expect("legacy v2 schema should be created");

    apply(&mut connection).expect("stale v2 schema should be rebuilt");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert!(table_has_column(
        &connection,
        "library_artifacts",
        "files_json"
    ));
    assert!(!table_has_column(
        &connection,
        "library_artifacts",
        "file_path"
    ));
    assert!(schema_object_exists(
        &connection,
        "table",
        "component_backups"
    ));
}

#[test]
fn apply_migrates_v8_to_current_without_rebuilding_catalog_rows() {
    let mut connection = open_test_connection();

    apply(&mut connection).expect("initial migration should succeed");
    connection
        .execute(
            "
            INSERT INTO installed_addons
                (game_id, kind, addon_file, addon_version,
                 created_files_json, backed_up_files_json, tracked_sources_json)
            VALUES
                ('steam:42', 'renodx', 'C:/Games/Test/renodx-test.addon64', NULL,
                 '[\"C:/Games/Test/renodx-test.addon64\"]', '[]', '[]')
            ",
            [],
        )
        .expect("installed addon should insert");
    connection
        .execute_batch(
            "
            DROP TRIGGER IF EXISTS trg_shared_artifacts_touch_updated_at;
            DROP TABLE IF EXISTS shared_artifacts;
            PRAGMA user_version = 8;
            ",
        )
        .expect("database should be downgraded to v8 shape");

    apply(&mut connection).expect("v8 schema should migrate in place");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert_catalog_schema_exists(&connection);
    assert!(schema_object_exists(
        &connection,
        "table",
        "shared_artifacts"
    ));
    assert!(table_has_column(
        &connection,
        "installed_addons",
        "host_kind"
    ));
    let addon_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM installed_addons", [], |row| {
            row.get(0)
        })
        .expect("installed addon count should be readable");
    assert_eq!(addon_count, 1);
    assert_preparing_state_is_accepted(&connection);
}

#[test]
fn apply_migrates_v9_to_current_additively_and_preserves_addon_rows() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial migration should succeed");
    connection
        .execute(
            "
            INSERT INTO installed_addons
                (game_id, kind, addon_file, addon_version,
                 created_files_json, backed_up_files_json, managed_files_json,
                 tracked_sources_json)
            VALUES
                ('steam:43', 'luma', 'C:/Games/Test/Luma-Test.addon', NULL,
                 '[\"C:/Games/Test/Luma-Test.addon\"]', '[]', '[]', '[]')
            ",
            [],
        )
        .expect("installed addon should insert");
    connection
        .execute_batch(REDUCE_INSTALLED_ADDONS_TO_V9)
        .expect("database should be reduced to v9 shape");

    apply(&mut connection).expect("v9 schema should migrate in place");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert!(table_has_column(
        &connection,
        "installed_addons",
        "managed_files_json"
    ));
    assert!(schema_object_exists(
        &connection,
        "table",
        "pending_file_mutations"
    ));
    let row: (i64, String) = connection
        .query_row(
            "SELECT COUNT(*), managed_files_json FROM installed_addons WHERE game_id = 'steam:43'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("migrated row");
    assert_eq!(row, (1, "[]".to_owned()));
    assert_preparing_state_is_accepted(&connection);
}

#[test]
fn apply_migrates_v11_component_backups_with_empty_auxiliary_array() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial migration");
    connection
        .execute_batch(
            "
            INSERT INTO games
                (id, title, launcher, platform, runtime, install_path,
                 install_key, root_authority, executable_candidates_json)
            VALUES
                ('steam:v11', 'Legacy', 'Steam', 'Windows', 'NativeWindows', 'C:/Game',
                 'c:/game', 'legacy', '[]');
            INSERT INTO component_backups
                (component_id, game_id, files_json)
            VALUES
                ('component:v11', 'steam:v11', '[]');
            ALTER TABLE component_backups DROP COLUMN auxiliary_json;
            PRAGMA user_version = 11;
            ",
        )
        .expect("reduce to v11");

    apply(&mut connection).expect("v11 migration");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    let auxiliary: String = connection
        .query_row(
            "SELECT auxiliary_json FROM component_backups WHERE component_id = 'component:v11'",
            [],
            |row| row.get(0),
        )
        .expect("auxiliary json");
    assert_eq!(auxiliary, "[]");
}

#[test]
fn apply_migrates_v12_to_v13_as_one_release_boundary() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial migration");
    reduce_current_to_v14(&connection);
    connection
        .execute_batch(
            "
            INSERT INTO settings (key, value) VALUES ('v12_marker', 'preserved');
            DROP TABLE profile_addon_capabilities;
            DROP TABLE IF EXISTS scan_source_checkpoints;
            PRAGMA user_version = 12;
            ",
        )
        .expect("reduce to v12");

    apply(&mut connection).expect("v12 migration");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert!(schema_object_exists(
        &connection,
        "table",
        "profile_addon_capabilities"
    ));
    assert!(!schema_object_exists(
        &connection,
        "table",
        "scan_source_checkpoints"
    ));
    let marker: String = connection
        .query_row(
            "SELECT value FROM settings WHERE key = 'v12_marker'",
            [],
            |row| row.get(0),
        )
        .expect("preserved marker");
    assert_eq!(marker, "preserved");
}
