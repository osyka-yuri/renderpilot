//! Execution of an already preflighted managed-cleanup plan.

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{AddonKind, GameId, InstalledAddonHostKind};

use crate::ServiceError;

use super::{ManagedCleanupPlan, ManagedInverseAction};

impl ManagedCleanupPlan {
    /// Executes the already-preflighted sequence. Every individual action uses
    /// its normal durable mechanism, so a retry resumes from fresh inventory.
    pub(in crate::catalog) fn execute_locked(
        &self,
        context: &crate::Context,
        guard: &crate::game_mutation_lock::GameMutationGuard,
        game_id: &GameId,
    ) -> Result<(), ServiceError> {
        for action in &self.actions {
            let result = execute_game_action(context, guard, game_id, action);
            map_action_result(result, game_id, action)?;
        }
        Ok(())
    }

    /// Executes the same ordered plan while the shared Vulkan lock is held.
    /// Only the exact RenoDX shared action uses that second boundary; every
    /// other action retains the ordinary game guard and durable lifecycle.
    pub(in crate::catalog) fn execute_shared_locked(
        &self,
        context: &crate::Context,
        guards: &crate::mutation_boundary::GameSharedMutationGuards,
        game_id: &GameId,
    ) -> Result<(), ServiceError> {
        for action in &self.actions {
            let result = match action {
                ManagedInverseAction::UninstallAddon(AddonKind::RenoDx)
                    if self.shared_renodx.is_some() =>
                {
                    let expected = self.shared_renodx.as_ref().expect("guarded above");
                    let current =
                        context
                            .storage()
                            .get_installed_addon(game_id)?
                            .ok_or_else(|| {
                                ServiceError::invalid_input(
                                    "shared RenoDX record disappeared during catalog cleanup",
                                )
                            })?;
                    if current.kind() != AddonKind::RenoDx
                        || current.host_kind() != Some(InstalledAddonHostKind::SharedVulkanLayer)
                        || current.registered_exe_path() != expected.registered_exe_path()
                        || !current.eq_ignoring_persistence_timestamps(expected)
                    {
                        return Err(ServiceError::invalid_input(
                            "shared RenoDX record changed during catalog cleanup",
                        ));
                    }
                    crate::addons::renodx::use_cases::commands::uninstall::uninstall_shared_locked(
                        context, guards, game_id, &current,
                    )
                }
                _ => execute_game_action(context, guards.game(), game_id, action),
            };
            map_action_result(result, game_id, action)?;
        }
        Ok(())
    }
}

fn execute_game_action(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    action: &ManagedInverseAction,
) -> Result<(), ServiceError> {
    match action {
        ManagedInverseAction::RollbackComponent(component_id) => {
            crate::catalog::execute::rollback_managed_component_locked(
                context,
                guard,
                game_id,
                component_id,
            )
        }
        ManagedInverseAction::ReleaseRedundantComponentBaseline(component_id) => {
            crate::catalog::execute::release_redundant_component_baseline_locked(
                context,
                guard,
                game_id,
                component_id,
            )
        }
        ManagedInverseAction::UninstallOptiScaler => {
            crate::addons::optiscaler::uninstall_locked(context, game_id, guard).map(|_| ())
        }
        ManagedInverseAction::UninstallAddon(AddonKind::Luma) => {
            crate::addons::luma::dependency::ensure_uninstall_allowed(context, game_id)?;
            crate::addons::luma::use_cases::commands::uninstall::uninstall_locked(
                context, guard, game_id,
            )
        }
        ManagedInverseAction::UninstallAddon(AddonKind::RenoDx) => {
            crate::addons::renodx::use_cases::commands::uninstall::uninstall_locked(
                context, guard, game_id,
            )
        }
        ManagedInverseAction::UninstallAddon(AddonKind::OptiScaler) => Err(
            ServiceError::invalid_input("OptiScaler must use its dedicated cleanup action"),
        ),
        ManagedInverseAction::RestoreNvapi => {
            crate::nvapi::ops::restore_game_setting_claims(context, guard, game_id.as_str())
        }
    }
}

fn map_action_result(
    result: Result<(), ServiceError>,
    game_id: &GameId,
    action: &ManagedInverseAction,
) -> Result<(), ServiceError> {
    result.map_err(|error| ServiceError::GameRemovalCleanupFailed {
        game_id: game_id.as_str().to_owned(),
        action: action.label(),
        reason: error.to_string(),
    })
}
