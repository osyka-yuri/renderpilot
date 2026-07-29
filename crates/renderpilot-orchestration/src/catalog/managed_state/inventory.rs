//! Stable inventory of managed state owned by one game.

use std::collections::{BTreeMap, BTreeSet};

use renderpilot_application::{ComponentRepository, InstalledAddonRepository};
use renderpilot_domain::{ComponentId, ComponentRollbackBaseline, GameId, InstalledAddon};

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
    /// Number of driver-setting baselines owned by the game.
    pub nvapi_baseline_count: usize,
}

impl ManagedGameStateInventory {
    /// Whether removal needs any inverse action before deleting the card.
    pub(in crate::catalog) fn is_empty(&self) -> bool {
        self.pending_recovery_count == 0
            && self.component_ids.is_empty()
            && self.addon.is_none()
            && self.nvapi_baseline_count == 0
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
    let mut component_ids = component_baselines.keys().cloned().collect::<Vec<_>>();
    component_ids.sort();
    let current_component_ids = storage
        .list_components_for_game(game_id)?
        .into_iter()
        .map(|component| component.id().clone())
        .collect::<BTreeSet<_>>();
    let orphaned_component_ids = component_ids
        .iter()
        .filter(|component_id| !current_component_ids.contains(*component_id))
        .cloned()
        .collect();
    Ok(ManagedGameStateInventory {
        pending_recovery_count: storage.pending_file_mutations_for_game(game_id)?.len(),
        component_ids,
        component_baselines,
        orphaned_component_ids,
        addon: storage.get_installed_addon(game_id)?,
        nvapi_baseline_count: storage
            .list_nvapi_setting_baselines_for_game(game_id.as_str())?
            .len(),
    })
}
