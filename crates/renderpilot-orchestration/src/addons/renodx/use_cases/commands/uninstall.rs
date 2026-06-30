//! Uninstalls RenoDX from a game and cleans shared Vulkan app registration.

use std::path::PathBuf;

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{GameId, InstalledAddon, InstalledAddonHostKind};

use crate::addons::renodx::errors;
use crate::addons::renodx::facts::analyze_game;
use crate::addons::renodx::game_context::{executable_override, require_game};
use crate::addons::renodx::install::uninstall as uninstall_files;
use crate::addons::renodx::operation_lock;
use crate::addons::renodx::policy::{HostKind, host_decision, primary_api};
use crate::addons::renodx::vulkan;
use crate::{Context, ServiceError};

/// Uninstalls RenoDX from a game, restoring files and clearing install metadata.
pub fn uninstall(context: &Context, game_id: &GameId) -> Result<(), ServiceError> {
    let _guard = operation_lock::blocking_lock(game_id);
    let record = context
        .storage()
        .get_installed_addon(game_id)?
        .ok_or_else(errors::not_installed)?;

    // 1. Per-game files: restore the game folder (add-on, ReShade.ini, backups).
    uninstall_files(&record)?;

    // 2. Shared Vulkan layer cleanup: unregister this game's exe from
    //    ReShadeApps.ini. If it was the last app, remove the empty shared
    //    layer (manifest registration + directory). Then forget the advisory
    //    DB row.
    let registered_exe = registered_vulkan_exe_for_uninstall(context, game_id, &record);
    if let Some(exe) = registered_exe.as_deref() {
        let _shared_guard = operation_lock::blocking_shared_vulkan_lock();
        match vulkan::unregister_app(exe) {
            Ok(true) => vulkan::forget_layer_record(context.storage()),
            Ok(false) => {}
            Err(error) => log::warn!("failed to unregister Vulkan layer app: {error}"),
        }
    }

    // 3. Delete the per-game DB row.
    context.storage().delete_installed_addon(game_id)?;
    Ok(())
}

fn registered_vulkan_exe_for_uninstall(
    context: &Context,
    game_id: &GameId,
    record: &InstalledAddon,
) -> Option<PathBuf> {
    match record.host_kind() {
        Some(InstalledAddonHostKind::SharedVulkanLayer) => {
            return record
                .registered_exe_path()
                .map(|path| PathBuf::from(path.as_str()));
        }
        Some(InstalledAddonHostKind::Proxy) => return None,
        None => {}
    }

    legacy_vulkan_exe_for_uninstall(context, game_id)
}

fn legacy_vulkan_exe_for_uninstall(context: &Context, game_id: &GameId) -> Option<PathBuf> {
    let game = require_game(context, game_id).ok()?;
    let analysis = analyze_game(&game, executable_override(context, game_id).as_deref());
    let host_kind = host_decision(primary_api(&analysis.facts.graphics))?;
    if !matches!(host_kind, HostKind::Vulkan) {
        return None;
    }
    analysis
        .primary_executable
        .as_ref()
        .map(|exe| PathBuf::from(exe.as_str()))
}
