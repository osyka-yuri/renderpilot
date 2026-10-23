//! Safe removal of a user-managed game card from the catalog.

use renderpilot_application::GameRepository;
use renderpilot_domain::{GameId, RootAuthority};

use super::managed_state::ManagedCleanupBoundary;
use crate::ServiceError;

mod assessment;

pub use crate::nvapi::ops::claim_release::{
    NvapiClaimAbandonmentAuthorization, NvapiClaimAbandonmentToken,
};
pub use assessment::{
    GameRemovalAssessment, NvapiClaimAssessment, NvapiClaimDisposition, NvapiRemovalAssessment,
    NvapiSettingValueDto, assess_game_removal,
};

/// Result of removing one user-managed game from the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveGameFromCatalogResult {
    /// Stable identity that was removed.
    pub game_id: String,
}

/// Optional parameters for catalog game removal.
#[derive(Debug, Clone, Default)]
pub struct RemoveGameOptions {
    /// Explicit user authorization to abandon divergent NVAPI setting claims without restoring them.
    pub abandonment_authorization: Option<NvapiClaimAbandonmentAuthorization>,
}

/// Removes one user-managed card after executing its complete cleanup plan.
///
/// The catalog and game/shared guards cover the complete compound operation.
/// NVAPI setting claims are restored or safely released before any irreversible
/// filesystem rollback executes. The card is deleted only after a fresh inventory
/// proves that no managed state remains.
pub fn remove_game_from_catalog(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<RemoveGameFromCatalogResult, ServiceError> {
    remove_game_from_catalog_with_options(context, game_id, &RemoveGameOptions::default())
}

/// Removes one user-managed card with explicit options, such as abandonment authorization.
pub fn remove_game_from_catalog_with_options(
    context: &crate::Context,
    game_id: &GameId,
    options: &RemoveGameOptions,
) -> Result<RemoveGameFromCatalogResult, ServiceError> {
    let _catalog_guard = context.catalog_scan_guard();
    require_removable_game(context, game_id)?;
    let auth = options.abandonment_authorization.as_ref();
    loop {
        let game_guard = crate::mutation_boundary::enter_game_mutation_boundary(context, game_id)?;
        let cleanup = super::managed_state::ManagedCleanupPlan::build_locked(
            context,
            &game_guard,
            game_id,
            auth,
        )?;
        let ManagedCleanupBoundary::SharedRenoDx { registered_exe } = &cleanup.boundary else {
            return remove_game_with_game_guard(context, game_id, &game_guard, &cleanup);
        };
        let expected_exe = registered_exe.clone();

        // Shared Vulkan cleanup must re-enter through game -> shared. The
        // initial game-only plan is only a routing hint; the complete plan is
        // rebuilt after both guards are held before any mutation begins.
        drop(game_guard);
        let guards =
            crate::mutation_boundary::enter_game_shared_mutation_boundary(context, game_id)?;
        let refreshed = super::managed_state::ManagedCleanupPlan::build_locked(
            context,
            guards.game(),
            game_id,
            auth,
        )?;
        if refreshed.boundary
            != (ManagedCleanupBoundary::SharedRenoDx {
                registered_exe: expected_exe,
            })
        {
            drop(guards);
            continue;
        }
        return remove_game_with_shared_guards(context, game_id, guards, &refreshed);
    }
}

fn remove_game_with_game_guard(
    context: &crate::Context,
    game_id: &GameId,
    game_guard: &crate::game_mutation_lock::GameMutationGuard,
    cleanup: &super::managed_state::ManagedCleanupPlan,
) -> Result<RemoveGameFromCatalogResult, ServiceError> {
    cleanup.execute_locked(context, game_guard, game_id)?;
    remove_game_after_cleanup(context, game_id, game_guard)
}

fn remove_game_with_shared_guards(
    context: &crate::Context,
    game_id: &GameId,
    guards: crate::mutation_boundary::GameSharedMutationGuards,
    cleanup: &super::managed_state::ManagedCleanupPlan,
) -> Result<RemoveGameFromCatalogResult, ServiceError> {
    cleanup.execute_shared_locked(context, &guards, game_id)?;
    let game_guard = guards.into_game();
    remove_game_after_cleanup(context, game_id, &game_guard)
}

fn remove_game_after_cleanup(
    context: &crate::Context,
    game_id: &GameId,
    _game_guard: &crate::game_mutation_lock::GameMutationGuard,
) -> Result<RemoveGameFromCatalogResult, ServiceError> {
    let remaining = super::managed_state::inventory(context, game_id)?;
    if !remaining.is_empty() {
        return Err(ServiceError::GameRemovalCleanupFailed {
            game_id: game_id.as_str().to_owned(),
            action: "cleanup verification".to_owned(),
            reason: "managed state remains after all planned inverse actions".to_owned(),
        });
    }

    let storage = context.storage();
    let deleted = storage.delete_game(game_id)?;
    if let Some(catalog_path) = storage.catalog_file_path()? {
        crate::covers::unlink_cover_file_best_effort(
            &catalog_path,
            deleted.old_cover_file_name.as_deref(),
        );
    }

    Ok(RemoveGameFromCatalogResult {
        game_id: game_id.as_str().to_owned(),
    })
}

pub(super) fn require_removable_game(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<renderpilot_domain::GameInstallation, ServiceError> {
    let game = context
        .storage()
        .find_game(game_id)?
        .ok_or_else(|| ServiceError::GameNotFound(game_id.as_str().to_owned()))?;
    if context.storage().is_installation_absent(game_id)? {
        return Err(ServiceError::invalid_input(
            "an absent installation cannot be removed manually",
        ));
    }
    if game.root_authority() == RootAuthority::LauncherManifest {
        return Err(ServiceError::invalid_input(
            "launcher-managed games cannot be removed from the catalog because launcher refresh would add them again",
        ));
    }
    Ok(game)
}

#[cfg(test)]
mod tests;
