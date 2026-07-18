//! Uninstalls Luma from a game.

mod execute;
mod plan;

#[cfg(test)]
mod tests;

use renderpilot_domain::GameId;

use crate::game_mutation_lock;
use crate::{Context, ServiceError};

/// Uninstalls Luma from a game, restoring files and clearing install metadata.
/// A record belonging to a different addon kind (e.g. RenoDX) is never touched --
/// this reports "not installed" for Luma exactly as if there were no record.
///
/// Commit order: **restore/remove on-disk files first**, then delete the DB row
/// (matches RenoDX). If file uninstall fails, the row stays so the desktop UI
/// (which keeps installed state on command error) and the DB remain consistent
/// and the user can retry. Kind safety comes from `record_of_kind` before any
/// mutation.
pub fn uninstall(context: &Context, game_id: &GameId) -> Result<(), ServiceError> {
    let guard = game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    let plan = plan::plan_uninstall(context, game_id)?;
    let plan::UninstallPlan { apply, workset } = plan;

    crate::addons::durable::run_uninstall_workset(
        crate::addons::durable::UninstallWorkset {
            context,
            guard: &guard,
            workset,
            feature: crate::addons::mutation_features::LUMA_UNINSTALL,
            game_id,
        },
        |mutation_id| execute::execute_uninstall_body(context, game_id, &apply, mutation_id),
        || execute::journal_cascade_after_commit(context, game_id, &apply),
    )
}
