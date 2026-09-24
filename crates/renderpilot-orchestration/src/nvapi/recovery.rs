//! Restart reconciliation for durable NVIDIA DRS mutation intents.
//!
//! The caller owns the game mutation boundary (when one applies) and the
//! process-wide DRS lock. Recovery only observes through a newly opened DRS
//! session; it never attempts to repeat a driver mutation.

use std::collections::HashMap;

use renderpilot_application::AppResult;
use renderpilot_nvapi::{ApplicationIdentity, ProfileIdentity, SettingIdentity};
use renderpilot_storage_sqlite::{NvapiPendingOperationRow, SqliteStorage};

use crate::Context;
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Lookup<T> {
    Found(T),
    #[default]
    Missing,
    Ambiguous,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedProfile {
    identity: ProfileIdentity,
    applications: Vec<ApplicationIdentity>,
    settings: Vec<SettingIdentity>,
    matched_application: Option<ApplicationIdentity>,
    explicit_setting: Option<(u32, bool, Option<u32>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Observation {
    named_profile: Lookup<ObservedProfile>,
    path_profiles: HashMap<String, Lookup<ObservedProfile>>,
    base_profile: Lookup<ObservedProfile>,
}

mod observe;
mod receipts;
mod reconcile;
#[cfg(test)]
mod tests;
/// Reconciles pending mutations for one game. Must be called while holding
/// that game's mutation boundary and the shared DRS operation lock.
pub(super) fn recover_game_pending(context: &Context, game_id: &str) -> AppResult<()> {
    let rows = context
        .storage()
        .list_pending_nvapi_operations()?
        .into_iter()
        .filter(|row| row.game_id.as_deref() == Some(game_id))
        .collect::<Vec<_>>();
    recover_rows(context.storage(), &rows, observe::observe_live)
}

/// Reconciles global-profile mutations while holding the shared DRS lock.
pub(super) fn recover_global_pending(context: &Context) -> AppResult<()> {
    let rows = context
        .storage()
        .list_pending_nvapi_operations()?
        .into_iter()
        .filter(|row| row.game_id.is_none())
        .collect::<Vec<_>>();
    recover_rows(context.storage(), &rows, observe::observe_live)
}

fn recover_rows<F>(
    storage: &SqliteStorage,
    rows: &[NvapiPendingOperationRow],
    mut observe: F,
) -> AppResult<()>
where
    F: FnMut(&NvapiPendingOperationRow) -> Result<Observation, String>,
{
    for row in rows {
        if row.phase == "conflict" {
            continue;
        }
        if let Err(error) = receipts::parse_snapshot(&row.before_json) {
            reconcile::mark_conflict(storage, row, &format!("invalid before receipt: {error}"))?;
            continue;
        }
        if let Err(error) = receipts::parse_snapshot(&row.after_json) {
            reconcile::mark_conflict(storage, row, &format!("invalid after receipt: {error}"))?;
            continue;
        }
        let observation = match observe(row) {
            Ok(observation) => observation,
            Err(error) => {
                tracing::warn!(
                    operation = %row.op_id,
                    kind = %row.kind,
                    %error,
                    "NVIDIA DRS mutation remains pending because a fresh session could not observe it"
                );
                continue;
            }
        };
        reconcile::reconcile_row(storage, row, &observation)?;
    }
    Ok(())
}
