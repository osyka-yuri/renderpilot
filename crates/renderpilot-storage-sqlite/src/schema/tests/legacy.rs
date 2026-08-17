use super::*;

#[test]
fn v10_migration_preserves_artifact_rows_with_empty_metadata() {
    let connection = open_test_connection();
    connection
        .execute_batch(
            r#"
            CREATE TABLE library_artifacts (id TEXT PRIMARY KEY NOT NULL) STRICT;
            INSERT INTO library_artifacts (id) VALUES ('artifact:legacy');
            PRAGMA user_version = 10;
            "#,
        )
        .expect("v10 artifact row should be seeded");

    super::super::steps::run_v10_to_v11_for_test(&connection)
        .expect("v10 step should migrate additively");
    super::super::steps::run_v10_to_v11_for_test(&connection)
        .expect("v10 step should be idempotent");

    let metadata: String = connection
        .query_row(
            "SELECT metadata_json FROM library_artifacts WHERE id = 'artifact:legacy'",
            [],
            |row| row.get(0),
        )
        .expect("legacy artifact should survive migration");
    assert_eq!(metadata, "{}");
}

#[test]
fn v10_migration_normalizes_legacy_trust_levels_without_losing_unknown_values() {
    let connection = open_test_connection();
    connection
        .execute_batch(
            r#"
            CREATE TABLE library_artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                trust_level TEXT NOT NULL
            ) STRICT;
            INSERT INTO library_artifacts (id, trust_level) VALUES
                ('a', 'LocalObserved'),
                ('b', 'UserImported'),
                ('c', 'ManifestDownloaded'),
                ('d', 'CatalogDownloaded'),
                ('e', 'Unknown'),
                ('f', 'FutureTrusted');
            PRAGMA user_version = 10;
            "#,
        )
        .expect("v10 trust levels should be seeded");

    super::super::steps::run_v10_to_v11_for_test(&connection)
        .expect("v10 step should migrate additively");
    super::super::steps::run_v10_to_v11_for_test(&connection)
        .expect("v10 step should be idempotent");

    let values: Vec<(String, String)> = connection
        .prepare("SELECT id, trust_level FROM library_artifacts ORDER BY id")
        .expect("trust query")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("trust rows")
        .collect::<Result<_, _>>()
        .expect("trust rows should decode");
    assert_eq!(
        values,
        vec![
            ("a".to_owned(), "local_observed".to_owned()),
            ("b".to_owned(), "user_imported".to_owned()),
            ("c".to_owned(), "catalog_downloaded".to_owned()),
            ("d".to_owned(), "catalog_downloaded".to_owned()),
            ("e".to_owned(), "unknown".to_owned()),
            ("f".to_owned(), "FutureTrusted".to_owned()),
        ]
    );
}

#[test]
fn v10_migration_removes_only_legacy_manifest_registrations() {
    let connection = open_test_connection();
    connection
        .execute_batch(
            r#"
            CREATE TABLE library_artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                trust_level TEXT NOT NULL,
                source TEXT
            ) STRICT;
            INSERT INTO library_artifacts (id, trust_level, source) VALUES
                ('legacy-manifest', 'ManifestDownloaded', 'manifest-v0'),
                ('legacy-without-source', 'ManifestDownloaded', NULL),
                ('catalog-v1', 'CatalogDownloaded', 'catalog-v1'),
                ('local', 'LocalObserved', 'game-scan'),
                ('future', 'FutureTrusted', 'future-source');
            PRAGMA user_version = 10;
            "#,
        )
        .expect("v10 artifact rows should be seeded");

    super::super::steps::run_v10_to_v11_for_test(&connection)
        .expect("v10 step should migrate additively");
    super::super::steps::run_v10_to_v11_for_test(&connection)
        .expect("v10 step should be idempotent");

    let values: Vec<(String, String, Option<String>)> = connection
        .prepare("SELECT id, trust_level, source FROM library_artifacts ORDER BY id")
        .expect("artifact query")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("artifact rows")
        .collect::<Result<_, _>>()
        .expect("artifact rows should decode");
    assert_eq!(
        values,
        vec![
            (
                "catalog-v1".to_owned(),
                "catalog_downloaded".to_owned(),
                Some("catalog-v1".to_owned()),
            ),
            (
                "future".to_owned(),
                "FutureTrusted".to_owned(),
                Some("future-source".to_owned()),
            ),
            (
                "local".to_owned(),
                "local_observed".to_owned(),
                Some("game-scan".to_owned()),
            ),
        ]
    );
}

