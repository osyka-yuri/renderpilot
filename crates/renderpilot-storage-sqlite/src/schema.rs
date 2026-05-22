use renderpilot_application::AppResult;
use rusqlite::{Connection, Error as SqliteError, OptionalExtension, TransactionBehavior};

use crate::error::storage_context;

const INITIAL_MIGRATION: &str = include_str!("../migrations/0001_initial.sql");

const CURRENT_SCHEMA_VERSION: i32 = 1;

const REQUIRED_TABLES: &[&str] = &[
    "games",
    "game_covers",
    "components",
    "library_artifacts",
    "operations",
    "operation_items",
    "settings",
    "file_hash_cache",
];

const REQUIRED_INDEXES: &[&str] = &[
    "uq_games_launcher_external_id",
    "idx_components_game_id",
    "idx_library_artifacts_library",
    "idx_operations_game_id_created_at",
    "idx_operation_items_operation_id",
    "idx_settings_updated_at",
];

const REQUIRED_TRIGGERS: &[&str] = &[
    "trg_operation_items_artifact_library_insert",
    "trg_operation_items_artifact_library_update",
];

const REQUIRED_SCHEMA_OBJECT_GROUPS: &[(SchemaObjectKind, &[&str])] = &[
    (SchemaObjectKind::Table, REQUIRED_TABLES),
    (SchemaObjectKind::Index, REQUIRED_INDEXES),
    (SchemaObjectKind::Trigger, REQUIRED_TRIGGERS),
];

/// Applies the bundled catalog DDL.
///
/// If the on-disk schema version/shape is not the bundled one, the schema is
/// dropped and recreated from the bundled migration.
pub(crate) fn apply(connection: &mut Connection) -> AppResult<()> {
    let foreign_keys = ForeignKeysState::capture_and_disable(connection)?;
    let result = apply_in_transaction(connection);

    foreign_keys.restore(connection, result)
}

fn apply_in_transaction(connection: &mut Connection) -> AppResult<()> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| storage_context("could not start sqlite migration transaction", error))?;

    match determine_migration_action(&transaction)? {
        MigrationAction::Keep => {}
        MigrationAction::ApplyInitial => {
            apply_initial_migration(&transaction)?;
            set_user_version(&transaction, CURRENT_SCHEMA_VERSION)?;
            validate_catalog_schema(&transaction)?;
        }
        MigrationAction::Rebuild => {
            reset_catalog_schema(&transaction)?;
        }
    }

    transaction
        .commit()
        .map_err(|error| storage_context("could not commit sqlite migration transaction", error))?;

    Ok(())
}

fn determine_migration_action(connection: &Connection) -> AppResult<MigrationAction> {
    let schema_version = read_user_version(connection)?;

    match schema_version {
        CURRENT_SCHEMA_VERSION => {
            if catalog_schema_is_valid(connection)? {
                Ok(MigrationAction::Keep)
            } else {
                Ok(MigrationAction::Rebuild)
            }
        }
        0 => {
            if database_has_user_schema(connection)? {
                Ok(MigrationAction::Rebuild)
            } else {
                Ok(MigrationAction::ApplyInitial)
            }
        }
        _ => Ok(MigrationAction::Rebuild),
    }
}

fn reset_catalog_schema(connection: &Connection) -> AppResult<()> {
    drop_user_schema_objects(connection)?;
    apply_initial_migration(connection)?;
    set_user_version(connection, CURRENT_SCHEMA_VERSION)?;
    validate_catalog_schema(connection)
}

fn apply_initial_migration(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(INITIAL_MIGRATION)
        .map_err(|error| storage_context("could not apply sqlite initial migration", error))
}

fn read_user_version(connection: &Connection) -> AppResult<i32> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| storage_context("could not read sqlite catalog schema version", error))
}

fn set_user_version(connection: &Connection, version: i32) -> AppResult<()> {
    debug_assert!(version >= 0);

    connection
        .execute_batch(&format!("PRAGMA user_version = {version}"))
        .map_err(|error| storage_context("could not write sqlite catalog schema version", error))
}

fn database_has_user_schema(connection: &Connection) -> AppResult<bool> {
    let object_count: i64 = connection
        .query_row(
            "
            SELECT COUNT(*)
            FROM sqlite_master
            WHERE type IN ('table', 'index', 'trigger', 'view')
              AND name NOT LIKE 'sqlite_%'
            ",
            [],
            |row| row.get(0),
        )
        .map_err(|error| storage_context("could not inspect existing sqlite schema", error))?;

    Ok(object_count > 0)
}

