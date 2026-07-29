//! Execution of an already preflighted managed-cleanup plan.

use renderpilot_domain::{AddonKind, GameId};

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
            let result = match action {
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
                ManagedInverseAction::UninstallAddon(AddonKind::Luma) => {
                    crate::addons::luma::use_cases::commands::uninstall::uninstall_locked(
                        context, guard, game_id,
                    )
                }
                ManagedInverseAction::UninstallAddon(AddonKind::RenoDx) => {
                    crate::addons::renodx::use_cases::commands::uninstall::uninstall_locked(
                        context, guard, game_id,
                    )
                }
                ManagedInverseAction::RestoreNvapi => {
                    crate::nvapi::ops::restore_game_baselines(context, game_id.as_str())
                }
            };
            result.map_err(|error| ServiceError::GameRemovalCleanupFailed {
                game_id: game_id.as_str().to_owned(),
                action: action.label(),
                reason: error.to_string(),
            })?;
        }
        Ok(())
    }
}
