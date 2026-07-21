use std::fs;
use std::path::PathBuf;

use rusqlite::Connection;

use super::contract::{CONTRACT_TABLES, REQUIRED_INDEXES, REQUIRED_TABLES, REQUIRED_TRIGGERS};
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

    super::steps::run_from(&connection, 10).expect("v10 step should migrate additively");
    super::steps::run_from(&connection, 10).expect("v10 step should be idempotent");

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

    super::steps::run_from(&connection, 10).expect("v10 step should migrate additively");
    super::steps::run_from(&connection, 10).expect("v10 step should be idempotent");

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

    super::steps::run_from(&connection, 10).expect("v10 step should migrate additively");
    super::steps::run_from(&connection, 10).expect("v10 step should be idempotent");

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
fn apply_backs_up_file_database_before_rebuild() {
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
                .is_some_and(|name| name.contains(".pre-rebuild.") && name.ends_with(".bak"))
        })
        .collect();
    assert_eq!(backups.len(), 1, "expected one pre-rebuild backup");

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
