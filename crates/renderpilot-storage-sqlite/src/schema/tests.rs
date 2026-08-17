use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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

const RELEASED_V16_SCAN_SCHEMA: &str =
    include_str!("../../tests/fixtures/catalog-v16-scan-state.sql");

fn restore_released_v16_scan_state(connection: &Connection) {
    connection
        .execute_batch(RELEASED_V16_SCAN_SCHEMA)
        .expect("restore released v16 scan state");
    connection
        .pragma_update(None, "user_version", 16)
        .expect("stamp v16 schema version");
}

fn reduce_current_to_v14(connection: &Connection) {
    restore_released_v16_scan_state(connection);
    connection
        .execute_batch(REDUCE_TECHNOLOGY_COLUMNS_TO_V14)
        .expect("reduce physical schema to v14");
}

mod baseline;
mod legacy;
mod schema_contract;
mod v14;
mod v16;

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

fn install_observational_authorizer(connection: &Connection) -> Arc<AtomicU64> {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};

    let denied = Arc::new(AtomicU64::new(0));
    let denied_actions = Arc::clone(&denied);
    connection
        .authorizer(Some(move |context: AuthContext<'_>| match context.action {
            AuthAction::Read { .. }
            | AuthAction::Select
            | AuthAction::Pragma {
                pragma_value: None, ..
            }
            | AuthAction::Pragma {
                pragma_name:
                    "table_info" | "table_xinfo" | "table_list" | "index_info" | "index_xinfo"
                    | "index_list" | "foreign_key_list" | "foreign_key_check" | "integrity_check",
                ..
            }
            | AuthAction::Function { .. } => Authorization::Allow,
            _ => {
                denied_actions.fetch_add(1, Ordering::Relaxed);
                Authorization::Deny
            }
        }))
        .expect("install observational SQLite authorizer");

    denied
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
