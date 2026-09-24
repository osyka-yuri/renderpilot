use super::*;

#[test]
fn apply_resets_unknown_schema_version() {
    let mut connection = open_test_connection();

    connection
        .execute_batch("PRAGMA user_version = 999;")
        .expect("schema version should be set");

    apply(&mut connection).expect("unknown version should be rebuilt");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert_catalog_schema_exists(&connection);
}

#[test]
fn apply_rebuilds_table_absent_v19_catalog_to_the_current_contract() {
    let mut connection = open_test_connection();
    connection
        .execute_batch("PRAGMA user_version = 19;")
        .expect("schema version should be set");

    apply(&mut connection).expect("v19 catalog should be rebuilt to v20");

    assert_eq!(CURRENT_SCHEMA_VERSION, 21);
    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    super::super::validation::validate_catalog_schema(&connection)
        .expect("rebuilt catalog should satisfy the canonical contract");
    assert!(table_has_column(
        &connection,
        "installed_addons",
        "renodx_config_receipt_json"
    ));
    assert!(table_has_column(
        &connection,
        "installed_addons",
        "engine_config_journal_json"
    ));
}

#[test]
fn apply_restores_foreign_keys_state() {
    let mut connection = open_test_connection();

    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("foreign keys should be enabled");

    apply(&mut connection).expect("migration should succeed");

    assert!(foreign_keys_enabled(&connection));
}

#[test]
fn apply_preserves_disabled_foreign_keys_state() {
    let mut connection = open_test_connection();

    connection
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .expect("foreign keys should be disabled");

    apply(&mut connection).expect("migration should succeed");

    assert!(!foreign_keys_enabled(&connection));
}

/// Column-contract: the shared runtime diff is empty after the bundled DDL is
/// applied, so physical constants cannot drift from the migration silently.
#[test]
fn contract_physical_columns_match_schema() {
    let mut connection = open_test_connection();
    super::apply(&mut connection).expect("migration should succeed");
    assert!(
        super::super::validation::physical_column_mismatches(&connection)
            .expect("physical-column validation should query")
            .is_empty()
    );
}

#[test]
fn contract_required_tables_match_physical_contract() {
    assert_eq!(REQUIRED_TABLES.len(), CONTRACT_TABLES.len());
    assert_eq!(REQUIRED_TABLES.len(), physical::CONTRACT_TABLES.len());
    for (index, &table) in REQUIRED_TABLES.iter().enumerate() {
        assert_eq!(
            CONTRACT_TABLES[index].0, table,
            "REQUIRED_TABLES and CONTRACT_TABLES order/name drift at {index}"
        );
        assert_eq!(physical::CONTRACT_TABLES[index].0, table);
    }
}

#[test]
fn every_game_or_component_scoped_table_has_a_consolidation_policy() {
    let policy_tables: std::collections::HashSet<&str> = CONSOLIDATION_POLICIES
        .iter()
        .map(|(table, _)| *table)
        .collect();

    for &(table, columns) in CONTRACT_TABLES {
        let scoped = columns
            .iter()
            .any(|column| *column == "component_id" || column.ends_with("game_id"));
        if scoped {
            assert!(
                policy_tables.contains(table),
                "table {table} is game/component scoped but has no consolidation policy"
            );
        }
    }

    for table in policy_tables {
        assert!(
            CONTRACT_TABLES.iter().any(|(known, _)| *known == table),
            "consolidation policy references unknown table {table}"
        );
    }
}

#[test]
fn apply_rebuilds_current_schema_with_an_unexpected_physical_column() {
    let mut connection = open_test_connection();
    super::apply(&mut connection).expect("migration should succeed");
    connection
        .execute_batch("ALTER TABLE games ADD COLUMN unexpected_column TEXT;")
        .expect("schema should accept the extra column");

    assert!(
        !super::super::validation::physical_column_mismatches(&connection)
            .expect("physical-column validation should query")
            .is_empty()
    );
    let error = super::super::validation::validate_catalog_schema(&connection)
        .expect_err("unexpected physical column should invalidate schema");
    assert!(
        error
            .message()
            .contains("unexpected column games.unexpected_column")
    );

    super::apply(&mut connection).expect("unexpected physical column should rebuild schema");

    assert!(
        super::super::validation::physical_column_mismatches(&connection)
            .expect("rebuilt schema should validate")
            .is_empty()
    );
}

