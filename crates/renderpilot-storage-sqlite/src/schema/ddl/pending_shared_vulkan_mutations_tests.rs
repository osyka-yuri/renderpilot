use rusqlite::Connection;

use super::{create_table_sql, validates, validates_observational};

const CHECKS: &[&str] = &[
    "CHECK (resource_key = 'renodx_vulkan_layer')",
    "CHECK (length(trim(id)) > 0)",
    "CHECK (instr(id, char(0)) = 0)",
    "CHECK (scope IN ('shared_only', 'game_shared'))",
    "CHECK ((scope = 'shared_only' AND game_id IS NULL)\n        OR (scope = 'game_shared' AND game_id IS NOT NULL AND length(trim(game_id)) > 0))",
    "CHECK (game_id IS NULL OR instr(game_id, char(0)) = 0)",
    "CHECK (feature <> '' AND length(trim(feature)) > 0)",
    "CHECK (instr(feature, char(0)) = 0)",
    "CHECK (state IN ('preparing', 'prepared', 'committed'))",
    "CHECK (json_valid(manifest_json))",
    "CHECK (json_type(manifest_json) = 'object')",
    "CHECK (json_valid(root_capabilities_json))",
    "CHECK (json_type(root_capabilities_json) = 'object')",
    "CHECK (created_at >= 0)",
    "CHECK (updated_at >= created_at)",
];

#[test]
fn observational_contract_rejects_each_physical_dimension() {
    let canonical = create_table_sql();
    let mutations = [
        (
            "primary key",
            "resource_key TEXT    PRIMARY KEY NOT NULL",
            "resource_key TEXT    NOT NULL",
        ),
        (
            "id unique",
            "id           TEXT    UNIQUE NOT NULL",
            "id           TEXT    NOT NULL",
        ),
        ("strict table", ") STRICT;", ");"),
        (
            "column type",
            "scope        TEXT    NOT NULL",
            "scope        INTEGER NOT NULL",
        ),
        (
            "column order",
            "scope        TEXT    NOT NULL,\n    game_id      TEXT,",
            "game_id      TEXT,\n    scope        TEXT    NOT NULL,",
        ),
        (
            "timestamp default",
            "DEFAULT (CAST(unixepoch('subsec') * 1000 AS INTEGER))",
            "DEFAULT (0)",
        ),
    ];

    for (dimension, from, to) in mutations {
        let malformed = canonical.replacen(from, to, 1);
        assert_ne!(malformed, canonical, "missing fixture marker: {dimension}");
        assert_rejected(&malformed, dimension);
    }
}

#[test]
fn observational_contract_rejects_each_check_constraint() {
    let canonical = create_table_sql();
    for check in CHECKS {
        let malformed = canonical.replacen(check, "CHECK (1)", 1);
        assert_ne!(
            malformed, canonical,
            "missing CHECK fixture marker: {check}"
        );
        assert_rejected(&malformed, check);
    }
}

#[test]
fn semantic_probe_detects_disabled_checks_and_rolls_back_every_probe_row() {
    let connection = Connection::open_in_memory().expect("in-memory catalog");
    connection
        .execute_batch(&create_table_sql())
        .expect("canonical shared Vulkan table");
    connection
        .pragma_update(None, "ignore_check_constraints", true)
        .expect("disable checks for semantic probe");

    assert!(
        validates_observational(&connection).expect("observational validation"),
        "physical DDL remains canonical"
    );
    assert!(
        !validates(&connection).expect("semantic validation"),
        "disabled CHECK enforcement must be detected"
    );
    let row_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pending_shared_vulkan_mutations",
            [],
            |row| row.get(0),
        )
        .expect("probe row count");
    assert_eq!(row_count, 0, "validation must not leave probe rows behind");
}

fn assert_rejected(sql: &str, dimension: &str) {
    let connection = Connection::open_in_memory().expect("in-memory catalog");
    connection
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("valid malformed fixture for {dimension}: {error}"));
    assert!(
        !validates_observational(&connection).expect("observational validation"),
        "malformed {dimension} was accepted"
    );
}
