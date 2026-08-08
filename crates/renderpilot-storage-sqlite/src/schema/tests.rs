use std::fs;
use std::path::PathBuf;

use rusqlite::Connection;

use super::contract::{
    CONSOLIDATION_POLICIES, CONTRACT_TABLES, REQUIRED_INDEXES, REQUIRED_TABLES, REQUIRED_TRIGGERS,
};
use super::physical;
use super::{CURRENT_SCHEMA_VERSION, apply, ddl};

const LEGACY_PENDING_WITHOUT_PREPARING: &str = r#"
DROP TABLE IF EXISTS pending_file_mutations;
CREATE TABLE pending_file_mutations (
    id             TEXT    PRIMARY KEY NOT NULL,
    game_id        TEXT    NOT NULL,
    feature        TEXT    NOT NULL,
    subject_id     TEXT,
    state          TEXT    NOT NULL,
    manifest_json  TEXT    NOT NULL,
    created_at     INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    updated_at     INTEGER NOT NULL DEFAULT (
        CAST(unixepoch('subsec') * 1000 AS INTEGER)
    ),
    CHECK (length(trim(id)) > 0),
    CHECK (length(trim(game_id)) > 0),
    CHECK (length(trim(feature)) > 0),
    CHECK (subject_id IS NULL OR length(trim(subject_id)) > 0),
    CHECK (state IN ('prepared', 'committed')),
    CHECK (json_valid(manifest_json)),
    CHECK (json_type(manifest_json) = 'object'),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at)
) STRICT;
CREATE INDEX idx_pending_file_mutations_game_id
    ON pending_file_mutations(game_id);
"#;

const REDUCE_INSTALLED_ADDONS_TO_V9: &str = "
DROP TABLE pending_file_mutations;
DROP TRIGGER trg_installed_addons_touch_updated_at;
CREATE TABLE installed_addons_v9 AS
SELECT game_id, kind, addon_file, addon_version,
       created_files_json, backed_up_files_json, tracked_sources_json,
       host_kind, reshade_channel, registered_exe_path,
       created_at, updated_at
  FROM installed_addons;
DROP TABLE installed_addons;
ALTER TABLE installed_addons_v9 RENAME TO installed_addons;
CREATE TRIGGER trg_installed_addons_touch_updated_at
AFTER UPDATE ON installed_addons
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE installed_addons
       SET updated_at = max(
           CAST(unixepoch('subsec') * 1000 AS INTEGER),
           OLD.updated_at + 1
       )
     WHERE game_id = NEW.game_id;
END;
PRAGMA user_version = 9;
";

const REDUCE_TECHNOLOGY_COLUMNS_TO_V14: &str = "
DROP TRIGGER trg_operation_items_artifact_technology_insert;
DROP TRIGGER trg_operation_items_artifact_technology_update;
DROP INDEX idx_components_game_id_technology;
DROP INDEX idx_components_technology;
DROP INDEX idx_library_artifacts_technology;
ALTER TABLE components RENAME COLUMN technology TO library;
ALTER TABLE library_artifacts RENAME COLUMN technology TO library;
CREATE INDEX idx_components_game_id_library ON components(game_id, library);
CREATE INDEX idx_components_library ON components(library);
CREATE INDEX idx_library_artifacts_library ON library_artifacts(library);
CREATE TRIGGER trg_operation_items_artifact_library_insert
BEFORE INSERT ON operation_items
FOR EACH ROW
WHEN NEW.artifact_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'operation_items artifact library mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM components AS c
        JOIN library_artifacts AS a
          ON a.id = NEW.artifact_id
         AND a.library = c.library
        WHERE c.id = NEW.component_id
          AND c.game_id = NEW.game_id
    );
END;
CREATE TRIGGER trg_operation_items_artifact_library_update
BEFORE UPDATE OF game_id, component_id, artifact_id ON operation_items
FOR EACH ROW
WHEN NEW.artifact_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'operation_items artifact library mismatch')
    WHERE NOT EXISTS (
        SELECT 1
        FROM components AS c
        JOIN library_artifacts AS a
          ON a.id = NEW.artifact_id
         AND a.library = c.library
        WHERE c.id = NEW.component_id
          AND c.game_id = NEW.game_id
    );
