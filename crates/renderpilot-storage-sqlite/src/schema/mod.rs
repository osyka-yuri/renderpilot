//! Catalog schema application: keep, initialize, or rebuild the bundled DDL.

use renderpilot_application::AppResult;
use rusqlite::{Connection, TransactionBehavior};

use crate::error::storage_context;

mod migration;
mod objects;
mod pragmas;
mod validation;

#[cfg(test)]
mod tests;

use self::migration::{
    MigrationAction, apply_initial_migration, determine_migration_action, reset_catalog_schema,
    set_user_version,
};
use self::objects::SchemaObjectKind;
use self::pragmas::ForeignKeysState;
use self::validation::validate_catalog_schema;

const INITIAL_MIGRATION: &str = include_str!("../../migrations/0001_initial.sql");

// Schema version history:
//   2 → 3: bundle-swap reshape of `library_artifacts` (files_json) + `component_backups` table.
//   4 → 8: `installed_addons` table for RenoDX install tracking, with `tracked_sources_json`
//          array (intermediate 5/6/7 iterations were never released and squashed into 8).
const CURRENT_SCHEMA_VERSION: i32 = 8;

const REQUIRED_TABLES: &[&str] = &[
    "games",
    "game_ui_state",
    "game_covers",
    "components",
    "component_backups",
    "installed_addons",
    "library_artifacts",
    "operations",
    "operation_items",
    "settings",
    "file_hash_cache",
    "nvapi_executable_overrides",
    "nvapi_setting_baselines",
];

const REQUIRED_INDEXES: &[&str] = &[
    "uq_games_launcher_external_id",
    "idx_components_game_id",
    "idx_component_backups_game_id",
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
