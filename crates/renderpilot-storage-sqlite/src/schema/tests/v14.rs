use super::*;

#[test]
fn apply_v14_consolidates_only_exact_install_key_duplicates() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial migration");
    reduce_current_to_v14(&connection);
    connection
        .execute_batch(
            "
            DROP INDEX uq_games_install_key;

            INSERT INTO games (
                id, title, launcher, platform, runtime, install_path,
                install_key, root_authority, executable_candidates_json,
                created_at, updated_at
            ) VALUES
                ('manual:keeper', 'Exact', 'Manual', 'Windows', 'NativeWindows',
                 'C:/Games/Exact', 'c:/games/exact', 'legacy', '[]', 1, 1),
                ('manual:duplicate', 'Exact', 'Manual', 'Windows', 'NativeWindows',
                 'c:/games/exact/', 'c:/games/exact', 'legacy', '[]', 2, 2),
                ('manual:child', 'Child', 'Manual', 'Windows', 'NativeWindows',
                 'C:/Games/Exact/D3D12', 'c:/games/exact/d3d12', 'legacy', '[]', 3, 3);

            INSERT INTO components (
                id, game_id, kind, library, swappability, files_json,
                created_at, updated_at
            ) VALUES (
                'component:duplicate', 'manual:duplicate', 'NativeLibrary',
                'DlssSuperResolution', 'Swappable',
                '[{\"path\":\"C:/Games/Exact/nvngx_dlss.dll\"}]', 2, 2
            );
            INSERT INTO game_ui_state (game_id, is_favorite, is_hidden, updated_at)
            VALUES
                ('manual:keeper', 0, 1, 1),
                ('manual:duplicate', 1, 0, 2);
            INSERT INTO game_covers (game_id, file_name, updated_at) VALUES
                ('manual:keeper', 'exact.webp', 1),
                ('manual:duplicate', 'exact.webp', 2);
            PRAGMA user_version = 13;
            ",
        )
        .expect("reduce to v13 and seed exact duplicates");

    apply(&mut connection).expect("v14 migration");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    let game_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))
        .expect("game count");
    assert_eq!(game_count, 2, "only exact duplicate should be merged");
    let component_owner: String = connection
        .query_row(
            "SELECT game_id FROM components WHERE id = 'component:duplicate'",
            [],
            |row| row.get(0),
        )
        .expect("component owner");
    assert_eq!(component_owner, "manual:keeper");
    let ui_state: (i64, i64) = connection
        .query_row(
            "SELECT is_favorite, is_hidden
               FROM game_ui_state WHERE game_id = 'manual:keeper'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("merged ui state");
    assert_eq!(ui_state, (1, 1));
    let child_exists: i64 = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM games WHERE id = 'manual:child')",
            [],
            |row| row.get(0),
        )
        .expect("child existence");
    assert_eq!(
        child_exists, 1,
        "nested legacy card is not a startup heuristic"
    );
}

#[test]
fn apply_v14_refuses_lossy_exact_duplicate_conflicts() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial migration");
    reduce_current_to_v14(&connection);
    connection
        .execute_batch(
            "
            DROP INDEX uq_games_install_key;
            INSERT INTO games (
                id, title, launcher, platform, runtime, install_path,
                install_key, root_authority, executable_candidates_json,
                created_at, updated_at
            ) VALUES
                ('manual:keeper', 'Exact', 'Manual', 'Windows', 'NativeWindows',
                 'C:/Games/Exact', 'c:/games/exact', 'legacy', '[]', 1, 1),
                ('manual:duplicate', 'Exact', 'Manual', 'Windows', 'NativeWindows',
                 'c:/games/exact/', 'c:/games/exact', 'legacy', '[]', 2, 2);
            INSERT INTO game_covers (game_id, file_name, updated_at) VALUES
                ('manual:keeper', 'keeper.webp', 1),
                ('manual:duplicate', 'duplicate.webp', 2);
            PRAGMA user_version = 13;
            ",
        )
        .expect("v13 duplicate conflict fixture");

    let error = apply(&mut connection).expect_err("lossy migration must fail closed");

    assert!(
        error
            .message()
            .contains("without discarding conflicting game_covers")
    );
    assert_eq!(user_version(&connection), 13);
    let games: i64 = connection
        .query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))
        .expect("game count");
    let covers: i64 = connection
        .query_row("SELECT COUNT(*) FROM game_covers", [], |row| row.get(0))
        .expect("cover count");
    assert_eq!((games, covers), (2, 2));
}

