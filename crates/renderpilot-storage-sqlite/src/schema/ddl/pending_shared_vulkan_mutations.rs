//! Canonical DDL and contract validation for the singleton shared-Vulkan fence.

use renderpilot_application::AppResult;
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::storage_context;

use super::super::SHARED_VULKAN_RESOURCE_KEY;
use super::common::MS_UNIXEPOCH_DEFAULT;

const TABLE_NAME: &str = "pending_shared_vulkan_mutations";
pub(crate) const RESOURCE_KEY: &str = SHARED_VULKAN_RESOURCE_KEY;

/// Full CREATE for greenfield catalogs and the v17→v18 additive migration.
pub(crate) fn create_table_sql() -> String {
    format!(
        r#"
CREATE TABLE IF NOT EXISTS {TABLE_NAME} (
    resource_key TEXT    PRIMARY KEY NOT NULL,
    id           TEXT    UNIQUE NOT NULL,
    scope        TEXT    NOT NULL,
    game_id      TEXT,
    feature      TEXT    NOT NULL,
    state        TEXT    NOT NULL,
    manifest_json TEXT   NOT NULL,
    root_capabilities_json TEXT NOT NULL,
    created_at   INTEGER NOT NULL DEFAULT ({default}),
    updated_at   INTEGER NOT NULL DEFAULT ({default}),

    CHECK (resource_key = '{resource_key}'),
    CHECK (length(trim(id)) > 0),
    CHECK (instr(id, char(0)) = 0),
    CHECK (scope IN ('shared_only', 'game_shared')),
    CHECK ((scope = 'shared_only' AND game_id IS NULL)
        OR (scope = 'game_shared' AND game_id IS NOT NULL AND length(trim(game_id)) > 0)),
    CHECK (game_id IS NULL OR instr(game_id, char(0)) = 0),
    CHECK (feature <> '' AND length(trim(feature)) > 0),
    CHECK (instr(feature, char(0)) = 0),
    CHECK (state IN ('preparing', 'prepared', 'committed')),
    CHECK (json_valid(manifest_json)),
    CHECK (json_type(manifest_json) = 'object'),
    CHECK (json_valid(root_capabilities_json)),
    CHECK (json_type(root_capabilities_json) = 'object'),
    CHECK (created_at >= 0),
    CHECK (updated_at >= created_at)
) STRICT;
"#,
        default = MS_UNIXEPOCH_DEFAULT,
        resource_key = RESOURCE_KEY,
    )
}

#[derive(Clone, Copy)]
struct ColumnContract {
    name: &'static str,
    type_name: &'static str,
    not_null: i64,
    default: Option<&'static str>,
    primary_key: i64,
    hidden: i64,
}

const MILLIS_DEFAULT: &str = "CAST(unixepoch('subsec') * 1000 AS INTEGER)";
const COLUMNS: &[ColumnContract] = &[
    ColumnContract {
        name: "resource_key",
        type_name: "TEXT",
        not_null: 1,
        default: None,
        primary_key: 1,
        hidden: 0,
    },
    ColumnContract {
        name: "id",
        type_name: "TEXT",
        not_null: 1,
        default: None,
        primary_key: 0,
        hidden: 0,
    },
    ColumnContract {
        name: "scope",
        type_name: "TEXT",
        not_null: 1,
        default: None,
        primary_key: 0,
        hidden: 0,
    },
    ColumnContract {
        name: "game_id",
        type_name: "TEXT",
        not_null: 0,
        default: None,
        primary_key: 0,
        hidden: 0,
    },
    ColumnContract {
        name: "feature",
        type_name: "TEXT",
        not_null: 1,
        default: None,
        primary_key: 0,
        hidden: 0,
    },
    ColumnContract {
        name: "state",
        type_name: "TEXT",
        not_null: 1,
        default: None,
        primary_key: 0,
        hidden: 0,
    },
    ColumnContract {
        name: "manifest_json",
        type_name: "TEXT",
        not_null: 1,
        default: None,
        primary_key: 0,
        hidden: 0,
    },
    ColumnContract {
        name: "root_capabilities_json",
        type_name: "TEXT",
        not_null: 1,
        default: None,
        primary_key: 0,
        hidden: 0,
    },
    ColumnContract {
        name: "created_at",
        type_name: "INTEGER",
        not_null: 1,
        default: Some(MILLIS_DEFAULT),
        primary_key: 0,
        hidden: 0,
    },
    ColumnContract {
        name: "updated_at",
        type_name: "INTEGER",
        not_null: 1,
        default: Some(MILLIS_DEFAULT),
        primary_key: 0,
        hidden: 0,
    },
];