#[test]
fn contract_rejects_pending_mutations_without_preparing() {
    let mut connection = open_test_connection();
    super::apply(&mut connection).expect("baseline");
    connection
        .execute_batch(LEGACY_PENDING_WITHOUT_PREPARING)
        .expect("broken CHECK");

    assert!(!super::super::validation::catalog_schema_is_valid(&connection).expect("validate"));
    let error =
        super::super::validation::validate_catalog_schema(&connection).expect_err("must fail");
    assert!(error.message().contains("preparing"));
}

#[test]
fn v21_nvapi_setting_claims_reject_null_present_values_and_u32_overflow() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("apply v21 schema");
    connection
        .execute(
            "INSERT INTO nvapi_drs_targets
                (target_id, profile_name, profile_is_predefined, kind, identity_json)
             VALUES ('profile', 'Profile', 0, 'external', '{}')",
            [],
        )
        .expect("seed NVIDIA target");

    let invalid = [
        ("original present with NULL value", "1, NULL, 0, NULL"),
        ("expected present with NULL value", "0, NULL, 1, NULL"),
        ("original value exceeds DWORD", "1, 4294967296, 0, NULL"),
        ("expected value exceeds DWORD", "0, NULL, 1, 4294967296"),
    ];
    for (setting_id, (case, values)) in invalid.into_iter().enumerate() {
        let sql = format!(
            "INSERT INTO nvapi_target_setting_claims
                (target_id, setting_id, original_present, original_value,
                 expected_present, expected_value)
             VALUES ('profile', {setting_id}, {values})"
        );
        assert!(
            connection.execute(&sql, []).is_err(),
            "schema must reject {case}"
        );
    }

    let overflow_id = connection.execute(
        "INSERT INTO nvapi_target_setting_claims
            (target_id, setting_id, original_present, original_value,
             expected_present, expected_value)
         VALUES ('profile', 4294967296, 0, NULL, 0, NULL)",
        [],
    );
    assert!(
        overflow_id.is_err(),
        "schema must reject a setting id over u32"
    );
}

#[test]
fn compose_baseline_is_non_empty_and_includes_shared_fragments() {
    let baseline = ddl::compose_baseline();
    assert!(baseline.contains("CREATE TABLE IF NOT EXISTS games"));
    assert!(baseline.contains("CREATE TABLE IF NOT EXISTS pending_file_mutations"));
    assert!(baseline.contains("CREATE TABLE IF NOT EXISTS shared_artifacts"));
    assert!(baseline.contains("trg_shared_artifacts_touch_updated_at"));
    assert!(baseline.contains("'preparing'"));
}

