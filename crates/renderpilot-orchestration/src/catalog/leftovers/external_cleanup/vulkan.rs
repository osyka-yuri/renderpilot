use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(any(windows, test))]
use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind,
    mutation_features::RETIRED_GAME_LEFTOVERS_UNREGISTER, normalized_path_key,
};
use renderpilot_platform_windows::vulkan_layer::{
    AppListChange, SharedVulkanLayerObservation, plan_unregister_app,
};
#[cfg(any(windows, test))]
use renderpilot_platform_windows::vulkan_layer::{
    AppUnregisterOutcome, LayerRegistry, observe_shared_vulkan_layer, plan_unregister_app_only,
};
#[cfg(any(windows, test))]
use renderpilot_storage_sqlite::SharedArtifactMutation;
use renderpilot_storage_sqlite::{
    LocalCleanupOwners, PendingSharedVulkanMutationRow, PendingSharedVulkanMutationState,
    SharedVulkanMutationScope,
};
use sha2::{Digest, Sha256};

use crate::{
    Context, ServiceError,
    addons::{
        shared_vulkan_mutation::{Manifest, Scope, TrustedRoots},
        vulkan_lock::SharedVulkanMutationGuard,
    },
    catalog::leftovers::dto::CleanupStep,
    context::RetiredLeftoversObservation,
    game_mutation_lock::GameMutationGuard,
};
#[cfg(any(windows, test))]
use crate::{
    addons::shared_vulkan_mutation::{
        CatalogProjection, MutationIdentity, PhysicalParticipants, Request, ScopeSpec, compose,
        execute, recover_pending,
    },
    catalog::leftovers::dto::{CleanupStepOutcome, LeftoverCategory},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::catalog::leftovers) enum VulkanPendingClass {
    None,
    OwnPrepared,
    OwnCommitted,
    Foreign,
}

#[cfg(any(windows, test))]
struct VulkanAuthority<'a> {
    layer_dir: &'a Path,
    registry: &'a dyn LayerRegistry,
}

pub(in crate::catalog::leftovers) fn unregister_operation_id(
    game_id: &GameId,
    availability_revision: u64,
    exe_path: &str,
) -> String {
    let identity = format!(
        "{}\n{}\n{}",
        game_id.as_str(),
        availability_revision,
        normalized_path_key(exe_path)
    );
    format!(
        "leftovers-vulkan-{}",
        hex::encode(Sha256::digest(identity.as_bytes()))
    )
}

/// Recognizes only this feature's ordinary app-list-only SVAM retries. A
/// preparing reservation, extra native participant, wrong scope/root, or a
/// changed before/after app-list image remains foreign and is never recovered.
pub(in crate::catalog::leftovers) fn classify_vulkan_pending(
    context: &Context,
    observation: &RetiredLeftoversObservation,
) -> VulkanPendingClass {
    let Some(row) = observation.pending_shared_vulkan.as_ref() else {
        return VulkanPendingClass::None;
    };
    let Some(Ok(native)) = observation.native_shared_vulkan.as_ref() else {
        return VulkanPendingClass::Foreign;
    };
    let owners = &observation.owners;
    let game = owners.game();
    let Some(addon) = owners.addon() else {
        if row.state != PendingSharedVulkanMutationState::Committed
            || owners.engine_journal().is_some()
        {
            return VulkanPendingClass::Foreign;
        }
        if validate_row(
            context,
            row,
            game.id(),
            owners.availability().revision(),
            None,
            &native.layer_dir,
            native,
        )
        .is_err()
        {
            return VulkanPendingClass::Foreign;
        }
        return VulkanPendingClass::OwnCommitted;
    };
    if !is_shared_vulkan_owner(addon) || owners.engine_journal().is_some() {
        return VulkanPendingClass::Foreign;
    }
    let Some(exe) = addon.registered_exe_path() else {
        return VulkanPendingClass::Foreign;
    };
    if validate_row(
        context,
        row,
        game.id(),
        owners.availability().revision(),
        Some(exe.as_str()),
        &native.layer_dir,
        native,
    )
    .is_err()
    {
        return VulkanPendingClass::Foreign;
    }
    match row.state {
        PendingSharedVulkanMutationState::Prepared => VulkanPendingClass::OwnPrepared,
        PendingSharedVulkanMutationState::Committed
        | PendingSharedVulkanMutationState::Preparing => VulkanPendingClass::Foreign,
    }
}