#[derive(Clone, Copy)]
struct ProbeRow<'a> {
    id: &'a str,
    resource_key: &'a str,
    scope: &'a str,
    game_id: Option<&'a str>,
    feature: &'a str,
    state: &'a str,
    manifest_json: &'a str,
    root_capabilities_json: &'a str,
    created_at: i64,
    updated_at: i64,
}

/// Probe semantic constraints using one rollback-only savepoint. A valid row
/// and one independent invalid row for every canonical CHECK are exercised;
/// no cleanup DELETE is needed because the savepoint is always rolled back.
pub(in crate::schema) fn validates(connection: &Connection) -> AppResult<bool> {
    if !validates_observational(connection)? {
        return Ok(false);
    }
    connection
        .execute_batch("SAVEPOINT probe_pending_shared_vulkan")
        .map_err(|error| storage_context("could not open shared Vulkan schema probe", error))?;

    let valid_row = ProbeRow {
        id: "__schema_probe_valid__",
        resource_key: RESOURCE_KEY,
        scope: "shared_only",
        game_id: None,
        feature: "schema_probe",
        state: "preparing",
        manifest_json: "{}",
        root_capabilities_json: "{}",
        created_at: 10,
        updated_at: 10,
    };
    let valid = insert_probe_row(connection, &valid_row).is_ok();

    // Remove the valid singleton before probing invalid rows. Otherwise its
    // primary key would reject every canonical-resource probe before the
    // CHECK under test is evaluated, producing a false-positive validation.
    connection
        .execute_batch("ROLLBACK TO probe_pending_shared_vulkan")
        .map_err(|error| {
            storage_context(
                "could not reset shared Vulkan schema probe after valid row",
                error,
            )
        })?;

    let invalid_rows = [
        ProbeRow {
            id: "__schema_probe_resource__",
            resource_key: "other_shared_resource",
            ..valid_row
        },
        ProbeRow {
            id: "",
            ..valid_row
        },
        ProbeRow {
            id: "   ",
            ..valid_row
        },
        ProbeRow {
            id: "\0",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_scope__",
            scope: "other",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_scope_owner__",
            scope: "game_shared",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_game_owner__",
            game_id: Some("probe:game"),
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_game_blank__",
            scope: "game_shared",
            game_id: Some("   "),
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_game_nul__",
            scope: "game_shared",
            game_id: Some("\0"),
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_feature_empty__",
            feature: "",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_feature_blank__",
            feature: "   ",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_feature_nul__",
            feature: "schema\0probe",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_state__",
            state: "unknown",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_manifest_json__",
            manifest_json: "not-json",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_manifest_type__",
            manifest_json: "[]",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_root_json__",
            root_capabilities_json: "not-json",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_root_type__",
            root_capabilities_json: "[]",
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_created_at__",
            created_at: -1,
            ..valid_row
        },
        ProbeRow {
            id: "__schema_probe_updated_at__",
            updated_at: 9,
            ..valid_row
        },
    ];
    let invalid = invalid_rows
        .iter()
        .all(|row| insert_probe_row(connection, row).is_err());
    connection
        .execute_batch("ROLLBACK TO probe_pending_shared_vulkan")
        .map_err(|error| {
            storage_context("could not roll back shared Vulkan schema probe", error)
        })?;
    connection
        .execute_batch("RELEASE probe_pending_shared_vulkan")
        .map_err(|error| storage_context("could not release shared Vulkan schema probe", error))?;
    Ok(valid && invalid)
}