fn catalog_schema_is_valid(connection: &Connection) -> AppResult<bool> {
    for &(object_kind, object_names) in REQUIRED_SCHEMA_OBJECT_GROUPS {
        if !required_objects_exist(connection, object_kind, object_names)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn required_objects_exist(
    connection: &Connection,
    object_kind: SchemaObjectKind,
    object_names: &[&str],
) -> AppResult<bool> {
    for &object_name in object_names {
        if !object_exists(connection, object_kind, object_name)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn validate_catalog_schema(connection: &Connection) -> AppResult<()> {
    let mut missing_objects = Vec::new();

    for &(object_kind, object_names) in REQUIRED_SCHEMA_OBJECT_GROUPS {
        collect_missing_objects(connection, object_kind, object_names, &mut missing_objects)?;
    }

    if missing_objects.is_empty() {
        return Ok(());
    }

    Err(storage_context(
        &format!(
            "sqlite catalog schema is incomplete; missing required objects: {}",
            missing_objects.join(", ")
        ),
        SqliteError::InvalidQuery,
    ))
}

fn collect_missing_objects(
    connection: &Connection,
    object_kind: SchemaObjectKind,
    object_names: &[&str],
    missing_objects: &mut Vec<String>,
) -> AppResult<()> {
    for &object_name in object_names {
        if !object_exists(connection, object_kind, object_name)? {
            missing_objects.push(format!("{} {object_name}", object_kind.sqlite_type()));
        }
    }

    Ok(())
}

fn object_exists(
    connection: &Connection,
    object_kind: SchemaObjectKind,
    object_name: &str,
) -> AppResult<bool> {
    let exists = connection
        .query_row(
            "
            SELECT 1
            FROM sqlite_master
            WHERE type = ?1
              AND name = ?2
            LIMIT 1
            ",
            [object_kind.sqlite_type(), object_name],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| storage_context("could not inspect sqlite catalog object", error))?
        .is_some();

    Ok(exists)
}

fn drop_user_schema_objects(connection: &Connection) -> AppResult<()> {
    for &object_kind in SchemaObjectKind::DROP_ORDER {
        drop_schema_objects_by_kind(connection, object_kind)?;
    }

    Ok(())
}

fn drop_schema_objects_by_kind(
    connection: &Connection,
    object_kind: SchemaObjectKind,
) -> AppResult<()> {
    let object_names = list_schema_object_names(connection, object_kind)?;

    for object_name in object_names {
        drop_schema_object(connection, object_kind, &object_name)?;
    }

    Ok(())
}

fn list_schema_object_names(
    connection: &Connection,
    object_kind: SchemaObjectKind,
) -> AppResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            "
            SELECT name
            FROM sqlite_master
            WHERE type = ?1
              AND name NOT LIKE 'sqlite_%'
            ORDER BY name
            ",
        )
        .map_err(|error| storage_context("could not enumerate sqlite schema objects", error))?;

    let rows = statement
        .query_map([object_kind.sqlite_type()], |row| row.get::<_, String>(0))
        .map_err(|error| storage_context("could not query sqlite schema objects", error))?;

    let mut object_names = Vec::new();

    for row in rows {
        object_names.push(
            row.map_err(|error| storage_context("could not parse sqlite object name", error))?,
        );
    }

    Ok(object_names)
}

fn drop_schema_object(
    connection: &Connection,
    object_kind: SchemaObjectKind,
    object_name: &str,
) -> AppResult<()> {
    let sql = format!(
        "DROP {} IF EXISTS {}",
        object_kind.drop_keyword(),
        quote_sql_identifier(object_name)
    );

    connection.execute_batch(&sql).map_err(|error| {
        storage_context(
            &format!(
                "could not drop sqlite catalog {} {object_name}",
                object_kind.sqlite_type()
            ),
            error,
        )
    })
}

fn quote_sql_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn foreign_keys_enabled(connection: &Connection) -> AppResult<bool> {
    let enabled: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|error| storage_context("could not read sqlite foreign_keys pragma", error))?;

    Ok(enabled != 0)
}

fn set_foreign_keys(connection: &Connection, enabled: bool, context: &str) -> AppResult<()> {
    let sql = if enabled {
        "PRAGMA foreign_keys = ON"
    } else {
        "PRAGMA foreign_keys = OFF"
    };

    connection
        .execute_batch(sql)
        .map_err(|error| storage_context(context, error))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MigrationAction {
    Keep,
    ApplyInitial,
    Rebuild,
}

#[derive(Clone, Copy, Debug)]
enum SchemaObjectKind {
    Trigger,
    View,
    Index,
    Table,
}

impl SchemaObjectKind {
    const DROP_ORDER: &'static [Self] = &[Self::Trigger, Self::View, Self::Index, Self::Table];

    fn sqlite_type(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::View => "view",
            Self::Index => "index",
            Self::Table => "table",
        }
    }

    fn drop_keyword(self) -> &'static str {
        match self {
            Self::Trigger => "TRIGGER",
            Self::View => "VIEW",
            Self::Index => "INDEX",
            Self::Table => "TABLE",
        }
    }
}

#[derive(Debug)]
struct ForeignKeysState {
    was_enabled: bool,
}

impl ForeignKeysState {
    fn capture_and_disable(connection: &Connection) -> AppResult<Self> {
        let was_enabled = foreign_keys_enabled(connection)?;

        set_foreign_keys(
            connection,
            false,
            "could not disable sqlite foreign_keys before schema migration",
        )?;

        Ok(Self { was_enabled })
    }

    fn restore(self, connection: &Connection, result: AppResult<()>) -> AppResult<()> {
        let restore_result = set_foreign_keys(
            connection,
            self.was_enabled,
            "could not restore sqlite foreign_keys after schema migration",
        );

        match (result, restore_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
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
}
