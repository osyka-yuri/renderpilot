//! Whole-set recovery for SVAM-v1.

use std::path::Path;

use renderpilot_platform_windows::vulkan_layer::LayerRegistry;
use renderpilot_storage_sqlite::PendingSharedVulkanMutationState;

use super::MutationError;
use super::io;
use super::manifest::{FileAfter, Manifest, Scope};
use super::plan::FilePayload;

/// Reconciles the singleton row while the caller owns the shared process
/// authority (and, for `GameShared`, the game authority in game->shared order).
/// No lock is acquired here: recovery must never wait for a game lock while the
/// shared authority is held.
pub(crate) fn recover_pending(
    context: &crate::Context,
    registry: Option<&dyn LayerRegistry>,
) -> Result<(), crate::ServiceError> {
    let Some(row) = context.storage().pending_shared_vulkan_mutation()? else {
        return Ok(());
    };
    let registry = require_native_registry_authority(registry)?;
    let shared_root = renderpilot_platform_windows::vulkan_layer::reshade_common_dir()
        .ok_or_else(|| crate::ServiceError::invalid_input("shared Vulkan root is unavailable"))?;
    let scope = storage_scope_to_manifest(row.scope);
    let roots = super::TrustedRoots::from_json(&row.root_capabilities_json, scope, &shared_root)?;
    recover_row(context, &row, &roots, registry)
}

/// Uses the already-issued authority of the in-flight forward transaction.
/// The persisted capabilities must still match byte-for-byte; this is the same
/// whole-set recovery algorithm, without re-deriving the platform root after
/// the caller has already proved and retained it.
pub(super) fn recover_pending_with_roots(
    context: &crate::Context,
    roots: &super::TrustedRoots,
    registry: Option<&dyn LayerRegistry>,
) -> Result<(), crate::ServiceError> {
    let Some(row) = context.storage().pending_shared_vulkan_mutation()? else {
        return Ok(());
    };
    let registry = require_native_registry_authority(registry)?;
    if row.root_capabilities_json != roots.to_json()? {
        return Err(crate::ServiceError::invalid_input(
            "shared Vulkan root authority changed during the transaction",
        ));
    }
    recover_row(context, &row, roots, registry)
}

fn recover_row(
    context: &crate::Context,
    row: &renderpilot_storage_sqlite::PendingSharedVulkanMutationRow,
    roots: &super::TrustedRoots,
    registry: &dyn LayerRegistry,
) -> Result<(), crate::ServiceError> {
    let root = super::transaction_root(context.file_mutation_root(), &row.id)?;
    match row.state {
        PendingSharedVulkanMutationState::Preparing => {
            cleanup_preparing_artifacts(&root)?;
            context
                .storage()
                .abandon_shared_vulkan_mutation_preparation(&row.id)?;
            Ok(())
        }
        PendingSharedVulkanMutationState::Committed => {
            let manifest =
                Manifest::from_json(&row.manifest_json).map_err(MutationError::manifest)?;
            manifest
                .validate_for_transaction(&row.id)
                .map_err(MutationError::manifest)?;
            validate_manifest_owner(row, &manifest)?;
            if io::verify_all_after(&root, &manifest, roots, Some(registry)).is_err() {
                // A committed drift is an integrity fence. Do not write or
                // delete anything that could hide it from reconciliation.
                return Err(crate::ServiceError::invalid_input(
                    "committed shared Vulkan mutation no longer matches its manifest",
                ));
            }
            io::cleanup_artifacts(&root, &manifest, roots, io::ParticipantState::After)?;
            context
                .storage()
                .cleanup_committed_shared_vulkan_mutation(&row.id)?;
            Ok(())
        }
        PendingSharedVulkanMutationState::Prepared => {
            recover_prepared(context, row, &root, roots, registry)
        }
    }
}

fn recover_prepared(
    context: &crate::Context,
    row: &renderpilot_storage_sqlite::PendingSharedVulkanMutationRow,
    root: &Path,
    roots: &super::TrustedRoots,
    registry: &dyn LayerRegistry,
) -> Result<(), crate::ServiceError> {
    let manifest = Manifest::from_json(&row.manifest_json).map_err(MutationError::manifest)?;
    manifest
        .validate_for_transaction(&row.id)
        .map_err(MutationError::manifest)?;
    if storage_scope(manifest.scope) != row.scope
        || manifest.game_id.as_deref()
            != row.game_id.as_ref().map(renderpilot_domain::GameId::as_str)
    {
        return Err(crate::ServiceError::invalid_input(
            "shared Vulkan manifest owner does not match its reservation",
        ));
    }
    let payloads = payloads_from_manifest(&manifest, roots)?;
    let participant_registry = (!manifest.registry.is_empty()).then_some(registry);
    let states = io::classify_all(root, &manifest, roots, participant_registry)?;
    let all_before = states
        .iter()
        .all(|state| *state == io::ParticipantState::Before);
    let has_third = states.contains(&io::ParticipantState::Third);
    let game_id = manifest
        .game_id
        .as_deref()
        .map(renderpilot_domain::GameId::new)
        .transpose()
        .map_err(|error| crate::ServiceError::invalid_input(error.to_string()))?;
    let fence = context
        .storage()
        .fence_prepared_shared_vulkan_mutation_resolution(
            &row.id,
            storage_scope(manifest.scope),
            game_id.as_ref(),
        )?;

    if all_before {
        io::cleanup_artifacts(root, &manifest, roots, io::ParticipantState::Before)?;
        context
            .storage()
            .complete_prepared_shared_vulkan_mutation_restored(fence)?;
        return Ok(());
    }
    if has_third {
        // A third state means a target drifted outside this transaction. The
        // fence is intentionally retained and no writes are attempted.
        return Err(crate::ServiceError::invalid_input(
            "prepared shared Vulkan mutation has an unclassified participant state",
        ));
    }

    let registry_was_published_last = !io::deactivates_registry(&manifest);
    if registry_was_published_last {
        restore_registry_before(&manifest, roots, participant_registry)?;
    }
    io::restore_files(root, &manifest, &payloads, roots)?;
    if !registry_was_published_last {
        restore_registry_before(&manifest, roots, participant_registry)?;
    }
    io::verify_all_before(root, &manifest, roots, participant_registry)?;
    io::cleanup_artifacts(root, &manifest, roots, io::ParticipantState::Before)?;
    context
        .storage()
        .complete_prepared_shared_vulkan_mutation_restored(fence)?;
    Ok(())
}