#[test]
fn apply_v14_refuses_install_key_collisions_with_different_game_identity() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial migration");
    reduce_current_to_v14(&connection);
    connection
        .execute_batch(
            "
            DROP INDEX uq_games_install_key;
            INSERT INTO games (
                id, title, launcher, external_id, platform, runtime, install_path,
                install_key, root_authority, executable_candidates_json,
                created_at, updated_at
            ) VALUES
                ('manual:keeper', 'Example', 'Manual', NULL, 'Windows', 'NativeWindows',
                 'C:/Games/Exact', 'c:/games/exact', 'legacy', '[]', 1, 1),
                ('steam:42', 'Example', 'Steam', '42', 'Windows', 'NativeWindows',
                 'c:/games/exact/', 'c:/games/exact', 'launcher_manifest',
                 '[\"bin/game.exe\"]', 2, 2);
            PRAGMA user_version = 13;
            ",
        )
        .expect("v13 identity collision fixture");

    let error = apply(&mut connection).expect_err("different game identities must not be merged");

    assert!(error.message().contains("game identity"));
    assert_eq!(user_version(&connection), 13);
    let games: i64 = connection
        .query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))
        .expect("game count");
    assert_eq!(games, 2);
}

#[test]
fn apply_normalizes_the_misstamped_development_v14_without_losing_rows() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial migration");
    connection
        .execute_batch(
            "
            INSERT INTO settings (key, value) VALUES ('v14_marker', 'preserved');
            PRAGMA user_version = 14;
            ",
        )
        .expect("stamp development version");

    apply(&mut connection).expect("development version normalization");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    let marker: String = connection
        .query_row(
            "SELECT value FROM settings WHERE key = 'v14_marker'",
            [],
            |row| row.get(0),
        )
        .expect("preserved marker");
    assert_eq!(marker, "preserved");
}

#[test]
fn apply_migrates_v14_technology_columns_without_losing_aggregate_state() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("v15 baseline");
    seed_v14_migration_aggregate(&connection);
    reduce_current_to_v14(&connection);

    apply(&mut connection).expect("v14 should migrate in place");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert!(table_has_column(&connection, "components", "technology"));
    assert!(table_has_column(
        &connection,
        "library_artifacts",
        "technology"
    ));
    assert!(!table_has_column(&connection, "components", "library"));
    assert!(!table_has_column(
        &connection,
        "library_artifacts",
        "library"
    ));
    assert!(schema_object_exists(
        &connection,
        "index",
        "idx_components_game_id_technology"
    ));
    assert!(schema_object_exists(
        &connection,
        "trigger",
        "trg_operation_items_artifact_technology_insert"
    ));
    assert!(!schema_object_exists(
        &connection,
        "trigger",
        "trg_operation_items_artifact_library_insert"
    ));

    let component: (String, String, i64, i64) = connection
        .query_row(
            "SELECT technology, files_json, created_at, updated_at
             FROM components WHERE id = 'component:v12'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("migrated component");
    assert_eq!(component, ("openvr".to_owned(), "[]".to_owned(), 100, 101));
    let artifact: (String, String, String) = connection
        .query_row(
            "SELECT technology, files_json, metadata_json
             FROM library_artifacts WHERE id = 'artifact:v12'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("migrated artifact");
    assert_eq!(
        artifact,
        (
            "openvr".to_owned(),
            "[{\"sentinel\":true}]".to_owned(),
            "{\"receipt\":\"preserved\"}".to_owned()
        )
    );
    for (table, expected) in [
        ("component_backups", 1_i64),
        ("operations", 1),
        ("operation_items", 1),
        ("pending_file_mutations", 1),
    ] {
        let count: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("preserved row count");
        assert_eq!(count, expected, "{table}");
    }

    connection
        .execute(
            "INSERT INTO library_artifacts
                (id, technology, file_name, files_json, metadata_json, trust_level)
             VALUES
                ('artifact:mismatch', 'intel_xess', 'mismatch.dll', '[{}]', '{}', 'user_imported')",
            [],
        )
        .expect("mismatched artifact");
    let error = connection
        .execute(
            "INSERT INTO operation_items
                (operation_id, game_id, component_id, artifact_id, source_path, status)
             VALUES
                ('operation:v12', 'game:v12', 'component:v12',
                 'artifact:mismatch', 'C:/mismatch.dll', 'pending')",
            [],
        )
        .expect_err("technology trigger must reject mismatch");
    assert!(error.to_string().contains("artifact technology mismatch"));
}
