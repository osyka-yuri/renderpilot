//! Inventory and inverse-action planning for managed state owned by one game.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use renderpilot_domain::{AddonKind, ComponentId, GameId, InstalledAddon, normalized_path_key};

use crate::ServiceError;

mod execution;
mod inventory;

pub(super) use inventory::inventory;

/// One durable inverse action in execution order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ManagedInverseAction {
    RollbackComponent(ComponentId),
    ReleaseRedundantComponentBaseline(ComponentId),
    UninstallAddon(AddonKind),
    RestoreNvapi,
}

impl ManagedInverseAction {
    pub(super) fn label(&self) -> String {
        match self {
            Self::RollbackComponent(component_id) => {
                format!("component rollback {}", component_id.as_str())
            }
            Self::ReleaseRedundantComponentBaseline(component_id) => {
                format!("redundant component baseline {}", component_id.as_str())
            }
            Self::UninstallAddon(kind) => format!("{} add-on uninstall", addon_kind_name(*kind)),
            Self::RestoreNvapi => "NVAPI baseline restore".to_owned(),
        }
    }
}

/// Fully preflighted cleanup sequence.
#[derive(Debug, Clone)]
pub(super) struct ManagedCleanupPlan {
    actions: Vec<ManagedInverseAction>,
}

impl ManagedCleanupPlan {
    /// Builds the complete action graph before any inverse action executes.
    pub(super) fn build_locked(
        context: &crate::Context,
        guard: &crate::game_mutation_lock::GameMutationGuard,
        game_id: &GameId,
    ) -> Result<Self, ServiceError> {
        if guard.game_id() != game_id {
            return Err(ServiceError::invalid_input(
                "managed cleanup guard does not match the requested game",
            ));
        }
        let inventory = inventory(context, game_id)?;
        if inventory.pending_recovery_count != 0 {
            return Err(ServiceError::GameRemovalCleanupFailed {
                game_id: game_id.as_str().to_owned(),
                action: "pending file recovery".to_owned(),
                reason: "durable recovery did not finish after acquiring the game lock".to_owned(),
            });
        }

        let mut component_plans = Vec::new();
        for component_id in &inventory.component_ids {
            match super::execute::build_managed_rollback_plan_locked(
                context,
                guard,
                game_id,
                component_id,
            ) {
                Ok(plan) => component_plans.push(plan),
                Err(error) if inventory.orphaned_component_ids.contains(component_id) => {
                    let baseline = context
                        .storage()
                        .get_component_backup(component_id)?
                        .ok_or_else(|| {
                            ServiceError::invalid_input(
                                "orphaned rollback baseline disappeared during cleanup planning",
                            )
                        })?;
                    let associated_paths =
                        super::execute::orphaned_rollback_affected_files(&baseline)
                            .iter()
                            .map(|path| PathBuf::from(path.as_str()))
                            .collect::<Vec<_>>();
                    let target = format!("orphaned component {}: {error}", component_id.as_str());
                    let recovery_bundle =
                        super::recovery_bundle::create_managed_cleanup_recovery_bundle(
                            context.storage(),
                            game_id.as_str(),
                            std::slice::from_ref(&target),
                            &associated_paths,
                        )?;
                    return Err(ServiceError::ManagedCleanupAmbiguous {
                        game_id: game_id.as_str().to_owned(),
                        targets: vec![target],
                        recovery_bundle_path: recovery_bundle.to_string_lossy().into_owned(),
                    });
                }
                Err(error) => {
                    return Err(ServiceError::GameRemovalCleanupFailed {
                        game_id: game_id.as_str().to_owned(),
                        action: format!("component rollback {}", component_id.as_str()),
                        reason: error.to_string(),
                    });
                }
            }
        }

        let component_targets = component_plans
            .iter()
            .map(|plan| {
                (
                    plan.component_id().clone(),
                    plan.affected_files()
                        .iter()
                        .map(|path| normalized_path_key(path.as_str()))
                        .collect::<BTreeSet<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut ambiguous_targets = BTreeSet::new();

        let component_entries = component_targets.iter().collect::<Vec<_>>();
        let mut redundant_component_ids = BTreeSet::new();
        for (index, (left_id, left_targets)) in component_entries.iter().enumerate() {
            for (right_id, right_targets) in component_entries.iter().skip(index + 1) {
                let shared_targets = left_targets
                    .intersection(right_targets)
                    .cloned()
                    .collect::<Vec<_>>();
                if shared_targets.is_empty() {
                    continue;
                }
                let equivalent_inverse = inventory
                    .component_baselines
                    .get(*left_id)
                    .zip(inventory.component_baselines.get(*right_id))
                    .is_some_and(|(left, right)| {
                        !left.expected_active_files().is_empty() && left == right
                    });
                if equivalent_inverse {
                    // Two detector identities describe the exact same inverse
                    // transition. Execute it once, then consume the duplicate
                    // metadata only after the shared original state is proven.
                    redundant_component_ids.insert((*right_id).clone());
                    continue;
                }
                for target in shared_targets {
                    ambiguous_targets.insert(format!(
                        "{} <> {}: {target}",
                        left_id.as_str(),
                        right_id.as_str()
                    ));
                }
            }
        }

        if let Some(addon) = inventory.addon.as_ref() {
            // Only generic add-on engine targets represent an independent
            // inverse edge. Coordinated managed-file bindings are owned by the
            // component rollback transaction itself, which removes the binding
            // from the add-on aggregate in the same database commit.
            let engine_targets = addon_engine_targets(addon);
            for (component_id, targets) in &component_targets {
                for target in targets.intersection(&engine_targets) {
                    ambiguous_targets.insert(format!(
                        "{} <> {} add-on: {target}",
                        component_id.as_str(),
                        addon_kind_name(addon.kind())
                    ));
                }
            }
        }

        if !ambiguous_targets.is_empty() {
            let associated_paths =
                cleanup_associated_paths(&component_plans, inventory.addon.as_ref());
            let recovery_bundle = super::recovery_bundle::create_managed_cleanup_recovery_bundle(
                context.storage(),
                game_id.as_str(),
                &ambiguous_targets.iter().cloned().collect::<Vec<_>>(),
                &associated_paths,
            )?;
            return Err(ServiceError::ManagedCleanupAmbiguous {
                game_id: game_id.as_str().to_owned(),
                targets: ambiguous_targets.into_iter().collect(),
                recovery_bundle_path: recovery_bundle.to_string_lossy().into_owned(),
            });
        }

        let mut actions = inventory
            .component_ids
            .into_iter()
            .map(|component_id| {
                if redundant_component_ids.contains(&component_id) {
                    ManagedInverseAction::ReleaseRedundantComponentBaseline(component_id)
                } else {
                    ManagedInverseAction::RollbackComponent(component_id)
                }
            })
            .collect::<Vec<_>>();
        if let Some(addon) = inventory.addon {
            actions.push(ManagedInverseAction::UninstallAddon(addon.kind()));
        }
        if inventory.nvapi_baseline_count > 0 {
            actions.push(ManagedInverseAction::RestoreNvapi);
        }
        Ok(Self { actions })
    }
}

fn addon_engine_targets(addon: &InstalledAddon) -> BTreeSet<String> {
    addon
        .created_files()
        .iter()
        .chain(addon.backed_up_files())
        .map(|path| normalized_path_key(path.as_str()))
        .collect()
}

fn cleanup_associated_paths(
    component_plans: &[super::execute::ManagedComponentRollbackPlan],
    addon: Option<&InstalledAddon>,
) -> Vec<PathBuf> {
    let mut paths = component_plans
        .iter()
        .flat_map(|plan| plan.affected_files())
        .map(|path| PathBuf::from(path.as_str()))
        .collect::<Vec<_>>();
    if let Some(addon) = addon {
        paths.extend(
            addon
                .created_files()
                .iter()
                .chain(addon.backed_up_files())
                .map(|path| PathBuf::from(path.as_str())),
        );
        paths.extend(
            addon
                .managed_files()
                .iter()
                .map(|file| PathBuf::from(file.path().as_str())),
        );
    }
    paths.sort();
    paths.dedup();
    paths
}

const fn addon_kind_name(kind: AddonKind) -> &'static str {
    match kind {
        AddonKind::Luma => "Luma",
        AddonKind::RenoDx => "RenoDX",
    }
}