/// Removes this game's `ReShadeApps.ini` association with the ordinary SVAM
/// transaction, keeping the global ReShade DLL, manifest, registry, and layer
/// directory even when this is the final application.
pub(in crate::catalog::leftovers) fn unregister_vulkan_owner(
    context: &Context,
    game_guard: &GameMutationGuard,
    shared_guard: &SharedVulkanMutationGuard,
    expected: &LocalCleanupOwners,
    observation: &RetiredLeftoversObservation,
    item_id: &str,
) -> Result<CleanupStep, ServiceError> {
    #[cfg(windows)]
    {
        let layer_dir = renderpilot_platform_windows::vulkan_layer::reshade_common_dir()
            .ok_or_else(|| {
                ServiceError::invalid_input("shared Vulkan layer location is unavailable")
            })?;
        let registry =
            crate::addons::renodx::platform::vulkan::native_registry().ok_or_else(|| {
                ServiceError::invalid_input("shared Vulkan registry authority is unavailable")
            })?;
        let authority = VulkanAuthority {
            layer_dir: &layer_dir,
            registry,
        };
        unregister_vulkan_owner_with_authority(
            context,
            game_guard,
            shared_guard,
            expected,
            observation,
            item_id,
            &authority,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (
            context,
            game_guard,
            shared_guard,
            expected,
            observation,
            item_id,
        );
        Err(ServiceError::invalid_input(
            "shared Vulkan cleanup is unavailable on this platform",
        ))
    }
}

#[cfg(any(windows, test))]
fn unregister_vulkan_owner_with_authority(
    context: &Context,
    game_guard: &GameMutationGuard,
    _shared_guard: &SharedVulkanMutationGuard,
    expected: &LocalCleanupOwners,
    observation: &RetiredLeftoversObservation,
    item_id: &str,
    authority: &VulkanAuthority<'_>,
) -> Result<CleanupStep, ServiceError> {
    let layer_dir = authority.layer_dir;
    let registry = authority.registry;
    let game = expected.game();
    let game_id = game.id();
    if game_guard.game_id() != game_id || !expected.availability().is_absent() {
        return Err(ServiceError::invalid_input(
            "shared Vulkan cleanup requires the same absent registered game",
        ));
    }
    if let Some(row) = observation.pending_shared_vulkan.as_ref()
        && expected.addon().is_none()
    {
        return recover_committed_without_owner(
            context,
            game_guard,
            expected,
            observation,
            row,
            item_id,
            authority,
        );
    }
    let addon = expected
        .addon()
        .filter(|addon| is_shared_vulkan_owner(addon))
        .ok_or_else(|| {
            ServiceError::invalid_input("shared Vulkan owner changed after cleanup was proposed")
        })?;
    let exe = addon.registered_exe_path().ok_or_else(|| {
        ServiceError::invalid_input("shared Vulkan owner has no executable binding")
    })?;
    let expected_item = super::super::projection::canonical_item_id(
        game_id,
        LeftoverCategory::VulkanRegistration,
        Some(exe.as_str()),
        "shared-vulkan-registration",
    );
    let pending = observation.pending_shared_vulkan.as_ref();
    let pending_item = pending.map(|row| pending_item_id(game_id, row));
    if item_id != expected_item && pending_item.as_deref() != Some(item_id) {
        return Err(ServiceError::invalid_input(
            "shared Vulkan cleanup item does not match the current owner",
        ));
    }
    let current = context.storage().read_local_cleanup_owners(game_id)?;
    if current.game() != game
        || current.availability() != expected.availability()
        || current.addon() != Some(addon)
        || current.engine_journal().is_some()
    {
        return Err(ServiceError::invalid_input(
            "shared Vulkan owner or Engine.ini receipt changed before cleanup",
        ));
    }
    let observed_native = observe_shared_vulkan_layer(registry, layer_dir)
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    if !native_target_signature_matches(
        observation.native_shared_vulkan.as_ref(),
        &observed_native,
        exe.as_str(),
    ) {
        return Err(ServiceError::invalid_input(
            "this game's shared Vulkan registration changed after proposal",
        ));
    }

    let operation_id =
        unregister_operation_id(game_id, expected.availability().revision(), exe.as_str());
    match pending.map(|row| row.state) {
        Some(PendingSharedVulkanMutationState::Preparing) => {
            return Err(ServiceError::invalid_input(
                "shared Vulkan preparation is not an eligible cleanup retry",
            ));
        }
        Some(PendingSharedVulkanMutationState::Prepared)
        | Some(PendingSharedVulkanMutationState::Committed) => {
            if context.storage().pending_shared_vulkan_mutation()?.as_ref() != pending {
                return Err(ServiceError::invalid_input(
                    "shared Vulkan pending row changed after proposal",
                ));
            }
            let class = classify_vulkan_pending(context, observation);
            if !matches!(
                class,
                VulkanPendingClass::OwnPrepared | VulkanPendingClass::OwnCommitted
            ) {
                return Err(ServiceError::invalid_input(
                    "shared Vulkan pending row is not this app-list-only cleanup",
                ));
            }
            recover_pending(context, Some(registry))?;
            let after_recovery = context.storage().read_local_cleanup_owners(game_id)?;
            if after_recovery.game() != game || !after_recovery.availability().is_absent() {
                return Err(ServiceError::invalid_input(
                    "registered game changed during shared Vulkan recovery",
                ));
            }
            if after_recovery.addon().is_none() {
                return Ok(step(item_id, CleanupStepOutcome::Recovered));
            }
            if after_recovery.addon() != Some(addon) {
                return Err(ServiceError::invalid_input(
                    "shared Vulkan owner changed during pending recovery",
                ));
            }
        }
        None => {
            if context
                .storage()
                .pending_shared_vulkan_mutation()?
                .is_some()
            {
                return Err(ServiceError::invalid_input(
                    "a shared Vulkan transaction appeared after cleanup proposal",
                ));
            }
        }
    }

    let native = observe_shared_vulkan_layer(registry, layer_dir)
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let plan = plan_unregister_app_only(native, Path::new(exe.as_str()))
        .map_err(|error| ServiceError::invalid_input(error.to_string()))?;
    match plan.unregister_outcome {
        Some(AppUnregisterOutcome::TargetAbsent) => {
            crate::file_mutation::ensure_feature_allowed_with_proxy_topology(
                context,
                game_id,
                RETIRED_GAME_LEFTOVERS_UNREGISTER,
            )?;
            let fresh = context.storage().read_local_cleanup_owners(game_id)?;
            if fresh.game() != game
                || fresh.availability() != expected.availability()
                || fresh.addon() != Some(addon)
                || fresh.engine_journal().is_some()
                || context
                    .storage()
                    .pending_shared_vulkan_mutation()?
                    .is_some()
            {
                return Err(ServiceError::invalid_input(
                    "shared Vulkan owner changed before metadata release",
                ));
            }
            context
                .storage()
                .delete_installed_addon(game_id, addon.kind())?;
            Ok(step(item_id, CleanupStepOutcome::Released))
        }
        Some(AppUnregisterOutcome::RemovedOthersRemain | AppUnregisterOutcome::RemovedLast) => {
            let shared_plan = plan;
            let mut participants = compose(None, Some(shared_plan))?;
            let apps_path = layer_dir.join("ReShadeApps.ini");
            participants
                .files
                .retain(|file| crate::paths::same_path(&file.live_path, &apps_path));
            participants.registry.clear();
            participants.created_dirs.clear();
            if participants.files.len() != 1 {
                return Err(ServiceError::invalid_input(
                    "the shared Vulkan plan does not contain exactly one app-list change",
                ));
            }
            if context
                .storage()
                .pending_shared_vulkan_mutation()?
                .is_some()
            {
                return Err(ServiceError::invalid_input(
                    "a shared Vulkan transaction appeared before app-list commit",
                ));
            }
            let roots = TrustedRoots::game_shared_without_game_files(layer_dir)?;
            let physical = PhysicalParticipants::new(roots, participants, Some(registry));
            let request = Request::new(
                context,
                MutationIdentity::new(
                    &operation_id,
                    ScopeSpec::game_delete(game_id, addon.kind()),
                    RETIRED_GAME_LEFTOVERS_UNREGISTER,
                ),
                physical,
                CatalogProjection::new(SharedArtifactMutation::Keep),
            );
            execute(request)?;
            Ok(step(item_id, CleanupStepOutcome::Released))
        }
        Some(AppUnregisterOutcome::Indeterminate) | None => Err(ServiceError::invalid_input(
            "the app list does not prove this game's registration",
        )),
    }
}

#[cfg(any(windows, test))]
fn recover_committed_without_owner(
    context: &Context,
    game_guard: &GameMutationGuard,
    expected: &LocalCleanupOwners,
    observation: &RetiredLeftoversObservation,
    row: &PendingSharedVulkanMutationRow,
    item_id: &str,
    authority: &VulkanAuthority<'_>,
) -> Result<CleanupStep, ServiceError> {
    let layer_dir = authority.layer_dir;
    let registry = authority.registry;
    let game = expected.game();
    if game_guard.game_id() != game.id()
        || !expected.availability().is_absent()
        || expected.engine_journal().is_some()
        || row.state != PendingSharedVulkanMutationState::Committed
        || item_id != pending_item_id(game.id(), row)
    {
        return Err(ServiceError::invalid_input(
            "committed shared Vulkan cleanup no longer matches its owner",
        ));
    }
    let current = context.storage().read_local_cleanup_owners(game.id())?;
    if current.game() != game
        || current.availability() != expected.availability()
        || current.addon().is_some()
        || current.engine_journal().is_some()
        || context.storage().pending_shared_vulkan_mutation()?.as_ref() != Some(row)
    {
        return Err(ServiceError::invalid_input(
            "committed shared Vulkan owner changed before recovery",
        ));
    }
    let native = observe_shared_vulkan_layer(registry, layer_dir)
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let mut fresh = observation.clone();
    fresh.native_shared_vulkan = Some(Ok(native));
    if classify_vulkan_pending(context, &fresh) != VulkanPendingClass::OwnCommitted {
        return Err(ServiceError::invalid_input(
            "committed shared Vulkan row is not this app-list-only cleanup",
        ));
    }
    recover_pending(context, Some(registry))?;
    let after = context.storage().read_local_cleanup_owners(game.id())?;
    if after.game() != game
        || after.availability() != expected.availability()
        || after.addon().is_some()
        || after.engine_journal().is_some()
        || context
            .storage()
            .pending_shared_vulkan_mutation()?
            .is_some()
    {
        return Err(ServiceError::invalid_input(
            "committed shared Vulkan recovery did not finish its exact cleanup",
        ));
    }
    Ok(step(item_id, CleanupStepOutcome::Recovered))
}

fn validate_row(
    context: &Context,
    row: &PendingSharedVulkanMutationRow,
    game_id: &GameId,
    availability_revision: u64,
    owner_exe: Option<&str>,
    layer_dir: &Path,
    native: &SharedVulkanLayerObservation,
) -> Result<(), ServiceError> {
    if row.scope != SharedVulkanMutationScope::GameShared
        || row.game_id.as_ref() != Some(game_id)
        || row.feature != RETIRED_GAME_LEFTOVERS_UNREGISTER
    {
        return Err(ServiceError::invalid_input("foreign shared Vulkan row"));
    }
    let roots = TrustedRoots::game_shared_without_game_files(layer_dir)?;
    if row.root_capabilities_json != roots.to_json()? {
        return Err(ServiceError::invalid_input(
            "shared Vulkan row has different root authority",
        ));
    }
    let manifest = Manifest::from_json(&row.manifest_json)
        .map_err(|error| ServiceError::invalid_input(error.to_string()))?;
    manifest
        .validate_for_transaction(&row.id)
        .map_err(|error| ServiceError::invalid_input(error.to_string()))?;
    if manifest.scope != Scope::GameShared
        || manifest.game_id.as_deref() != Some(game_id.as_str())
        || manifest.feature != RETIRED_GAME_LEFTOVERS_UNREGISTER
        || !manifest.registry.is_empty()
        || !manifest.directories.is_empty()
        || manifest.peer_program.is_some()
        || manifest.files.len() != 1
    {
        return Err(ServiceError::invalid_input(
            "shared Vulkan manifest has extra participants",
        ));
    }
    let value: serde_json::Value = serde_json::from_str(&row.manifest_json)
        .map_err(|error| ServiceError::invalid_input(error.to_string()))?;
    let file = value["files"].get(0).ok_or_else(|| {
        ServiceError::invalid_input("shared Vulkan manifest is missing its app-list participant")
    })?;
    if file["live_path"]["root"].as_str() != Some("shared")
        || file["live_path"]["relative"].as_str() != Some("ReShadeApps.ini")
        || file["stage_path"]["root"].as_str() != Some("shared")
        || file["tomb_path"].as_str().is_some()
        || file["after"]["Present"].is_null()
    {
        return Err(ServiceError::invalid_input(
            "shared Vulkan participant is not app-list-only",
        ));
    }
    let before = &file["before"]["Snapshot"];
    let snapshot_path = before["snapshot_path"].as_str();
    let before_hash = before["sha256"].as_str();
    let before_len = before["len"].as_u64();
    let after_hash = file["after"]["Present"]["sha256"].as_str();
    let after_len = file["after"]["Present"]["len"].as_u64();
    if snapshot_path != Some("snapshots/file-0.bin") {
        return Err(ServiceError::invalid_input(
            "app-list snapshot is not canonical",
        ));
    }
    let transaction_root = crate::addons::shared_vulkan_mutation::transaction_root(
        context.file_mutation_root(),
        &row.id,
    )?;
    let snapshot = transaction_root.join("snapshots").join("file-0.bin");
    let current_bytes = match &native.apps {
        renderpilot_platform_windows::vulkan_layer::FileObservation::Present(bytes) => bytes,
        renderpilot_platform_windows::vulkan_layer::FileObservation::Absent => {
            return Err(ServiceError::invalid_input(
                "pending app-list target disappeared",
            ));
        }
    };
    let snapshot_exists = match fs::symlink_metadata(&snapshot) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => true,
        Ok(_) => {
            return Err(ServiceError::invalid_input(
                "app-list snapshot is not a regular file",
            ));
        }
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && row.state == PendingSharedVulkanMutationState::Committed
                && owner_exe.is_none() =>
        {
            false
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ServiceError::invalid_input(
                "app-list snapshot is required for this retry",
            ));
        }
        Err(error) => return Err(ServiceError::command_failed(error.to_string())),
    };
    if !snapshot_exists {
        let after_bytes = current_bytes;
        if Some(after_bytes.len() as u64) != after_len
            || after_hash != Some(digest(after_bytes).as_str())
        {
            return Err(ServiceError::invalid_input(
                "committed app-list is not at its exact postimage",
            ));
        }
        renderpilot_platform_windows::vulkan_layer::parse_app_list(after_bytes)
            .map_err(|error| ServiceError::invalid_input(error.to_string()))?;
        return Ok(());
    }
    let before_bytes =
        fs::read(&snapshot).map_err(|error| ServiceError::command_failed(error.to_string()))?;
    if Some(before_bytes.len() as u64) != before_len
        || before_hash != Some(digest(&before_bytes).as_str())
    {
        return Err(ServiceError::invalid_input(
            "app-list snapshot digest changed",
        ));
    }
    let before_apps = renderpilot_platform_windows::vulkan_layer::parse_app_list(&before_bytes)
        .map_err(|error| ServiceError::invalid_input(error.to_string()))?;
    let candidate = if let Some(exe) = owner_exe {
        PathBuf::from(exe)
    } else {
        let removed = before_apps
            .iter()
            .filter(|path| {
                !native_apps(native)
                    .iter()
                    .any(|current| crate::paths::same_path(current, path))
            })
            .collect::<Vec<_>>();
        let mut unique = removed
            .iter()
            .map(|path| normalized_path_key(&path.to_string_lossy()))
            .collect::<Vec<_>>();
        unique.sort();
        unique.dedup();
        if unique.len() != 1 {
            return Err(ServiceError::invalid_input(
                "committed app-list retry does not identify one removed game",
            ));
        }
        PathBuf::from(removed[0].as_os_str())
    };
    let app_plan = plan_unregister_app(Some(&before_bytes), &candidate)
        .map_err(|error| ServiceError::invalid_input(error.to_string()))?;
    let AppListChange::Replacement(after_bytes) = app_plan.change else {
        return Err(ServiceError::invalid_input(
            "app-list retry contains no actual removal",
        ));
    };
    if Some(after_bytes.len() as u64) != after_len
        || after_hash != Some(digest(&after_bytes).as_str())
    {
        return Err(ServiceError::invalid_input(
            "app-list postimage is not the exact unregister plan",
        ));
    }
    match row.state {
        PendingSharedVulkanMutationState::Prepared => {
            if current_bytes != &before_bytes && current_bytes != &after_bytes {
                return Err(ServiceError::invalid_input(
                    "pending app-list matches neither exact image",
                ));
            }
            if owner_exe.is_some_and(|exe| !crate::paths::same_path(Path::new(exe), &candidate)) {
                return Err(ServiceError::invalid_input(
                    "pending app-list removes another game's executable",
                ));
            }
            if row.id
                != unregister_operation_id(
                    game_id,
                    availability_revision,
                    &candidate.to_string_lossy(),
                )
            {
                return Err(ServiceError::invalid_input(
                    "pending app-list operation id is not canonical",
                ));
            }
        }
        PendingSharedVulkanMutationState::Committed => {
            if current_bytes != &after_bytes || owner_exe.is_some() {
                return Err(ServiceError::invalid_input(
                    "committed app-list is not at its exact postimage",
                ));
            }
            if row.id
                != unregister_operation_id(
                    game_id,
                    availability_revision,
                    &candidate.to_string_lossy(),
                )
            {
                return Err(ServiceError::invalid_input(
                    "committed app-list operation id is not canonical",
                ));
            }
        }
        PendingSharedVulkanMutationState::Preparing => {
            return Err(ServiceError::invalid_input("preparing shared Vulkan row"));
        }
    }
    Ok(())
}

