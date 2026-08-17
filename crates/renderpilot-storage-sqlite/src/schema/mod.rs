//! Catalog schema application: keep, initialize, upgrade, or rebuild.
//!
//! Upgrade path is a linear step registry (sqlx-style discipline on rusqlite):
//! known `user_version` values run `from→to` steps until CURRENT, then validate.
//! Unknown or corrupt shapes rebuild from the composed baseline after a
//! file-backed backup before rebuilds and non-additive migrations.
//!
//! Healthy CURRENT catalogs take a validation-only keep path. A malformed
//! CURRENT catalog is backed up, then reset transactionally from the composed
//! baseline.

use std::collections::HashSet;

use renderpilot_application::AppResult;
use rusqlite::{Connection, TransactionBehavior};

use crate::error::storage_context;

include!(concat!(
    env!("OUT_DIR"),
    "/portable_runtime_release_contract.rs"
));

pub(crate) mod backup;
mod contract;
mod ddl;
mod objects;
pub(crate) mod physical;
pub(crate) mod portable_catalog;
mod pragmas;
mod rebuild;
mod steps;
mod validation;
mod version;

#[cfg(test)]
mod portable_catalog_tests;
#[cfg(test)]
mod tests;

use self::pragmas::ForeignKeysState;
use self::rebuild::{apply_baseline, reset_catalog_schema};
use self::validation::{
    catalog_schema_is_valid, catalog_schema_is_valid_observational, validate_catalog_schema,
};
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
//   12 → 13: durable profile-derived add-on capability cache + reliable launcher
//            scan source checkpoints (one release boundary).
//   13 → 14: canonical installation identity + explicit root authority.
//   14 → 15: generalize stored library classification columns to `technology`.
//   15 → 16: records the portable-runtime path-tag/receipt boundary. Existing
//             game install paths are external data and are intentionally not
//             rebased by catalog schema migration.
//   16 → 17: replace weak global scan caches with owner-scoped observations and
//             typed, fail-closed scan authority.
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
    let plan = classify(connection)?;

    if plan == Plan::Keep {
        return Ok(());
    }

    let foreign_keys = ForeignKeysState::capture_and_disable(connection)?;
    let result = apply_plan(connection, plan);

    foreign_keys.restore(connection, result)
}

fn apply_plan(connection: &mut Connection, plan: Plan) -> AppResult<()> {
    match plan {
        Plan::Keep => Ok(()),
        Plan::ApplyBaseline => apply_with_transaction(connection, |tx| {
            apply_baseline(tx)?;
            version::write(tx, CURRENT_SCHEMA_VERSION)?;
            validate_catalog_schema(tx)
        }),
        Plan::Upgrade { from } => {
            backup::backup_before_migration(connection, CURRENT_SCHEMA_VERSION)?;
            apply_with_transaction(connection, |tx| {
                steps::run_from(tx, from)?;
                if !catalog_schema_is_valid(tx)? {
                    // The pre-migration backup is already complete. Reset in
                    // this transaction only when the completed linear chain
                    // does not satisfy the exact current contract.
                    reset_catalog_schema(tx)?;
                }
                validate_catalog_schema(tx)?;
                validation::validate_database_integrity(tx)
            })
        }
        Plan::Rebuild => {
            backup::backup_before_rebuild(connection)?;
            apply_with_transaction(connection, reset_catalog_schema)
        }
    }
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
        return if catalog_schema_is_valid_observational(connection)? {
            Ok(Plan::Keep)
        } else {
            Ok(Plan::Rebuild)
        };
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
