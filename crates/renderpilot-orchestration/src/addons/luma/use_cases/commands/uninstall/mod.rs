//! Uninstalls Luma from a game.

mod execute;
mod plan;

#[cfg(test)]
mod tests;

use renderpilot_application::{InstalledAddonRepository, ProxyTopologyRepository};
use renderpilot_domain::{
    AddonKind, EngineConfigJournal, GameId, InstalledAddon, InstalledAddonHostKind,
    normalized_path_relation,
};

use crate::game_mutation_lock;
use crate::{Context, ServiceError};

pub(crate) use plan::PreparedExternalOwnerRelease;

/// Prepares the receipt-bounded filesystem reversal for an inactive external
/// Luma Proxy owner. The caller holds the game's mutation boundary and owns the
/// eventual durable file transaction and final expected-owner catalog commit.
pub(crate) fn prepare_external_owner_release(
    context: &Context,
    game_id: &GameId,
    record: &InstalledAddon,
) -> Result<PreparedExternalOwnerRelease, ServiceError> {
    if record.game_id() != game_id
        || record.kind() != AddonKind::Luma
        || matches!(
            record.host_kind(),
            Some(InstalledAddonHostKind::SharedVulkanLayer)
        )
    {
        return Err(ServiceError::invalid_input(
            "Luma external owner release requires this game's Proxy record",
        ));
    }

    let game = context
        .storage()
        .find_active_game(game_id)?
        .ok_or_else(|| ServiceError::invalid_input("Luma external owner game is not active"))?;
    let game_root =
        crate::paths::canonical_candidate(std::path::Path::new(game.install_path().as_str()))
            .map_err(|error| {
                ServiceError::invalid_input(format!(
                    "Luma external owner game root is invalid: {error}"
                ))
            })?;

    let current = context
        .storage()
        .get_installed_addon(game_id)?
        .ok_or_else(|| ServiceError::invalid_input("Luma external owner disappeared"))?;
    if !record.eq_ignoring_persistence_timestamps(&current) {
        return Err(ServiceError::invalid_input(
            "Luma external owner changed before release preparation",
        ));
    }
    if context.storage().get_proxy_topology(game_id)?.is_some()
        || crate::addons::tool::record_is_active_for_game(context, &current)?
    {
        return Err(ServiceError::invalid_input(
            "Luma external owner is still active in the current game loading chain",
        ));
    }

    let prepared =
        plan::plan_external_owner_release(context, game_id, current.clone(), &game_root)?;
    ensure_engine_journal_is_outside_file_receipts(
        current.engine_config_journal(),
        prepared.targets(),
    )?;
    Ok(prepared)
}

fn ensure_engine_journal_is_outside_file_receipts(
    journal: Option<&EngineConfigJournal>,
    targets: &crate::addons::mutation_targets::MutationTargets,
) -> Result<(), ServiceError> {
    let Some(journal) = journal else {
        return Ok(());
    };
    let mut journal_paths = Vec::new();
    if let Some(stable) = journal.stable.as_ref() {
        journal_paths.push(std::path::PathBuf::from(&stable.path));
    }
    if let Some(pending) = journal.pending.as_ref() {
        journal_paths.extend(
            pending
                .prior
                .iter()
                .chain(pending.after.iter())
                .map(|receipt| std::path::PathBuf::from(&receipt.path)),
        );
        let stage_parent = pending
            .after
            .as_ref()
            .or(pending.prior.as_ref())
            .and_then(|receipt| std::path::Path::new(&receipt.path).parent());
        if let Some(parent) = stage_parent {
            journal_paths.push(parent.join(&pending.stage_name));
        }
    }
    let journal_paths = crate::fs::expand_with_sidecars(journal_paths);
    if journal_paths.iter().any(|journal_path| {
        targets.paths.iter().any(|receipt_path| {
            normalized_path_relation(
                &journal_path.to_string_lossy(),
                &receipt_path.to_string_lossy(),
            ) != renderpilot_domain::NormalizedPathRelation::Disjoint
        })
    }) {
        return Err(ServiceError::invalid_input(
            "Luma external file receipts overlap the independent Engine.ini owner",
        ));
    }
    Ok(())
}

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
    let guard = crate::mutation_boundary::enter_game_mutation_boundary(context, game_id)?;
    crate::addons::luma::dependency::ensure_uninstall_allowed(context, game_id)?;
    uninstall_locked(context, &guard, game_id)?;
    drop(guard);

    if let Err(error) = crate::catalog::refresh_game_components_sync(context, game_id) {
        tracing::warn!("failed to refresh game components after Luma uninstall: {error}");
    }

    Ok(())
}

/// Uninstalls Luma while a compound operation owns the game mutation boundary.
pub(crate) fn uninstall_locked(
    context: &Context,
    guard: &game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
) -> Result<(), ServiceError> {
    if guard.game_id() != game_id {
        return Err(ServiceError::invalid_input(
            "Luma uninstall guard does not match the requested game",
        ));
    }
    match plan::plan_uninstall(context, game_id)? {
        plan::UninstallPlan::Inactive(plan) => {
            let plan::InactiveUninstallPlan { apply, workset } = *plan;
            crate::addons::durable::run_uninstall_workset(
                crate::addons::durable::UninstallWorkset {
                    context,
                    guard,
                    workset,
                    feature: crate::addons::mutation_features::LUMA_UNINSTALL,
                    game_id,
                },
                |mutation_id| {
                    execute::execute_uninstall_body(context, game_id, &apply, mutation_id)
                },
                || execute::journal_cascade_after_commit(context, game_id, &apply),
            )
        }
        plan::UninstallPlan::Active(apply) => {
            execute::execute_active_uninstall(context, guard, *apply)
        }
    }
}
