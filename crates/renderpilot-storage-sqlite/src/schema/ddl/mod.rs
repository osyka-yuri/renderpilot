//! Canonical DDL fragments shared by baseline composition and upgrade steps.
//!
//! Composition rule:
//! - Stable tables/indexes/triggers that never appear in upgrade steps live in
//!   `migrations/fragments/core.sql`.
//! - Fragments also needed by additive steps live as Rust modules here so baseline
//!   and upgrades share one definition.
//!
//! Runtime applies [`compose_baseline`]; there is no separate checked-in full
//! SQL snapshot of CURRENT.

pub(super) mod common;
pub(super) mod pending_file_mutations;
pub(super) mod profile_addon_capabilities;
pub(super) mod scan_source_checkpoints;
pub(super) mod shared_artifacts;

const BASELINE_HEADER: &str = r#"
PRAGMA foreign_keys = ON;

-- =============================================================================
-- RenderPilot catalog schema (CURRENT = user_version stamp after apply)
--
-- Notes:
-- - Timestamps are Unix milliseconds.
-- - Paths are normalized PathRef-style UTF-8 strings with "/" separators.
-- - JSON fields are stored as TEXT and validated with json_valid/json_type.
-- - Tables use STRICT mode.
-- - Lifecycle: greenfield applies this baseline; released versions 8+ upgrade
--   linearly via `src/schema/steps`; unknown/corrupt shapes rebuild (with
--   pre-rebuild backup for file-backed DBs). See `src/schema/mod.rs`.
-- =============================================================================
"#;

const CORE_SQL: &str = include_str!("../../../migrations/fragments/core.sql");

/// Composed CURRENT catalog DDL (idempotent CREATE IF NOT EXISTS).
pub(super) fn compose_baseline() -> String {
    let pending = pending_file_mutations::baseline_sql();
    let shared_table = shared_artifacts::create_table_sql();
    let shared_trigger = shared_artifacts::touch_trigger_sql();
    let profile_capabilities = profile_addon_capabilities::baseline_sql();
    let scan_checkpoints = scan_source_checkpoints::baseline_sql();

    let mut sql = String::with_capacity(
        BASELINE_HEADER.len()
            + CORE_SQL.len()
            + pending.len()
            + shared_table.len()
            + shared_trigger.len()
            + profile_capabilities.len()
            + scan_checkpoints.len()
            + 8,
    );
    sql.push_str(BASELINE_HEADER.trim_start());
    sql.push('\n');
    sql.push_str(CORE_SQL);
    sql.push('\n');
    sql.push_str(&pending);
    sql.push('\n');
    sql.push_str(&shared_table);
    sql.push('\n');
    sql.push_str(&shared_trigger);
    sql.push('\n');
    sql.push_str(profile_capabilities);
    sql.push('\n');
    sql.push_str(scan_checkpoints);
    sql.push('\n');
    sql
}
