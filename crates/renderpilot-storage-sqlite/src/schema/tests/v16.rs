use super::*;

#[test]
fn apply_migrates_v16_observation_authority_fail_closed_and_is_idempotent() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("current baseline");
    restore_released_v16_scan_state(&connection);
    connection
        .execute_batch(
            r#"
            INSERT INTO games
                (id, title, launcher, platform, runtime, install_path,
                 install_key, root_authority, executable_candidates_json)
            VALUES
                ('game:clean', 'Clean', 'Manual', 'Windows', 'NativeWindows',
                 'C:/Clean', 'c:/clean', 'legacy', '[]'),
                ('game:stale', 'Stale cache', 'Manual', 'Windows', 'NativeWindows',
                 'C:/Stale', 'c:/stale', 'legacy', '[]'),
                ('game:preparing', 'Preparing', 'Manual', 'Windows', 'NativeWindows',
                 'C:/Preparing', 'c:/preparing', 'legacy', '[]'),
                ('game:prepared', 'Prepared', 'Manual', 'Windows', 'NativeWindows',
                 'C:/Prepared', 'c:/prepared', 'legacy', '[]'),
                ('game:committed', 'Committed', 'Manual', 'Windows', 'NativeWindows',
                 'C:/Committed', 'c:/committed', 'legacy', '[]');
            INSERT INTO file_hash_cache
                (path, size, modified_at, sha256, version)
            VALUES
                ('C:/Stale/nvngx_dlss.dll', 4, 0,
                 '0000000000000000000000000000000000000000000000000000000000000000',
                 '3.7.20.0');
            INSERT INTO scan_source_checkpoints (source_key, fingerprint)
            VALUES ('steam:stale', 'released-v16-fingerprint');
            INSERT INTO pending_file_mutations
                (id, game_id, feature, state, manifest_json, created_at, updated_at)
            VALUES
                ('tx-preparing', 'game:preparing', 'catalog_swap', 'preparing', '{"snapshots":[]}', 10, 10),
                ('tx-prepared', 'game:prepared', 'catalog_swap', 'prepared', '{"snapshots":[]}', 20, 20),
                ('tx-committed', 'game:committed', 'catalog_swap', 'committed', '{"snapshots":[]}', 30, 30);
            "#,
        )
        .expect("v16 fixture");

    apply(&mut connection).expect("v16 to v17");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert!(!schema_object_exists(
        &connection,
        "table",
        "file_hash_cache"
    ));
    assert!(!schema_object_exists(
        &connection,
        "table",
        "scan_source_checkpoints"
    ));
    let states = connection
        .prepare(
            "SELECT game_id, readiness, authority_epoch, mutation_token
             FROM catalog_scan_authority ORDER BY game_id",
        )
        .expect("prepare authority query")
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .expect("query authority states")
        .collect::<Result<Vec<_>, _>>()
        .expect("read authority states");
    assert_eq!(
        states,
        vec![
            (
                "game:clean".to_owned(),
                "never_completed".to_owned(),
                0,
                None
            ),
            (
                "game:committed".to_owned(),
                "invalidated".to_owned(),
                1,
                Some("tx-committed".to_owned()),
            ),
            (
                "game:prepared".to_owned(),
                "invalidated".to_owned(),
                1,
                Some("tx-prepared".to_owned()),
            ),
            (
                "game:preparing".to_owned(),
                "invalidated".to_owned(),
                1,
                Some("tx-preparing".to_owned()),
            ),
            (
                "game:stale".to_owned(),
                "never_completed".to_owned(),
                0,
                None
            ),
        ],
        "no v16 scan cache can migrate to Complete"
    );
    let complete_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM catalog_scan_authority WHERE readiness = 'complete'",
            [],
            |row| row.get(0),
        )
        .expect("count complete states");
    assert_eq!(complete_count, 0);

    apply(&mut connection).expect("current restart is idempotent");
    let repeated: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM catalog_scan_authority WHERE readiness = 'invalidated'",
            [],
            |row| row.get(0),
        )
        .expect("count invalidated states");
    assert_eq!(repeated, 3);
}

#[test]
fn fresh_v15_and_migrated_v14_have_equivalent_schema_semantics() {
    let mut fresh = open_test_connection();
    apply(&mut fresh).expect("fresh v15 baseline");
    seed_v14_migration_aggregate(&fresh);

    let mut migrated = open_test_connection();
    apply(&mut migrated).expect("v15 baseline for migration fixture");
    seed_v14_migration_aggregate(&migrated);
    reduce_current_to_v14(&migrated);
    apply(&mut migrated).expect("v14 to v15 migration");

    assert_eq!(
        schema_semantic_snapshot(&migrated),
        schema_semantic_snapshot(&fresh),
        "fresh and migrated databases must agree on columns, indexes, foreign keys, and triggers"
    );
    assert_technology_trigger_rejects_mismatch(&fresh, "fresh");
    assert_technology_trigger_rejects_mismatch(&migrated, "migrated");
}

#[test]
fn apply_rolls_back_when_the_second_v15_column_rename_fails() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};

    let mut connection = open_test_connection();
    apply(&mut connection).expect("v15 baseline");
    seed_v14_migration_aggregate(&connection);
    reduce_current_to_v14(&connection);

    connection
        .authorizer(Some(|context: AuthContext<'_>| match context.action {
            AuthAction::AlterTable {
                table_name: "library_artifacts",
                ..
            } => Authorization::Deny,
            _ => Authorization::Allow,
        }))
        .expect("install migration failure hook");
    let error = apply(&mut connection).expect_err("second rename must fail");
    connection
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
        .expect("remove migration failure hook");

    assert!(error.to_string().contains("library_artifacts.library"));
    assert_eq!(user_version(&connection), 14);
    assert!(table_has_column(&connection, "components", "library"));
    assert!(table_has_column(
        &connection,
        "library_artifacts",
        "library"
    ));
    assert!(!table_has_column(&connection, "components", "technology"));
    assert!(!table_has_column(
        &connection,
        "library_artifacts",
        "technology"
    ));
    assert!(schema_object_exists(
        &connection,
        "trigger",
        "trg_operation_items_artifact_library_insert"
    ));
}

#[test]
fn apply_rolls_back_v15_migration_when_post_migration_validation_fails() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("v15 baseline");
    seed_v14_migration_aggregate(&connection);
    reduce_current_to_v14(&connection);

    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = OFF;
            INSERT INTO operation_items
                (operation_id, game_id, component_id, artifact_id,
                 source_path, target_path, status, created_at, updated_at,
                 metadata_json)
            VALUES
                ('missing-operation', 'missing-game', 'missing-component', NULL,
                 'C:/source.dll', 'C:/target.dll', 'pending', 1, 1, '{}');
            ",
        )
        .expect("inject a foreign-key violation");

    let error = apply(&mut connection).expect_err("integrity validation must fail");
    assert!(
        error.to_string().contains("foreign_key_check"),
        "unexpected error: {error}"
    );
    assert_eq!(user_version(&connection), 14);
    assert!(table_has_column(&connection, "components", "library"));
    assert!(!table_has_column(&connection, "components", "technology"));
    assert!(schema_object_exists(
        &connection,
        "trigger",
        "trg_operation_items_artifact_library_insert"
    ));
    assert!(!schema_object_exists(
        &connection,
        "trigger",
        "trg_operation_items_artifact_technology_insert"
    ));
}