END;
PRAGMA user_version = 14;
";

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
fn apply_keep_preserves_catalog_rows_on_healthy_current() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial");
    connection
        .execute(
            "INSERT INTO settings (key, value) VALUES ('keep_marker', 'alive')",
            [],
        )
        .expect("marker");

    apply(&mut connection).expect("healthy keep must not wipe");

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
    connection
        .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
        .expect("restore pre-v15 technology columns");
    connection
        .execute_batch(
            "
            INSERT INTO settings (key, value) VALUES ('v12_marker', 'preserved');
            DROP TABLE profile_addon_capabilities;
            DROP TABLE scan_source_checkpoints;
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
    assert!(schema_object_exists(
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

#[test]
fn apply_v14_consolidates_only_exact_install_key_duplicates() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial migration");
    connection
        .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
        .expect("restore v14 library columns");
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
    connection
        .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
        .expect("restore v14 library columns");
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
    connection
        .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
        .expect("restore v14 library columns");
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
    connection
        .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
        .expect("reduce physical schema to v14");

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

#[test]
fn fresh_v15_and_migrated_v14_have_equivalent_schema_semantics() {
    let mut fresh = open_test_connection();
    apply(&mut fresh).expect("fresh v15 baseline");
    seed_v14_migration_aggregate(&fresh);

    let mut migrated = open_test_connection();
    apply(&mut migrated).expect("v15 baseline for migration fixture");
    seed_v14_migration_aggregate(&migrated);
    migrated
        .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
        .expect("reduce physical schema to v14");
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
    connection
        .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
        .expect("reduce physical schema to v14");

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
    connection
        .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
        .expect("reduce physical schema to v14");

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

    super::steps::run_v10_to_v11_for_test(&connection).expect("v10 step should migrate additively");
    super::steps::run_v10_to_v11_for_test(&connection).expect("v10 step should be idempotent");

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

    super::steps::run_v10_to_v11_for_test(&connection).expect("v10 step should migrate additively");
    super::steps::run_v10_to_v11_for_test(&connection).expect("v10 step should be idempotent");

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

    super::steps::run_v10_to_v11_for_test(&connection).expect("v10 step should migrate additively");
    super::steps::run_v10_to_v11_for_test(&connection).expect("v10 step should be idempotent");

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
fn apply_heals_v10_pending_mutations_that_reject_preparing_without_wipe() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("initial migration should succeed");
    connection
        .execute(
            "
            INSERT INTO settings (key, value)
            VALUES ('keep_me', 'alive')
            ",
            [],
        )
        .expect("marker setting should insert");
    connection
        .execute_batch(LEGACY_PENDING_WITHOUT_PREPARING)
        .expect("legacy WIP pending_file_mutations shape");
    connection
        .execute_batch(
            r#"
            INSERT INTO pending_file_mutations
                (id, game_id, feature, subject_id, state, manifest_json)
            VALUES
                ('tx-legacy', 'steam:1', 'catalog_swap', NULL, 'prepared',
                 '{"format_version":1,"roots":[],"transaction_dir":"x","snapshots":[]}');
            PRAGMA user_version = 10;
            "#,
        )
        .expect("seed legacy prepared row");

    apply(&mut connection).expect("current schema should soft-heal pending table");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert_preparing_state_is_accepted(&connection);
    let kept: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pending_file_mutations WHERE id = 'tx-legacy'",
            [],
            |row| row.get(0),
        )
        .expect("legacy prepared row");
    assert_eq!(kept, 1);
    let marker: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'keep_me'",
            [],
            |row| row.get(0),
        )
        .expect("catalog rows must survive soft-heal");
    assert_eq!(marker, 1);
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
        super::validation::physical_column_mismatches(&connection)
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
        !super::validation::physical_column_mismatches(&connection)
            .expect("physical-column validation should query")
            .is_empty()
    );
    let error = super::validation::validate_catalog_schema(&connection)
        .expect_err("unexpected physical column should invalidate schema");
    assert!(
        error
            .message()
            .contains("unexpected column games.unexpected_column")
    );

    super::apply(&mut connection).expect("unexpected physical column should rebuild schema");

    assert!(
        super::validation::physical_column_mismatches(&connection)
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

    assert!(!super::validation::catalog_schema_is_valid(&connection).expect("validate"));
    let error = super::validation::validate_catalog_schema(&connection).expect_err("must fail");
    assert!(error.message().contains("preparing"));
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
fn apply_backs_up_file_database_before_upgrade_that_requires_rebuild() {
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
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".pre-migration-v16.") && name.ends_with(".bak"))
        })
        .collect();
    assert_eq!(
        backups.len(),
        1,
        "expected one pre-migration backup before the transactional rebuild"
    );

    let backup = Connection::open(&backups[0]).expect("open backup");
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
        connection
            .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
            .expect("reduce physical schema to v14");
        apply(&mut connection).expect("migrate file database");
        assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
        ddl::portable_path_tags::validate(&connection).expect("canonical v16 path tags");
    }

    let backups: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("list temp")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(".pre-migration-v16.") && name.ends_with(".bak"))
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