#[test]
fn apply_backs_up_and_rebuilds_malformed_current_pending_mutations() {
    let dir = std::env::temp_dir().join(format!(
        "renderpilot-schema-malformed-current-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    let db_path = dir.join("catalog.db");

    {
        let mut connection = Connection::open(&db_path).expect("open file db");
        apply(&mut connection).expect("initial migration should succeed");
        connection
            .execute(
                "INSERT INTO settings (key, value) VALUES ('keep_me', 'alive')",
                [],
            )
            .expect("marker setting should insert");
        connection
            .execute_batch(LEGACY_PENDING_WITHOUT_PREPARING)
            .expect("malformed current pending_file_mutations shape");
        connection
            .execute(
                r#"
                INSERT INTO pending_file_mutations
                    (id, game_id, feature, subject_id, state, manifest_json)
                VALUES
                    ('tx-malformed-current', 'steam:1', 'catalog_swap', NULL, 'prepared',
                     '{"format_version":1,"roots":[],"transaction_dir":"x","snapshots":[]}')
                "#,
                [],
            )
            .expect("seed malformed current row");

        assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
        apply(&mut connection).expect("malformed current schema should rebuild");

        assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
        assert_catalog_schema_exists(&connection);
        assert_preparing_state_is_accepted(&connection);
        let marker: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'keep_me'",
                [],
                |row| row.get(0),
            )
            .expect("settings should be readable after rebuild");
        assert_eq!(marker, 0);
    }

    let backups: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("list temp")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".pre-rebuild.") && name.ends_with(".bak"))
        })
        .collect();
    assert_eq!(
        backups.len(),
        1,
        "malformed current schema needs one backup"
    );

    let backup = Connection::open(&backups[0]).expect("open original backup");
    assert_eq!(user_version(&backup), CURRENT_SCHEMA_VERSION);
    let marker: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'keep_me'",
            [],
            |row| row.get(0),
        )
        .expect("marker in original backup");
    assert_eq!(marker, 1);
    let preserved_mutation: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM pending_file_mutations WHERE id = 'tx-malformed-current'",
            [],
            |row| row.get(0),
        )
        .expect("malformed row in original backup");
    assert_eq!(preserved_mutation, 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_rebuilds_current_schema_when_shared_artifact_trigger_is_missing() {
    let mut connection = open_test_connection();

    apply(&mut connection).expect("initial migration should succeed");
    connection
        .execute(
            "
            INSERT INTO settings (key, value)
            VALUES ('transient_marker', 'will be rebuilt')
            ",
            [],
        )
        .expect("marker setting should insert");
    connection
        .execute_batch(
            "
            DROP TRIGGER IF EXISTS trg_shared_artifacts_touch_updated_at;
            PRAGMA user_version = 10;
            ",
        )
        .expect("schema should be made incomplete");

    apply(&mut connection).expect("incomplete current schema should rebuild");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert_catalog_schema_exists(&connection);
    assert!(schema_object_exists(
        &connection,
        "trigger",
        "trg_shared_artifacts_touch_updated_at"
    ));
    let marker_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'transient_marker'",
            [],
            |row| row.get(0),
        )
        .expect("settings should be readable after rebuild");
    assert_eq!(marker_count, 0);
}