fn require_native_registry_authority(
    registry: Option<&dyn LayerRegistry>,
) -> Result<&dyn LayerRegistry, MutationError> {
    registry.ok_or_else(|| {
        MutationError::conflict("shared Vulkan recovery requires native registry authority")
    })
}

fn restore_registry_before(
    manifest: &Manifest,
    roots: &super::TrustedRoots,
    registry: Option<&dyn LayerRegistry>,
) -> Result<(), MutationError> {
    let Some(registry) = registry else {
        return Ok(());
    };
    io::restore_registry(registry, manifest, roots, true)
}

fn payloads_from_manifest(
    manifest: &Manifest,
    roots: &super::TrustedRoots,
) -> Result<Vec<FilePayload>, MutationError> {
    manifest
        .files
        .iter()
        .map(|participant| {
            Ok(FilePayload {
                stage_path: participant
                    .stage_path
                    .as_ref()
                    .map(|path| roots.resolve(path))
                    .transpose()?,
                tomb_path: participant
                    .tomb_path
                    .as_ref()
                    .map(|path| roots.resolve(path))
                    .transpose()?,
                bytes: match participant.after {
                    FileAfter::Absent => None,
                    FileAfter::Present { .. } => None,
                },
            })
        })
        .collect()
}

pub(super) fn cleanup_preparing_artifacts(root: &Path) -> Result<(), MutationError> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(MutationError::io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(MutationError::conflict(
            "preparing transaction root is not an owned directory",
        ));
    }
    let entries = std::fs::read_dir(root)
        .map_err(MutationError::io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(MutationError::io)?;
    if entries.is_empty() {
        return std::fs::remove_dir(root).map_err(MutationError::io);
    }
    if entries.len() != 1 || entries[0].file_name() != "snapshots" {
        return Err(MutationError::conflict(
            "preparing transaction contains an unowned entry",
        ));
    }
    let snapshots = entries[0].path();
    let metadata = std::fs::symlink_metadata(&snapshots).map_err(MutationError::io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(MutationError::conflict(
            "preparing snapshot container is not an owned directory",
        ));
    }
    let files = std::fs::read_dir(&snapshots)
        .map_err(MutationError::io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(MutationError::io)?;
    for entry in &files {
        let metadata = std::fs::symlink_metadata(entry.path()).map_err(MutationError::io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(MutationError::conflict(
                "preparing transaction contains a non-UTF-8 snapshot name",
            ));
        };
        if !metadata.is_file() || !is_preparing_snapshot_name(name) {
            return Err(MutationError::conflict(
                "preparing transaction contains an unowned snapshot entry",
            ));
        }
    }
    for entry in files {
        std::fs::remove_file(entry.path()).map_err(MutationError::io)?;
    }
    std::fs::remove_dir(&snapshots).map_err(MutationError::io)?;
    std::fs::remove_dir(root).map_err(MutationError::io)
}

fn is_preparing_snapshot_name(name: &str) -> bool {
    name.strip_prefix("file-")
        .and_then(|value| value.strip_suffix(".bin"))
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
}

fn storage_scope(scope: Scope) -> renderpilot_storage_sqlite::SharedVulkanMutationScope {
    match scope {
        Scope::SharedOnly => renderpilot_storage_sqlite::SharedVulkanMutationScope::SharedOnly,
        Scope::GameShared => renderpilot_storage_sqlite::SharedVulkanMutationScope::GameShared,
    }
}

fn storage_scope_to_manifest(
    scope: renderpilot_storage_sqlite::SharedVulkanMutationScope,
) -> Scope {
    match scope {
        renderpilot_storage_sqlite::SharedVulkanMutationScope::SharedOnly => Scope::SharedOnly,
        renderpilot_storage_sqlite::SharedVulkanMutationScope::GameShared => Scope::GameShared,
    }
}

fn validate_manifest_owner(
    row: &renderpilot_storage_sqlite::PendingSharedVulkanMutationRow,
    manifest: &Manifest,
) -> Result<(), crate::ServiceError> {
    if storage_scope(manifest.scope) != row.scope
        || manifest.game_id.as_deref()
            != row.game_id.as_ref().map(renderpilot_domain::GameId::as_str)
    {
        return Err(crate::ServiceError::invalid_input(
            "shared Vulkan manifest owner does not match its reservation",
        ));
    }
    Ok(())
}