fn seed_v14_migration_aggregate(connection: &Connection) {
    connection
        .execute_batch(
            r#"
            INSERT INTO games
                (id, title, launcher, platform, runtime, install_path,
                 install_key, root_authority, executable_candidates_json)
            VALUES
                ('game:v12', 'Migration Fixture', 'Manual', 'Windows',
                 'NativeWindows', 'C:/Game', 'c:/game', 'legacy', '[]');
            INSERT INTO components
                (id, game_id, kind, technology, swappability, files_json,
                 created_at, updated_at)
            VALUES
                ('component:v12', 'game:v12', 'NativeLibrary', 'openvr',
                 'Swappable', '[]', 100, 101);
            INSERT INTO library_artifacts
                (id, technology, file_name, files_json, metadata_json,
                 source, trust_level, created_at, updated_at)
            VALUES
                ('artifact:v12', 'openvr', 'openvr_api.dll',
                 '[{"sentinel":true}]', '{"receipt":"preserved"}',
                 'fixture', 'user_imported', 102, 103);
            INSERT INTO component_backups
                (component_id, game_id, files_json, auxiliary_json,
                 created_at, updated_at)
            VALUES
                ('component:v12', 'game:v12', '[]', '[]', 104, 105);
            INSERT INTO operations
                (id, game_id, kind, status, created_at, updated_at, metadata_json)
            VALUES
                ('operation:v12', 'game:v12', 'replace_component', 'pending',
                 106, 107, '{"sentinel":true}');
            INSERT INTO operation_items
                (operation_id, game_id, component_id, artifact_id,
                 source_path, target_path, status, created_at, updated_at,
                 metadata_json)
            VALUES
                ('operation:v12', 'game:v12', 'component:v12',
                 'artifact:v12', 'C:/source.dll', 'C:/target.dll', 'pending',
                 108, 109, '{"sentinel":true}');
            INSERT INTO pending_file_mutations
                (id, game_id, feature, subject_id, state, manifest_json,
                 created_at, updated_at)
            VALUES
                ('mutation:v12', 'game:v12', 'catalog_swap', 'component:v12',
                 'prepared', '{"sentinel":true}', 110, 111);
            "#,
        )
        .expect("seed migration aggregate");
}

#[derive(Debug, PartialEq, Eq)]
struct SchemaSemanticSnapshot {
    table_xinfo: Vec<(String, Vec<String>)>,
    index_xinfo: Vec<(String, Vec<String>)>,
    foreign_keys: Vec<(String, Vec<String>)>,
    triggers: Vec<String>,
}

fn schema_semantic_snapshot(connection: &Connection) -> SchemaSemanticSnapshot {
    let table_xinfo = REQUIRED_TABLES
        .iter()
        .map(|table| ((*table).to_owned(), table_xinfo_rows(connection, table)))
        .collect();
    let index_xinfo = REQUIRED_INDEXES
        .iter()
        .map(|index| ((*index).to_owned(), index_xinfo_rows(connection, index)))
        .collect();
    let foreign_keys = REQUIRED_TABLES
        .iter()
        .map(|table| ((*table).to_owned(), foreign_key_rows(connection, table)))
        .collect();
    let mut triggers = REQUIRED_TRIGGERS
        .iter()
        .filter(|trigger| schema_object_exists(connection, "trigger", trigger))
        .map(|trigger| (*trigger).to_owned())
        .collect::<Vec<_>>();
    triggers.sort();

    SchemaSemanticSnapshot {
        table_xinfo,
        index_xinfo,
        foreign_keys,
        triggers,
    }
}

