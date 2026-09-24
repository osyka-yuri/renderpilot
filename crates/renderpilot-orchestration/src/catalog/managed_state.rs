//! Inventory and inverse-action planning for managed state owned by one game.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use renderpilot_domain::{
    AddonKind, ComponentId, GameId, InstalledAddon, InstalledAddonHostKind, NormalizedPathRelation,
    normalized_path_key, normalized_path_relation,
};

use crate::ServiceError;

mod execution;
mod inventory;

pub(super) use inventory::inventory;

/// One durable inverse action in execution order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ManagedInverseAction {
    RollbackComponent(ComponentId),
    ReleaseRedundantComponentBaseline(ComponentId),
    UninstallOptiScaler,
    UninstallAddon(AddonKind),
    RestoreNvapi,
}

/// Lock boundary selected by the complete cleanup plan.
///
/// This is deliberately a property of the plan rather than a second routing
/// read from storage.  The plan is built under the game boundary, and a
/// shared-Vulkan plan is rebuilt after acquiring the ordered game/shared
/// boundaries before any inverse action can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ManagedCleanupBoundary {
    GameOnly,
    SharedRenoDx { registered_exe: PathBuf },
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
            Self::UninstallOptiScaler => "OptiScaler add-on uninstall".to_owned(),
            Self::UninstallAddon(kind) => format!("{} add-on uninstall", addon_kind_name(*kind)),
            Self::RestoreNvapi => "NVIDIA setting claim restore".to_owned(),
        }
    }
}

/// Fully preflighted cleanup sequence.
#[derive(Debug, Clone)]
pub(super) struct ManagedCleanupPlan {
    actions: Vec<ManagedInverseAction>,
    shared_renodx: Option<InstalledAddon>,
    pub(super) boundary: ManagedCleanupBoundary,
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
        if inventory.malformed_optiscaler_addon.is_some() {
            return Err(ServiceError::GameRemovalCleanupFailed {
                game_id: game_id.as_str().to_owned(),
                action: "managed add-on inventory".to_owned(),
                reason: "generic OptiScaler record has no dedicated install state".to_owned(),
            });
        }
        if inventory.pending_recovery_count != 0 {
            return Err(ServiceError::GameRemovalCleanupFailed {
                game_id: game_id.as_str().to_owned(),
                action: "pending file recovery".to_owned(),
                reason: "durable recovery did not finish after acquiring the game lock".to_owned(),
            });
        }
        if inventory.nvapi_pending_count != 0 {
            return Err(ServiceError::GameRemovalCleanupFailed {
                game_id: game_id.as_str().to_owned(),
                action: "pending NVIDIA driver recovery".to_owned(),
                reason: "a durable DRS operation must be reconciled before removing this game"
                    .to_owned(),
            });
        }
        if inventory.nvapi_owned_profile {
            return Err(ServiceError::GameRemovalCleanupFailed {
                game_id: game_id.as_str().to_owned(),
                action: "RenderPilot-owned NVIDIA profile".to_owned(),
                reason: "delete the owned NVIDIA profile from the game details page before removing this game".to_owned(),
            });
        }
        let optiscaler_footprint = inventory
            .optiscaler_state
            .as_ref()
            .map(|_| {
                crate::addons::optiscaler::preflight_managed_cleanup_uninstall_locked(
                    context, game_id, guard,
                )
            })
            .transpose()
            .map_err(|error| ServiceError::GameRemovalCleanupFailed {
                game_id: game_id.as_str().to_owned(),
                action: "OptiScaler add-on uninstall".to_owned(),
                reason: error.to_string(),
            })?;

        let shared_renodx = match inventory.addon.as_ref() {
            Some(addon) if addon.kind() == AddonKind::RenoDx => match addon.host_kind() {
                Some(InstalledAddonHostKind::SharedVulkanLayer)
                    if addon.registered_exe_path().is_some() =>
                {
                    Some(addon.clone())
                }
                Some(InstalledAddonHostKind::SharedVulkanLayer) => {
                    return Err(invalid_managed_addon_inventory(
                        game_id,
                        "shared RenoDX record has no registered executable",
                    ));
                }
                _ => None,
            },
            Some(addon) if addon.kind() == AddonKind::Luma => {
                if addon.host_kind() == Some(InstalledAddonHostKind::SharedVulkanLayer) {
                    return Err(invalid_managed_addon_inventory(
                        game_id,
                        "Luma cannot use the shared Vulkan host cleanup route",
                    ));
                }
                None
            }
            Some(addon) => {
                return Err(invalid_managed_addon_inventory(
                    game_id,
                    &format!(
                        "unsupported generic add-on kind {}",
                        addon_kind_name(addon.kind())
                    ),
                ));
            }
            None => None,
        };

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

        if let Some(footprint) = optiscaler_footprint.as_ref() {
            ambiguous_targets.extend(optiscaler_collisions(&component_targets, footprint));
        }

        if !ambiguous_targets.is_empty() {
            let associated_paths = cleanup_associated_paths(
                &component_plans,
                inventory.addon.as_ref(),
                optiscaler_footprint.as_ref(),
            );
            let targets: Vec<_> = ambiguous_targets.into_iter().collect();
            let recovery_bundle = super::recovery_bundle::create_managed_cleanup_recovery_bundle(
                context.storage(),
                game_id.as_str(),
                &targets,
                &associated_paths,
            )?;
            return Err(ServiceError::ManagedCleanupAmbiguous {
                game_id: game_id.as_str().to_owned(),
                targets,
                recovery_bundle_path: recovery_bundle.to_string_lossy().into_owned(),
            });
        }

