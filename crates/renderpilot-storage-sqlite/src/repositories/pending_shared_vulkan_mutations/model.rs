use std::str::FromStr;

use renderpilot_application::AppError;
use renderpilot_domain::{GameId, SharedArtifactKind, SharedArtifactRecord};

use super::super::game_mutations::InstalledAddonMutation;

/// Scope of a shared Vulkan mutation reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedVulkanMutationScope {
    /// The shared layer changes without a game-owned add-on lifecycle change.
    SharedOnly,
    /// The shared layer and one game's files/add-on state change together.
    GameShared,
}

impl SharedVulkanMutationScope {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::SharedOnly => "shared_only",
            Self::GameShared => "game_shared",
        }
    }
}

impl FromStr for SharedVulkanMutationScope {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "shared_only" => Ok(Self::SharedOnly),
            "game_shared" => Ok(Self::GameShared),
            _ => Err(AppError::storage_failed(format!(
                "invalid shared Vulkan mutation scope `{value}`"
            ))),
        }
    }
}

/// Durable phase of a shared Vulkan filesystem transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingSharedVulkanMutationState {
    /// The reservation exists but before-snapshots are not complete.
    Preparing,
    /// Before-snapshots are complete and the shared mutation may begin.
    Prepared,
    /// Database feature state has committed; snapshots can be removed.
    Committed,
}

impl PendingSharedVulkanMutationState {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Prepared => "prepared",
            Self::Committed => "committed",
        }
    }
}

impl FromStr for PendingSharedVulkanMutationState {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "preparing" => Ok(Self::Preparing),
            "prepared" => Ok(Self::Prepared),
            "committed" => Ok(Self::Committed),
            _ => Err(AppError::storage_failed(format!(
                "invalid shared Vulkan mutation state `{value}`"
            ))),
        }
    }
}

/// One singleton shared-Vulkan durable row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSharedVulkanMutationRow {
    /// Mutation token. It is unique across the singleton table.
    pub id: String,
    /// Whether the owner is a game or only the shared layer.
    pub scope: SharedVulkanMutationScope,
    /// Game owner for a game-shared reservation.
    pub game_id: Option<GameId>,
    /// Feature label owned by orchestration.
    pub feature: String,
    /// Durable phase.
    pub state: PendingSharedVulkanMutationState,
    /// JSON object containing exact shared and, when applicable, game snapshots.
    pub manifest_json: String,
    /// Immutable, orchestration-issued roots that authorize every manifest path.
    /// This value is fixed when the singleton is reserved and is never amended
    /// by the preparation transition.
    pub root_capabilities_json: String,
}

/// Inputs accepted when reserving the singleton shared-Vulkan resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginSharedVulkanMutation {
    /// Unique mutation token.
    pub id: String,
    /// Resource scope.
    pub scope: SharedVulkanMutationScope,
    /// Game owner for [`SharedVulkanMutationScope::GameShared`].
    pub game_id: Option<GameId>,
    /// Feature label owned by orchestration.
    pub feature: String,
    /// Initial JSON-object manifest persisted before snapshot work.
    pub initial_manifest_json: String,
    /// Immutable JSON-object root authority persisted with the reservation.
    pub root_capabilities_json: String,
}

/// Result of trying to reserve the singleton resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedVulkanMutationReservation {
    /// This call inserted the reservation.
    Reserved(PendingSharedVulkanMutationRow),
    /// Another transaction owns the singleton; no row was changed.
    Occupied(PendingSharedVulkanMutationRow),
}

/// A shared artifact change committed with the durable row.
#[derive(Debug, Clone, Copy, Default)]
pub enum SharedArtifactMutation<'a> {
    /// Keep the current advisory provenance row.
    #[default]
    Keep,
    /// Insert or replace one validated provenance row.
    Upsert(&'a SharedArtifactRecord),
    /// Delete one provenance row.
    Delete(SharedArtifactKind),
}

/// Database half of one shared-Vulkan durable mutation.
#[derive(Debug, Clone, Copy)]
pub struct SharedVulkanMutationCommit<'a> {
    /// Exact prepared mutation token.
    pub id: &'a str,
    /// Expected reservation scope.
    pub scope: SharedVulkanMutationScope,
    /// Expected owner. Must match the row exactly.
    pub game_id: Option<&'a GameId>,
    /// Optional game add-on lifecycle effect.
    pub addon: InstalledAddonMutation<'a>,
    /// Shared artifact provenance effect.
    pub shared_artifact: SharedArtifactMutation<'a>,
}

/// Opaque proof that a matching prepared row can be resolved after restore.
#[derive(Debug)]
pub struct PreparedSharedVulkanMutationResolutionFence {
    pub(super) id: String,
    pub(super) scope: SharedVulkanMutationScope,
    pub(super) game_id: Option<GameId>,
    pub(super) catalog_binding: SharedCatalogBinding,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum SharedCatalogBinding {
    Absent,
    Invalidated { authority_epoch: u64 },
}