fn table_xinfo_rows(connection: &Connection, table: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(
            "SELECT cid, name, type, \"notnull\", dflt_value, pk, hidden
             FROM pragma_table_xinfo(?1)
             ORDER BY cid",
        )
        .expect("prepare table_xinfo");
    statement
        .query_map([table], |row| {
            Ok(format!(
                "{}|{}|{}|{}|{:?}|{}|{}",
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .expect("query table_xinfo")
        .collect::<Result<Vec<_>, _>>()
        .expect("read table_xinfo")
}

fn index_xinfo_rows(connection: &Connection, index: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(
            "SELECT seqno, cid, name, \"desc\", coll, key
             FROM pragma_index_xinfo(?1)
             ORDER BY seqno",
        )
        .expect("prepare index_xinfo");
    statement
        .query_map([index], |row| {
            Ok(format!(
                "{}|{}|{:?}|{}|{:?}|{}",
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .expect("query index_xinfo")
        .collect::<Result<Vec<_>, _>>()
        .expect("read index_xinfo")
}

fn foreign_key_rows(connection: &Connection, table: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(
            "SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete, \"match\"
             FROM pragma_foreign_key_list(?1)
             ORDER BY id, seq",
        )
        .expect("prepare foreign_key_list");
    statement
        .query_map([table], |row| {
            Ok(format!(
                "{}|{}|{}|{}|{:?}|{}|{}|{}",
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .expect("query foreign_key_list")
        .collect::<Result<Vec<_>, _>>()
        .expect("read foreign_key_list")
}

fn assert_technology_trigger_rejects_mismatch(connection: &Connection, suffix: &str) {
    let artifact_id = format!("artifact:mismatch:{suffix}");
    connection
        .execute(
            "INSERT INTO library_artifacts
                (id, technology, file_name, files_json, metadata_json, trust_level)
             VALUES
                (?1, 'intel_xess', 'mismatch.dll', '[{}]', '{}', 'user_imported')",
            [&artifact_id],
        )
        .expect("mismatched artifact");
    let error = connection
        .execute(
            "INSERT INTO operation_items
                (operation_id, game_id, component_id, artifact_id, source_path, status)
             VALUES
                ('operation:v12', 'game:v12', 'component:v12', ?1, 'C:/mismatch.dll', 'pending')",
            [&artifact_id],
        )
        .expect_err("technology trigger must reject mismatch");
    assert!(error.to_string().contains("artifact technology mismatch"));
}

fn open_test_connection() -> Connection {
    Connection::open_in_memory().expect("sqlite should open")
}

fn assert_catalog_schema_exists(connection: &Connection) {
    for &table in REQUIRED_TABLES {
        assert!(
            schema_object_exists(connection, "table", table),
            "missing table {table}"
        );
        assert!(
            table_has_columns(connection, table),
            "table {table} has no columns"
        );
    }
    for &index in REQUIRED_INDEXES {
        assert!(
            schema_object_exists(connection, "index", index),
            "missing index {index}"
        );
    }
    for &trigger in REQUIRED_TRIGGERS {
        assert!(
            schema_object_exists(connection, "trigger", trigger),
            "missing trigger {trigger}"
        );
    }
}

fn assert_preparing_state_is_accepted(connection: &Connection) {
    connection
        .execute(
            "
            INSERT INTO pending_file_mutations
                (id, game_id, feature, subject_id, state, manifest_json)
            VALUES
                ('tx-preparing-assert', 'steam:probe', 'schema_probe', NULL, 'preparing',
                 '{\"snapshots\":[]}')
            ",
            [],
        )
        .expect("preparing state must be accepted");
    connection
        .execute(
            "DELETE FROM pending_file_mutations WHERE id = 'tx-preparing-assert'",
            [],
        )
        .expect("cleanup probe row");
}

fn user_version(connection: &Connection) -> i32 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("schema version should be readable")
}

fn foreign_keys_enabled(connection: &Connection) -> bool {
    let enabled: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("foreign_keys pragma should be readable");

    enabled != 0
}

fn table_has_columns(connection: &Connection, table_name: &str) -> bool {
    let sql = format!(
        "SELECT COUNT(*) FROM pragma_table_info({})",
        quote_sql_literal(table_name)
    );

    let column_count: i64 = connection
        .query_row(&sql, [], |row| row.get(0))
        .expect("table info should be readable");

    column_count > 0
}

fn table_has_column(connection: &Connection, table_name: &str, column_name: &str) -> bool {
    let sql = format!(
        "SELECT COUNT(*) FROM pragma_table_info({}) WHERE name = {}",
        quote_sql_literal(table_name),
        quote_sql_literal(column_name)
    );

    let column_count: i64 = connection
        .query_row(&sql, [], |row| row.get(0))
        .expect("table info should be readable");

    column_count > 0
}

fn schema_object_exists(connection: &Connection, object_type: &str, object_name: &str) -> bool {
    let object_count: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type = ?1
              AND name = ?2
            ",
            [object_type, object_name],
            |row| row.get(0),
        )
        .expect("schema object should be queryable");

    object_count == 1
}

fn quote_sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