fn native_apps(native: &SharedVulkanLayerObservation) -> Vec<PathBuf> {
    match &native.apps {
        renderpilot_platform_windows::vulkan_layer::FileObservation::Present(bytes) => {
            renderpilot_platform_windows::vulkan_layer::parse_app_list(bytes).unwrap_or_default()
        }
        renderpilot_platform_windows::vulkan_layer::FileObservation::Absent => Vec::new(),
    }
}

#[cfg(any(windows, test))]
fn native_target_signature_matches(
    before: Option<&Result<SharedVulkanLayerObservation, String>>,
    current: &SharedVulkanLayerObservation,
    exe: &str,
) -> bool {
    let Some(Ok(before)) = before else {
        return false;
    };
    if !crate::paths::same_path(&before.layer_dir, &current.layer_dir) {
        return false;
    }
    fn selected(native: &SharedVulkanLayerObservation, exe: &str) -> Option<Vec<String>> {
        let bytes = match &native.apps {
            renderpilot_platform_windows::vulkan_layer::FileObservation::Present(bytes) => bytes,
            renderpilot_platform_windows::vulkan_layer::FileObservation::Absent => {
                return Some(Vec::new());
            }
        };
        let mut matches = renderpilot_platform_windows::vulkan_layer::parse_app_list(bytes)
            .ok()?
            .into_iter()
            .filter(|path| crate::paths::same_path(path, Path::new(exe)))
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        matches.sort_by_key(|path| normalized_path_key(path));
        Some(matches)
    }
    selected(before, exe) == selected(current, exe)
}

fn is_shared_vulkan_owner(addon: &InstalledAddon) -> bool {
    addon.kind() == AddonKind::RenoDx
        && addon.host_kind() == Some(InstalledAddonHostKind::SharedVulkanLayer)
}

#[cfg(any(windows, test))]
fn pending_item_id(game_id: &GameId, row: &PendingSharedVulkanMutationRow) -> String {
    super::super::projection::canonical_item_id(
        game_id,
        LeftoverCategory::VulkanRegistration,
        None,
        &format!("pending-shared:{}", row.id),
    )
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(any(windows, test))]
fn step(item_id: &str, outcome: CleanupStepOutcome) -> CleanupStep {
    CleanupStep {
        item_id: item_id.to_owned(),
        category: LeftoverCategory::VulkanRegistration,
        outcome,
        issue: None,
    }
}

#[cfg(test)]
mod tests;
