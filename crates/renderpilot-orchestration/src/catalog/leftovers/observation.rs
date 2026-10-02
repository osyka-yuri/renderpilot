use renderpilot_domain::{GameInstallation, InstalledAddonHostKind};
use renderpilot_platform_windows::vulkan_layer::SharedVulkanLayerObservation;
use renderpilot_storage_sqlite::LocalCleanupOwners;

use crate::{
    Context, ServiceError,
    addons::vulkan_lock::SharedVulkanMutationGuard,
    catalog::installation_lifecycle::residue::{RetiredRootObservation, observe_registered_root},
    context::RetiredLeftoversObservation,
};

use super::{dto::*, issue};

pub(super) struct CapturedObservation {
    pub(super) observation: RetiredLeftoversObservation,
    pub(super) _shared_guard: Option<SharedVulkanMutationGuard>,
}

/// Reads one absent game's canonical owners, root, and existing operation fences.
/// This function is observational: it never enters a recovery or mutation boundary.
pub(super) fn capture_observation(
    context: &Context,
    game: &GameInstallation,
    retain_shared_guard: bool,
) -> Result<Option<CapturedObservation>, ServiceError> {
    let owners = context.storage().read_local_cleanup_owners(game.id())?;
    if owners.game() != game {
        return Err(ServiceError::command_failed(
            "installation identity changed while cleanup was observed",
        ));
    }
    if owners.availability().is_active() {
        return Ok(None);
    }

    let root = observe_registered_root(&owners);
    let storage = context.storage();
    let pending_files = storage.pending_file_mutations_for_game(game.id())?;
    let pending_nvapi = storage
        .list_pending_nvapi_operations()?
        .into_iter()
        .filter(|row| row.game_id.as_deref() == Some(game.id().as_str()))
        .collect::<Vec<_>>();

    // The singleton pending row and native shared state share one process lock.
    // Catalog and game observation locks are already held by the caller.
    let shared_guard = crate::addons::vulkan_lock::blocking_shared_vulkan_lock();
    let pending_shared_vulkan = storage.pending_shared_vulkan_mutation()?;
    let needs_shared_observation =
        has_shared_vulkan_owner(&owners) || pending_shared_vulkan.is_some();
    let native_shared_vulkan = needs_shared_observation.then(observe_standard_shared_vulkan_layer);
    Ok(Some(CapturedObservation {
        observation: RetiredLeftoversObservation {
            owners,
            root,
            pending_files,
            pending_shared_vulkan,
            pending_nvapi,
            native_shared_vulkan,
        },
        _shared_guard: retain_shared_guard.then_some(shared_guard),
    }))
}

pub(super) fn observe_standard_shared_vulkan_layer() -> Result<SharedVulkanLayerObservation, String>
{
    #[cfg(windows)]
    {
        renderpilot_platform_windows::vulkan_layer::observe_standard_shared_vulkan_layer()
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("native shared Vulkan observation is unavailable on this platform".to_owned())
    }
}

pub(super) fn has_shared_vulkan_owner(owners: &LocalCleanupOwners) -> bool {
    owners
        .addon()
        .is_some_and(|addon| addon.host_kind() == Some(InstalledAddonHostKind::SharedVulkanLayer))
}

pub(super) fn root_state(root: &RetiredRootObservation) -> Option<LeftoverRootState> {
    match root {
        RetiredRootObservation::Missing(_) => Some(LeftoverRootState::Missing),
        RetiredRootObservation::ResidueOnly(_) => Some(LeftoverRootState::ResidueOnly),
        RetiredRootObservation::PresentNonResidue { .. }
        | RetiredRootObservation::Indeterminate { .. } => None,
    }
}

pub(super) fn root_issue(root: &RetiredRootObservation) -> LeftoverIssue {
    match root {
        RetiredRootObservation::PresentNonResidue { .. } => issue(
            LeftoverIssueCode::UnprovenOwnership,
            Some("The installation root contains content without a cleanup receipt.".to_owned()),
        ),
        RetiredRootObservation::Indeterminate { reason } => {
            issue(LeftoverIssueCode::UnavailableTarget, Some(reason.clone()))
        }
        RetiredRootObservation::Missing(_) | RetiredRootObservation::ResidueOnly(_) => issue(
            LeftoverIssueCode::ActiveOwnershipConflict,
            Some("The installation is not currently eligible for cleanup.".to_owned()),
        ),
    }
}
