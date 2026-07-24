//! Catalog schema application: keep, initialize, upgrade, or rebuild.
//!
//! Upgrade path is a linear step registry (sqlx-style discipline on rusqlite):
//! known `user_version` values run `from→to` steps until CURRENT, then validate.
//! Unknown or corrupt shapes rebuild from the composed baseline after a
//! file-backed pre-rebuild backup.
//!
//! Healthy CURRENT catalogs take a read-only keep path (validate only). Soft-heal
//! of WIP `pending_file_mutations` CHECK shapes runs only when validation fails.
//!
//! **Soft-heal exit strategy:** the CHECK reshape for state `preparing` exists
//! only to recover developer / pre-release DBs from the WIP shape. After one
//! released schema cycle past v10, treat a wrong CHECK as Rebuild-only and
//! delete the soft-heal path.

use std::collections::HashSet;

use renderpilot_application::AppResult;
use rusqlite::{Connection, TransactionBehavior};

use crate::error::storage_context;

mod backup;
mod contract;
mod ddl;
mod objects;
pub(crate) mod physical;
mod pragmas;
mod rebuild;
mod steps;
mod validation;
mod version;

#[cfg(test)]
mod tests;

use self::ddl::pending_file_mutations;
use self::pragmas::ForeignKeysState;
use self::rebuild::{apply_baseline, reset_catalog_schema};
use self::validation::{catalog_schema_is_valid, validate_catalog_schema};
use self::version::database_has_user_schema;

// Schema version history:
//   2 → 3: bundle-swap reshape of `library_artifacts` (files_json) + `component_backups` table.
//   4 → 8: `installed_addons` table for RenoDX install tracking, with `tracked_sources_json`
//          array (intermediate 5/6/7 iterations were never released and squashed into 8).
//   8 → 9: advisory shared artifact provenance + nullable RenoDX host metadata.
//   9 → 10: coordinated add-on files + crash-recoverable game-file mutations
//          (`pending_file_mutations` must accept state `preparing`).
//   10 → 11: typed artifact package/runtime metadata persisted as JSON.
//   11 → 12: typed auxiliary rollback baselines (including managed D3D12 EXEs).
const CURRENT_SCHEMA_VERSION: i32 = 12;

pub(super) fn pragma_column_names(
    connection: &Connection,
    table_name: &str,
) -> AppResult<HashSet<String>> {
    let mut statement = connection
        .prepare("SELECT name FROM pragma_table_info(?1)")
        .map_err(|error| storage_context("prepare pragma_table_info", error))?;
    let names = statement
        .query_map([table_name], |row| row.get::<_, String>(0))
        .map_err(|error| storage_context("query pragma_table_info", error))?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| storage_context("read pragma_table_info rows", error))?;
    Ok(names)
}

/// Applies the composed catalog DDL.
///
/// If the on-disk schema version/shape is not the bundled one, the schema is
/// upgraded linearly or rebuilt from the baseline (with pre-rebuild backup for
/// file-backed databases).
pub(crate) fn apply(connection: &mut Connection) -> AppResult<()> {
    let foreign_keys = ForeignKeysState::capture_and_disable(connection)?;
    let result = apply_plan(connection);

    foreign_keys.restore(connection, result)
}

fn apply_plan(connection: &mut Connection) -> AppResult<()> {
    let plan = classify(connection)?;

    match plan {
        Plan::Keep => apply_keep(connection),
        Plan::ApplyBaseline => apply_with_transaction(connection, |tx| {
            apply_baseline(tx)?;
            version::write(tx, CURRENT_SCHEMA_VERSION)?;
            validate_catalog_schema(tx)
        }),
        Plan::Upgrade { from } => {
            apply_with_transaction(connection, |tx| steps::run_from(tx, from))?;
            // A version upgrade can coexist with an older malformed physical
            // shape (for example a pre-release v10 CHECK constraint). Reuse the
            // current-schema validation, targeted healing, and rebuild policy
            // after the additive migration commits.
            apply_keep(connection)
        }
        Plan::Rebuild => {
            backup::backup_before_rebuild(connection)?;
            apply_with_transaction(connection, reset_catalog_schema)
        }
    }
}

fn apply_keep(connection: &mut Connection) -> AppResult<()> {
    if catalog_schema_is_valid(connection)? {
        return Ok(());
    }

    // Soft-heal WIP catalogs originating from v10 with a CHECK that rejects
    // `preparing`, without wiping the rest of the catalog.
    apply_with_transaction(connection, |tx| {
        pending_file_mutations::ensure_correct_shape(tx)
    })?;

    if catalog_schema_is_valid(connection)? {
        return Ok(());
    }

    backup::backup_before_rebuild(connection)?;
    apply_with_transaction(connection, reset_catalog_schema)
}

fn apply_with_transaction(
    connection: &mut Connection,
    operation: impl FnOnce(&Connection) -> AppResult<()>,
) -> AppResult<()> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| storage_context("could not start sqlite migration transaction", error))?;

    operation(&transaction)?;

    transaction
        .commit()
        .map_err(|error| storage_context("could not commit sqlite migration transaction", error))?;

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Plan {
    Keep,
    ApplyBaseline,
    Upgrade { from: i32 },
    Rebuild,
}

fn classify(connection: &Connection) -> AppResult<Plan> {
    let schema_version = version::read(connection)?;

    if schema_version == CURRENT_SCHEMA_VERSION {
        return Ok(Plan::Keep);
    }
    if schema_version == 0 {
        return if database_has_user_schema(connection)? {
            Ok(Plan::Rebuild)
        } else {
            Ok(Plan::ApplyBaseline)
        };
    }
    if steps::can_upgrade_from(schema_version) {
        return Ok(Plan::Upgrade {
            from: schema_version,
        });
    }
    Ok(Plan::Rebuild)
}