#[test]
fn apply_backs_up_malformed_stamped_v10_before_post_upgrade_rebuild() {
    let dir =
        std::env::temp_dir().join(format!("renderpilot-schema-backup-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    let db_path = dir.join("catalog.db");

    {
        let mut connection = Connection::open(&db_path).expect("open file db");
        apply(&mut connection).expect("initial apply");
        connection
            .execute(
                "INSERT INTO settings (key, value) VALUES ('marker', 'alive')",
                [],
            )
            .expect("marker");
        connection
            .execute_batch(
                "
                DROP TRIGGER IF EXISTS trg_shared_artifacts_touch_updated_at;
                PRAGMA user_version = 10;
                ",
            )
            .expect("break schema");
        apply(&mut connection).expect("rebuild with backup");
        assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
        let marker: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'marker'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(marker, 0);
    }

    let backups: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("list temp")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.contains(&format!(".pre-migration-v{CURRENT_SCHEMA_VERSION}."))
                        && name.ends_with(".bak")
                })
        })
        .collect();
    assert_eq!(
        backups.len(),
        1,
        "expected one pre-migration backup before the transactional rebuild"
    );

    let backup = Connection::open(&backups[0]).expect("open backup");
    assert_eq!(user_version(&backup), 10);
    let restored: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'marker'",
            [],
            |row| row.get(0),
        )
        .expect("marker in backup");
    assert_eq!(restored, 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn apply_backs_up_file_database_before_v15_and_v16_migrations() {
    let dir = std::env::temp_dir().join(format!(
        "renderpilot-schema-v15-backup-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    let db_path = dir.join("catalog.db");

    {
        let mut connection = Connection::open(&db_path).expect("open file db");
        apply(&mut connection).expect("current baseline");
        seed_v14_migration_aggregate(&connection);
        reduce_current_to_v14(&connection);
        apply(&mut connection).expect("migrate file database");
        assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
        ddl::portable_path_tags::validate(&connection).expect("canonical v16 path tags");
    }

    let backups: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("list temp")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.contains(&format!(".pre-migration-v{CURRENT_SCHEMA_VERSION}."))
                        && name.ends_with(".bak")
                })
        })
        .collect();
    assert_eq!(backups.len(), 1, "expected one pre-migration backup");

    let backup = Connection::open(&backups[0]).expect("open backup");
    assert_eq!(user_version(&backup), 14);
    assert!(table_has_column(&backup, "components", "library"));
    assert!(!table_has_column(&backup, "components", "technology"));
    let preserved: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM operation_items WHERE artifact_id = 'artifact:v12'",
            [],
            |row| row.get(0),
        )
        .expect("operation item in backup");
    assert_eq!(preserved, 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn v20_to_v21_backup_preserves_discarded_baselines_and_other_data() {
    let directory = tempfile::tempdir().expect("temporary catalog directory");
    let db_path = directory.path().join("catalog.db");

    {
        let mut connection = Connection::open(&db_path).expect("open file database");
        apply(&mut connection).expect("create current fixture schema");
        connection
            .execute(
                "INSERT INTO settings (key, value) VALUES ('migration-marker', 'preserved')",
                [],
            )
            .expect("write preservation marker");
        connection
            .execute_batch(
                "
                DROP TRIGGER IF EXISTS trg_games_restrict_nvapi_owned_delete;
                DROP TABLE pending_drs_operations;
                DROP TABLE nvapi_game_claim_refs;
                DROP TABLE nvapi_target_setting_claims;
                DROP TABLE nvapi_drs_target_app_witnesses;
                DROP TABLE nvapi_drs_targets;
                DROP TABLE nvapi_owned_profiles;
                INSERT INTO games (
                    id, title, launcher, platform, runtime, install_path, install_key,
                    root_authority, executable_candidates_json
                ) VALUES (
                    'manual:legacy-v20', 'Legacy game', 'manual', 'windows',
                    'native_windows', 'C:/Games/Legacy', 'manual:/games/legacy',
                    'user_confirmed', '[]'
                );
                CREATE TABLE nvapi_setting_baselines (
                    game_id TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                    setting_key TEXT NOT NULL,
                    baseline_dword INTEGER NOT NULL,
                    baseline_was_predefined INTEGER NOT NULL,
                    predefined_dword INTEGER,
                    captured_exe TEXT NOT NULL,
                    captured_at INTEGER NOT NULL,
                    PRIMARY KEY (game_id, setting_key)
                ) STRICT;
                INSERT INTO nvapi_setting_baselines VALUES (
                    'manual:legacy-v20', 'dlss_sr_render_preset', 3, 0, 0, 'game.exe', 7
                );
                PRAGMA user_version = 20;
                ",
            )
            .expect("restore v20 shape and seed old state");

        apply(&mut connection).expect("run v20 to v21 migration");
        assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
        let old_table_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'nvapi_setting_baselines')",
                [],
                |row| row.get(0),
            )
            .expect("check obsolete baseline table");
        assert!(!old_table_exists);
        let marker: String = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'migration-marker'",
                [],
                |row| row.get(0),
            )
            .expect("preserved setting");
        assert_eq!(marker, "preserved");
        let games: i64 = connection
            .query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))
            .expect("preserved games");
        assert_eq!(games, 1);
    }

    let backups: Vec<PathBuf> = fs::read_dir(directory.path())
        .expect("list backup directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".pre-migration-v21.") && name.ends_with(".bak"))
        })
        .collect();
    assert_eq!(
        backups.len(),
        1,
        "migration must create one pre-change backup"
    );

    let backup = Connection::open(&backups[0]).expect("open pre-migration backup");
    assert_eq!(user_version(&backup), 20);
    let baseline_rows: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM nvapi_setting_baselines WHERE game_id = 'manual:legacy-v20'",
            [],
            |row| row.get(0),
        )
        .expect("old baseline is retained in backup");
    assert_eq!(baseline_rows, 1);
    let marker: String = backup
        .query_row(
            "SELECT value FROM settings WHERE key = 'migration-marker'",
            [],
            |row| row.get(0),
        )
        .expect("other data is retained in backup");
    assert_eq!(marker, "preserved");
}
