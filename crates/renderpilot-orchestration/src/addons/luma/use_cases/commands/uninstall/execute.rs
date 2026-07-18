//! Filesystem reverse + DB commit body for Luma uninstall.

use renderpilot_domain::{AddonKind, GameId};

use crate::addons::luma::install::uninstall_engine_files;
use crate::{Context, ServiceError};

use super::plan::UninstallApply;

pub(super) fn execute_uninstall_body(
    context: &Context,
    game_id: &GameId,
    apply: &UninstallApply,
    mutation_id: Option<&str>,
) -> Result<(), ServiceError> {
    // Metadata-only: roots are gone -- FS reverse is best-effort so a
    // missing tree cannot block clearing the install row.
    if mutation_id.is_none() {
        if let Err(error) =
            crate::catalog::cascade::apply_cascade_rollback_fs(&apply.rollback_specs)
        {
            log::warn!("luma metadata-only uninstall cascade FS failed: {error}");
        }
        for release in &apply.release_plans {
            if let Err(error) = release.execute() {
                log::warn!("luma metadata-only uninstall managed release failed: {error}");
            }
        }
        if let Err(error) = uninstall_engine_files(&apply.record) {
            log::warn!("luma metadata-only uninstall engine cleanup failed: {error}");
        }
    } else {
        crate::catalog::cascade::apply_cascade_rollback_fs(&apply.rollback_specs)?;
        for release in &apply.release_plans {
            release.execute()?;
        }
        uninstall_engine_files(&apply.record)?;
    }
    context
        .storage()
        .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
            game_id,
            component_set: Some(&apply.next_components),
            baseline_inserts: &[],
            baseline_deletes: &apply.rolled_back_ids,
            addon: renderpilot_storage_sqlite::InstalledAddonMutation::Delete(AddonKind::Luma),
            mutation_id,
        })?;
    Ok(())
}

pub(super) fn journal_cascade_after_commit(
    context: &Context,
    game_id: &GameId,
    apply: &UninstallApply,
) {
    crate::catalog::cascade::record_cascade_rollback_journal(
        context.storage(),
        game_id,
        &apply.rollback_specs,
    );
}