fn insert_probe_row(connection: &Connection, row: &ProbeRow<'_>) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO pending_shared_vulkan_mutations
            (resource_key, id, scope, game_id, feature, state, manifest_json,
             root_capabilities_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            row.resource_key,
            row.id,
            row.scope,
            row.game_id,
            row.feature,
            row.state,
            row.manifest_json,
            row.root_capabilities_json,
            row.created_at,
            row.updated_at
        ],
    )
}

/// Read-only validation for healthy CURRENT catalogs. It uses only
/// `pragma_*` virtual tables and `sqlite_master`; unlike [`validates`], it
/// never opens a savepoint or attempts an INSERT.
pub(in crate::schema) fn validates_observational(connection: &Connection) -> AppResult<bool> {
    let Some(sql) = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [TABLE_NAME],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| storage_context("could not read shared Vulkan mutation DDL", error))?
        .flatten()
    else {
        return Ok(false);
    };
    if normalize_table_sql(&sql) != normalize_table_sql(&create_table_sql()) {
        return Ok(false);
    }
    let strict: i64 = connection
        .query_row(
            "SELECT strict FROM pragma_table_list WHERE name = ?1",
            [TABLE_NAME],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| storage_context("could not read shared Vulkan strict flag", error))?
        .unwrap_or(0);
    let columns = columns_match(connection)?;
    let unique = has_single_column_id_unique(connection)?;
    if strict != 1 || !columns || !unique {
        return Ok(false);
    }
    Ok(true)
}

fn columns_match(connection: &Connection) -> AppResult<bool> {
    let mut statement = connection.prepare("SELECT cid, name, type, \"notnull\", dflt_value, pk, hidden FROM pragma_table_xinfo(?1) ORDER BY cid").map_err(|error| storage_context("could not prepare shared Vulkan column contract", error))?;
    let rows = statement
        .query_map([TABLE_NAME], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .map_err(|error| storage_context("could not query shared Vulkan column contract", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| storage_context("could not read shared Vulkan column contract", error))?;
    if rows.len() != COLUMNS.len() {
        return Ok(false);
    }
    let result = rows
        .iter()
        .zip(COLUMNS)
        .enumerate()
        .all(|(cid, (row, expected))| {
            let (actual_cid, name, type_name, not_null, default, primary_key, hidden) = row;
            *actual_cid == cid as i64
                && name == expected.name
                && type_name == expected.type_name
                && *not_null == expected.not_null
                && default.as_deref().map(normalize_sql) == expected.default.map(normalize_sql)
                && *primary_key == expected.primary_key
                && *hidden == expected.hidden
        });
    Ok(result)
}

fn has_single_column_id_unique(connection: &Connection) -> AppResult<bool> {
    let mut statement = connection
        .prepare("SELECT name FROM pragma_index_list(?1) WHERE \"unique\" = 1 AND partial = 0")
        .map_err(|error| {
            storage_context("could not prepare shared Vulkan unique indexes", error)
        })?;
    let indexes = statement
        .query_map([TABLE_NAME], |row| row.get::<_, String>(0))
        .map_err(|error| storage_context("could not query shared Vulkan unique indexes", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| storage_context("could not read shared Vulkan unique indexes", error))?;
    let mut id_indexes = 0;
    for index in indexes {
        let mut columns = connection
            .prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")
            .map_err(|error| {
                storage_context("could not prepare shared Vulkan index columns", error)
            })?;
        let names = columns
            .query_map([index], |row| row.get::<_, Option<String>>(0))
            .map_err(|error| storage_context("could not query shared Vulkan index columns", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                storage_context("could not read shared Vulkan index columns", error)
            })?;
        if names.len() == 1 && names[0].as_deref() == Some("id") {
            id_indexes += 1;
        }
    }
    Ok(id_indexes == 1)
}

fn normalize_sql(sql: &str) -> String {
    sql.chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_table_sql(sql: &str) -> String {
    normalize_sql(sql)
        .replacen("createtableifnotexists", "createtable", 1)
        .trim_end_matches(';')
        .to_owned()
}

#[cfg(test)]
#[path = "pending_shared_vulkan_mutations_tests.rs"]
mod tests;
