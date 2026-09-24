//! Stable inventory of managed state owned by one game.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use renderpilot_application::{
    ComponentRepository, InstalledAddonRepository, OptiScalerStateRepository,
};
use renderpilot_domain::{
    ComponentId, ComponentRollbackBaseline, GameId, InstalledAddon, LibraryComponent,
    OptiScalerInstallState, normalized_path_key,
};

use crate::ServiceError;

/// Complete read-only inventory used by root correction and removal.
#[derive(Debug, Clone)]
pub(in crate::catalog) struct ManagedGameStateInventory {
    /// Pending durable mutations that should normally be recovered on lock entry.
    pub pending_recovery_count: usize,
    /// Component rollback aggregates owned by the game.
    pub component_ids: Vec<ComponentId>,
    pub(super) component_baselines: BTreeMap<ComponentId, ComponentRollbackBaseline>,
    /// Baselines whose component row disappeared during a later scan.
    pub orphaned_component_ids: BTreeSet<ComponentId>,
    /// Installed add-on aggregate, when present.
    pub addon: Option<InstalledAddon>,
    /// Dedicated OptiScaler aggregate, when present.
    pub optiscaler_state: Option<OptiScalerInstallState>,
    /// A generic OptiScaler row is never a valid substitute for the dedicated
    /// aggregate. It is retained only so planning can reject the malformed
    /// inventory before any inverse action runs.
    pub malformed_optiscaler_addon: Option<InstalledAddon>,
    /// Number of target-setting claims this game participates in.
    pub nvapi_claim_count: usize,
    /// Whether the game owns a RenderPilot-created driver profile.
    pub nvapi_owned_profile: bool,
    /// Unresolved driver operations that must be reconciled before cleanup.
    pub nvapi_pending_count: usize,
}

impl ManagedGameStateInventory {
    /// Whether removal needs any inverse action before deleting the card.
    pub(in crate::catalog) fn is_empty(&self) -> bool {
        self.pending_recovery_count == 0
            && self.component_ids.is_empty()
            && self.addon.is_none()
            && self.optiscaler_state.is_none()
            && self.malformed_optiscaler_addon.is_none()
            && self.nvapi_claim_count == 0
            && !self.nvapi_owned_profile
            && self.nvapi_pending_count == 0
    }
}

/// Captures all managed state in stable order.
pub(in crate::catalog) fn inventory(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<ManagedGameStateInventory, ServiceError> {
    let storage = context.storage();
    let component_baselines = storage
        .component_backups_for_game(game_id)?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let component_ids = component_baselines.keys().cloned().collect::<Vec<_>>();
    let current_components = storage.list_components_for_game(game_id)?;
    let current_component_ids: HashSet<&ComponentId> = current_components
        .iter()
        .map(LibraryComponent::id)
        .collect();
    let orphaned_component_ids = component_ids
        .iter()
        .filter(|component_id| !current_component_ids.contains(*component_id))
        .cloned()
        .collect();
    let installed_addon = storage.get_installed_addon(game_id)?;
    let (addon, malformed_optiscaler_addon) = match installed_addon {
        Some(addon) if addon.kind() == renderpilot_domain::AddonKind::OptiScaler => {
            (None, Some(addon))
        }
        other => (other, None),
    };
    let nvapi_claims = storage.list_nvapi_setting_claims_for_game(game_id.as_str())?;
    let nvapi_owned_profile = storage.get_nvapi_owned_profile(game_id.as_str())?;
    let target_ids = nvapi_claims
        .iter()
        .map(|claim| claim.target_id.as_str())
        .chain(
            nvapi_owned_profile
                .iter()
                .map(|profile| profile.profile_name.as_str()),
        )
        .collect::<HashSet<_>>();
    let target_paths = nvapi_claims
        .iter()
        .map(|claim| normalized_path_key(&claim.executable_path))
        .chain(
            nvapi_owned_profile
                .iter()
                .map(|profile| normalized_path_key(&profile.binding_path)),
        )
        .collect::<HashSet<_>>();
    let pending_nvapi_count = storage
        .list_pending_nvapi_operations()?
        .iter()
        .filter(|operation| {
            operation.game_id.as_deref() == Some(game_id.as_str())
                || operation
                    .target_id
                    .as_deref()
                    .is_some_and(|target| target_ids.contains(target))
                || operation_references_paths(&operation.before_json, &target_paths)
                || operation_references_paths(&operation.after_json, &target_paths)
        })
        .count();
    Ok(ManagedGameStateInventory {
        pending_recovery_count: storage.pending_file_mutations_for_game(game_id)?.len(),
        component_ids,
        component_baselines,
        orphaned_component_ids,
        addon,
        optiscaler_state: storage.get_optiscaler_install_state(game_id)?,
        malformed_optiscaler_addon,
        nvapi_claim_count: nvapi_claims.len(),
        nvapi_owned_profile: nvapi_owned_profile.is_some(),
        nvapi_pending_count: pending_nvapi_count,
    })
}

fn operation_references_paths(snapshot: &str, target_paths: &HashSet<String>) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(snapshot) else {
        return false;
    };
    ["binding_path", "executable_path", "old_path", "new_path"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(serde_json::Value::as_str))
        .any(|path| {
            let path_key = normalized_path_key(path);
            target_paths.contains(&path_key)
        })
}
