//! Generalize persisted library classifications from `library` to `technology`.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

use super::super::version;
use super::util::table_has_column;

pub(super) const SOURCE_VERSION: i32 = 14;
pub(super) const TARGET_VERSION: i32 = 15;

#[derive(Clone, Copy)]
struct ColumnRename {
    table: &'static str,
    statement: &'static str,
}

const LIBRARY_COLUMN_RENAMES: [ColumnRename; 2] = [
    ColumnRename {
        table: "components",
        statement: "ALTER TABLE components RENAME COLUMN library TO technology",
    },
    ColumnRename {
        table: "library_artifacts",
        statement: "ALTER TABLE library_artifacts RENAME COLUMN library TO technology",
    },
];

const DROP_LEGACY_OBJECTS: &str = "
    DROP TRIGGER IF EXISTS trg_operation_items_artifact_library_insert;
    DROP TRIGGER IF EXISTS trg_operation_items_artifact_library_update;
    DROP TRIGGER IF EXISTS trg_operation_items_artifact_technology_insert;
    DROP TRIGGER IF EXISTS trg_operation_items_artifact_technology_update;
    DROP INDEX IF EXISTS idx_components_game_id_library;
    DROP INDEX IF EXISTS idx_components_library;
    DROP INDEX IF EXISTS idx_library_artifacts_library;
    DROP INDEX IF EXISTS idx_components_game_id_technology;
    DROP INDEX IF EXISTS idx_components_technology;
    DROP INDEX IF EXISTS idx_library_artifacts_technology;
";

const CREATE_V15_OBJECTS: &str = "
    CREATE INDEX idx_components_game_id_technology
        ON components(game_id, technology);
    CREATE INDEX idx_components_technology
        ON components(technology);
    CREATE INDEX idx_library_artifacts_technology
        ON library_artifacts(technology);

    CREATE TRIGGER trg_operation_items_artifact_technology_insert
    BEFORE INSERT ON operation_items
    FOR EACH ROW
    WHEN NEW.artifact_id IS NOT NULL
    BEGIN
        SELECT RAISE(ABORT, 'operation_items artifact technology mismatch')
        WHERE NOT EXISTS (
            SELECT 1
            FROM components AS c
            JOIN library_artifacts AS a
              ON a.id = NEW.artifact_id
             AND a.technology = c.technology
            WHERE c.id = NEW.component_id
              AND c.game_id = NEW.game_id
        );
    END;

    CREATE TRIGGER trg_operation_items_artifact_technology_update
    BEFORE UPDATE OF game_id, component_id, artifact_id ON operation_items
    FOR EACH ROW
    WHEN NEW.artifact_id IS NOT NULL
    BEGIN
        SELECT RAISE(ABORT, 'operation_items artifact technology mismatch')
        WHERE NOT EXISTS (
            SELECT 1
            FROM components AS c
            JOIN library_artifacts AS a
              ON a.id = NEW.artifact_id
             AND a.technology = c.technology
            WHERE c.id = NEW.component_id
              AND c.game_id = NEW.game_id
        );
    END;
";

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch(DROP_LEGACY_OBJECTS)
        .map_err(|error| storage_context("could not drop v14 technology objects", error))?;

    for rename in LIBRARY_COLUMN_RENAMES {
        rename_column_if_legacy(connection, rename)?;
    }

    let components_ready = table_has_column(connection, "components", "technology")?;
    let artifacts_ready = table_has_column(connection, "library_artifacts", "technology")?;
    if components_ready && artifacts_ready {
        connection
            .execute_batch(CREATE_V15_OBJECTS)
            .map_err(|error| storage_context("could not create v15 technology objects", error))?;
    }

    version::write(connection, TARGET_VERSION)
}

fn rename_column_if_legacy(connection: &Connection, rename: ColumnRename) -> AppResult<()> {
    let has_legacy = table_has_column(connection, rename.table, "library")?;
    let has_current = table_has_column(connection, rename.table, "technology")?;
    if !has_legacy || has_current {
        return Ok(());
    }

    connection.execute_batch(rename.statement).map_err(|error| {
        storage_context(
            &format!("could not rename {}.library to technology", rename.table),
            error,
        )
    })
}
