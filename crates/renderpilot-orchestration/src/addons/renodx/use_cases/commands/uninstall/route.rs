use renderpilot_application::ProxyTopologyRepository;
use renderpilot_domain::{AddonKind, GameId, InstalledAddon, InstalledAddonHostKind};

use crate::addons::records;
use crate::addons::renodx::errors;
use crate::{Context, ServiceError};

pub(super) fn uninstall(context: &Context, game_id: &GameId) -> Result<(), ServiceError> {
    loop {
        let guard = crate::mutation_boundary::enter_game_mutation_boundary(context, game_id)?;
        let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
            .ok_or_else(errors::not_installed)?;
        if record.host_kind() == Some(InstalledAddonHostKind::SharedVulkanLayer) {
            let executable = registered_vulkan_exe_for_uninstall(&record).ok_or_else(|| {
                ServiceError::invalid_input(
                    "active shared RenoDX uninstall has no registered executable",
                )
            })?;
            drop(guard);

            let guards =
                crate::mutation_boundary::enter_game_shared_mutation_boundary(context, game_id)?;
            let current = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
                .ok_or_else(errors::not_installed)?;
            if current.host_kind() != Some(InstalledAddonHostKind::SharedVulkanLayer)
                || registered_vulkan_exe_for_uninstall(&current) != Some(executable)
            {
                drop(guards);
                continue;
            }
            if context.storage().get_proxy_topology(game_id)?.is_some() {
                return super::shared::uninstall_shared_locked(context, &guards, game_id, &current);
            }
            return super::inactive::uninstall_shared_locked(context, &guards, game_id, &current);
        } else if context.storage().get_proxy_topology(game_id)?.is_some() {
            return super::active::uninstall_locked(context, &guard, game_id);
        }
        return super::inactive::uninstall_locked(context, &guard, game_id);
    }
}

pub(super) fn uninstall_shared_locked(
    context: &Context,
    guards: &crate::mutation_boundary::GameSharedMutationGuards,
    game_id: &GameId,
    record: &InstalledAddon,
) -> Result<(), ServiceError> {
    if context.storage().get_proxy_topology(game_id)?.is_some() {
        super::shared::uninstall_shared_locked(context, guards, game_id, record)
    } else {
        super::inactive::uninstall_shared_locked(context, guards, game_id, record)
    }
}

pub(super) fn uninstall_locked(
    context: &Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
) -> Result<(), ServiceError> {
    if guard.game_id() != game_id {
        return Err(ServiceError::invalid_input(
            "RenoDX uninstall guard does not match the requested game",
        ));
    }
    if context.storage().get_proxy_topology(game_id)?.is_some() {
        super::active::uninstall_locked(context, guard, game_id)
    } else {
        super::inactive::uninstall_locked(context, guard, game_id)
    }
}

pub(super) fn registered_vulkan_exe_for_uninstall(
    record: &InstalledAddon,
) -> Option<std::path::PathBuf> {
    match record.host_kind() {
        Some(InstalledAddonHostKind::SharedVulkanLayer) => record
            .registered_exe_path()
            .map(|path| std::path::PathBuf::from(path.as_str())),
        Some(InstalledAddonHostKind::Proxy) | None => None,
    }
}
