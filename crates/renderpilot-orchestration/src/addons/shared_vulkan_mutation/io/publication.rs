//! Ordered publication and rollback of declared SVAM participants.

use std::fs;
use std::path::Path;

use renderpilot_platform_windows::vulkan_layer::{LayerRegistry, RegistryValueState};

use super::super::manifest::{FileAfter, FileBefore, Manifest, RegistryValue};
use super::super::plan;
use super::super::{MutationError, TrustedRoots};
use super::observation::{
    digest_hex, file_state, matches_file_after, matches_file_before, matches_file_before_strict,
    observe_registry, read_verified_snapshot,
};

/// A deactivation must become invisible to the Vulkan loader before its files
/// are removed. Activation uses the opposite order: publish files first, then
/// expose the registry value.
pub(crate) fn deactivates_registry(manifest: &Manifest) -> bool {
    manifest.registry.iter().any(|participant| {
        matches!(&participant.before, RegistryValue::Present { .. })
            && matches!(&participant.after, RegistryValue::Absent)
    })
}

pub(crate) fn apply_files(
    transaction_root: &Path,
    manifest: &Manifest,
    payloads: &[plan::FilePayload],
    roots: &TrustedRoots,
) -> Result<(), MutationError> {
    for (participant, payload) in manifest.files.iter().zip(payloads) {
        let live = roots.resolve(&participant.live_path)?;
        let actual = file_state(&live).map_err(MutationError::io)?;
        if !matches_file_before_strict(transaction_root, participant, actual.as_deref())? {
            return Err(MutationError::conflict(format!(
                "file drifted immediately before publishing shared Vulkan transaction: {}",
                live.display()
            )));
        }
        match &participant.after {
            FileAfter::Present { sha256, len } => {
                let stage = payload
                    .stage_path
                    .as_deref()
                    .ok_or_else(|| MutationError::conflict("present target missing stage"))?;
                let bytes = fs::read(stage).map_err(MutationError::io)?;
                if bytes.len() as u64 != *len || digest_hex(&bytes) != *sha256 {
                    return Err(MutationError::conflict(format!(
                        "stage digest mismatch: {}",
                        stage.display()
                    )));
                }
                crate::fs::publish_staged_replace(stage, &live).map_err(MutationError::Service)?;
            }
            FileAfter::Absent => {
                let tomb = payload
                    .tomb_path
                    .as_deref()
                    .ok_or_else(|| MutationError::conflict("absent target missing tomb"))?;
                crate::fs::move_file_no_replace(&live, tomb).map_err(MutationError::Service)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn restore_files(
    transaction_root: &Path,
    manifest: &Manifest,
    payloads: &[plan::FilePayload],
    roots: &TrustedRoots,
) -> Result<(), MutationError> {
    for (participant, payload) in manifest.files.iter().zip(payloads).rev() {
        let live = roots.resolve(&participant.live_path)?;
        let actual = file_state(&live).map_err(MutationError::io)?;
        if matches_file_before(transaction_root, participant, actual.as_deref())? {
            continue;
        }
        if !matches_file_after(participant, actual.as_deref()) {
            return Err(MutationError::conflict(format!(
                "file drifted while restoring shared Vulkan transaction: {}",
                live.display()
            )));
        }
        if let Some(tomb) = payload.tomb_path.as_deref()
            && actual.is_none()
            && let Some(tomb_bytes) = file_state(tomb).map_err(MutationError::io)?
        {
            let FileBefore::Snapshot {
                snapshot_path,
                sha256,
                len,
            } = &participant.before
            else {
                return Err(MutationError::conflict(
                    "deletion tomb has no matching before snapshot",
                ));
            };
            let before = read_verified_snapshot(transaction_root, snapshot_path, sha256, *len)?;
            if tomb_bytes != before {
                return Err(MutationError::conflict(format!(
                    "deletion tomb drifted before restoration: {}",
                    tomb.display()
                )));
            }
            crate::fs::move_file_no_replace(tomb, &live).map_err(MutationError::Service)?;
            let restored = file_state(&live).map_err(MutationError::io)?;
            if !matches_file_before_strict(transaction_root, participant, restored.as_deref())? {
                return Err(MutationError::conflict(format!(
                    "restored deletion target failed verification: {}",
                    live.display()
                )));
            }
            continue;
        }
        match &participant.before {
            FileBefore::Absent => {
                if live.exists() {
                    fs::remove_file(live).map_err(MutationError::io)?;
                }
            }
            FileBefore::Snapshot {
                snapshot_path,
                sha256,
                len,
            } => {
                let bytes = read_verified_snapshot(transaction_root, snapshot_path, sha256, *len)?;
                crate::fs::write_file_atomically(&live, &bytes).map_err(MutationError::Service)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn restore_registry(
    registry: &dyn LayerRegistry,
    manifest: &Manifest,
    roots: &TrustedRoots,
    before: bool,
) -> Result<(), MutationError> {
    for participant in manifest.registry.iter().rev() {
        let value = if before {
            &participant.before
        } else {
            &participant.after
        };
        let manifest_path = roots.resolve(&participant.manifest_path)?;
        let current = observe_registry(registry, &manifest_path)?;
        if current == *value {
            continue;
        }
        let opposite = if before {
            &participant.after
        } else {
            &participant.before
        };
        if current != *opposite {
            return Err(MutationError::conflict(format!(
                "registry participant drifted immediately before restoration: {}",
                manifest_path.display()
            )));
        }
        let state = match value {
            RegistryValue::Absent => RegistryValueState::Absent,
            RegistryValue::Present {
                value_type,
                raw_bytes,
            } => RegistryValueState::Present {
                value_type: *value_type,
                raw_bytes: raw_bytes.clone(),
            },
        };
        registry
            .restore_canonical_registration(&manifest_path, &state)
            .map_err(MutationError::io)?;
    }
    Ok(())
}