        let mut actions = Vec::new();
        if inventory.optiscaler_state.is_some() {
            actions.push(ManagedInverseAction::UninstallOptiScaler);
        }
        actions.extend(inventory.component_ids.into_iter().map(|component_id| {
            if redundant_component_ids.contains(&component_id) {
                ManagedInverseAction::ReleaseRedundantComponentBaseline(component_id)
            } else {
                ManagedInverseAction::RollbackComponent(component_id)
            }
        }));
        if let Some(addon) = inventory.addon {
            actions.push(ManagedInverseAction::UninstallAddon(addon.kind()));
        }
        if inventory.nvapi_claim_count > 0 {
            actions.push(ManagedInverseAction::RestoreNvapi);
        }
        let boundary = match shared_renodx.as_ref() {
            Some(addon) => ManagedCleanupBoundary::SharedRenoDx {
                registered_exe: PathBuf::from(
                    addon
                        .registered_exe_path()
                        .expect("shared RenoDX record was validated above")
                        .as_str(),
                ),
            },
            None => ManagedCleanupBoundary::GameOnly,
        };
        Ok(Self {
            actions,
            shared_renodx,
            boundary,
        })
    }
}

fn optiscaler_collisions(
    component_targets: &BTreeMap<ComponentId, BTreeSet<String>>,
    footprint: &crate::addons::optiscaler::OptiScalerManagedCleanupFootprint,
) -> BTreeSet<String> {
    let mut collisions = BTreeSet::new();
    for (component_id, targets) in component_targets {
        for target in targets {
            for footprint_path in footprint
                .exact_mutations
                .iter()
                .chain(&footprint.removed_directories)
            {
                let relation = normalized_path_relation(target, &footprint_path.to_string_lossy());
                if relation != NormalizedPathRelation::Disjoint {
                    collisions.insert(format!(
                        "{} <> OptiScaler: {} ({})",
                        component_id.as_str(),
                        target,
                        footprint_path.display()
                    ));
                }
            }
        }
    }
    collisions
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
    optiscaler_footprint: Option<&crate::addons::optiscaler::OptiScalerManagedCleanupFootprint>,
) -> Vec<PathBuf> {
    let mut paths = component_plans
        .iter()
        .flat_map(super::execute::ManagedComponentRollbackPlan::affected_files)
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
    if let Some(footprint) = optiscaler_footprint {
        paths.extend(footprint.exact_mutations.iter().cloned());
        paths.extend(footprint.removed_directories.iter().cloned());
    }
    paths.sort();
    paths.dedup();
    paths
}

fn invalid_managed_addon_inventory(game_id: &GameId, reason: &str) -> ServiceError {
    ServiceError::GameRemovalCleanupFailed {
        game_id: game_id.as_str().to_owned(),
        action: "managed add-on inventory".to_owned(),
        reason: reason.to_owned(),
    }
}

const fn addon_kind_name(kind: AddonKind) -> &'static str {
    match kind {
        AddonKind::Luma => "Luma",
        AddonKind::RenoDx => "RenoDX",
        AddonKind::OptiScaler => "OptiScaler",
    }
}

#[cfg(test)]
mod tests {
    use super::{ManagedCleanupBoundary, optiscaler_collisions};
    use renderpilot_domain::{ComponentId, NormalizedPathRelation, normalized_path_relation};
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    fn component_targets(path: &str) -> BTreeMap<ComponentId, BTreeSet<String>> {
        let id = ComponentId::new("component:cleanup-test").expect("component id");
        [(id, [path.to_owned()].into_iter().collect())]
            .into_iter()
            .collect()
    }

    fn footprint(path: &str) -> crate::addons::optiscaler::OptiScalerManagedCleanupFootprint {
        crate::addons::optiscaler::OptiScalerManagedCleanupFootprint {
            exact_mutations: vec![PathBuf::from(path)],
            removed_directories: Vec::new(),
        }
    }

    #[test]
    fn cleanup_boundary_is_explicit_and_stable() {
        assert_eq!(
            ManagedCleanupBoundary::GameOnly,
            ManagedCleanupBoundary::GameOnly
        );
        assert_ne!(
            ManagedCleanupBoundary::GameOnly,
            ManagedCleanupBoundary::SharedRenoDx {
                registered_exe: PathBuf::from("game.exe")
            }
        );
    }

    #[test]
    fn optiscaler_footprint_rejects_file_and_directory_collisions() {
        let cases = [
            (
                "C:/Games/Test/bin/dxgi.dll",
                "c:\\games\\test\\bin\\dxgi.dll",
                true,
            ),
            ("C:/Games/Test/bin", "C:\\Games\\Test\\bin\\dxgi.dll", true),
            ("C:/Games/Test/bin/dxgi.dll", "C:\\Games\\Test\\bin", true),
            ("C:/Games/Test/bin2/dxgi.dll", "C:\\Games\\Test\\bin", false),
        ];
        for (component_path, footprint_path, collides) in cases {
            let actual = !optiscaler_collisions(
                &component_targets(component_path),
                &footprint(footprint_path),
            )
            .is_empty();
            assert_eq!(actual, collides, "{component_path} <> {footprint_path}");
            assert_eq!(
                normalized_path_relation(component_path, footprint_path)
                    != NormalizedPathRelation::Disjoint,
                collides
            );
        }
    }
}
