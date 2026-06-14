use rusqlite::Connection;

use super::{apply, CURRENT_SCHEMA_VERSION};

#[test]
fn apply_creates_catalog_schema() {
    let mut connection = open_test_connection();

    apply(&mut connection).expect("migration should succeed");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert_catalog_schema_exists(&connection);
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

fn open_test_connection() -> Connection {
    Connection::open_in_memory().expect("sqlite should open")
}

fn assert_catalog_schema_exists(connection: &Connection) {
    assert!(table_has_columns(connection, "games"));
    assert!(table_has_columns(connection, "game_covers"));
    assert!(schema_object_exists(
        connection,
        "index",
        "uq_games_launcher_external_id"
    ));
    assert!(schema_object_exists(
        connection,
        "trigger",
        "trg_operation_items_artifact_library_insert"
    ));
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
