//! Before/after fences and whole-set state classification.

use std::path::Path;

use renderpilot_platform_windows::vulkan_layer::LayerRegistry;

use super::super::manifest::{FileAfter, FileBefore, Manifest};
use super::super::{MutationError, TrustedRoots};
use super::observation::{
    digest_hex, file_state, matches_file_after, matches_file_before, observe_registry,
    read_snapshot,
};
use super::{ParticipantState, auxiliaries_are_owned, classify_directory};

pub(crate) fn verify_all_before(
    transaction_root: &Path,
    manifest: &Manifest,
    roots: &TrustedRoots,
    registry: Option<&dyn LayerRegistry>,
) -> Result<(), MutationError> {
    for participant in &manifest.files {
        let expected = match &participant.before {
            FileBefore::Absent => None,
            FileBefore::Snapshot {
                snapshot_path,
                sha256,
                len,
            } => Some((
                read_snapshot(transaction_root, snapshot_path)?,
                sha256,
                *len,
            )),
        };
        let live = roots.resolve(&participant.live_path)?;
        let actual = file_state(&live).map_err(MutationError::io)?;
        match expected {
            None if actual.is_none() => {}
            None => {
                return Err(MutationError::conflict(format!(
                    "before target became present: {}",
                    live.display()
                )));
            }
            Some((bytes, sha256, len))
                if actual.as_deref() == Some(bytes.as_slice())
                    && bytes.len() as u64 == len
                    && digest_hex(&bytes) == *sha256 => {}
            Some(_) => {
                return Err(MutationError::conflict(format!(
                    "before target drifted: {}",
                    live.display()
                )));
            }
        }
    }
    if !manifest.registry.is_empty() {
        let registry = registry.ok_or_else(|| {
            MutationError::conflict("registry participant requires a registry authority")
        })?;
        for participant in &manifest.registry {
            let manifest_path = roots.resolve(&participant.manifest_path)?;
            let actual = observe_registry(registry, &manifest_path)?;
            if actual != participant.before {
                return Err(MutationError::conflict(format!(
                    "registry participant drifted: {}",
                    manifest_path.display()
                )));
            }
        }
    }
    Ok(())
}

/// Classifies every participant against both exact fence states without
/// writing. Recovery uses this whole-set result to distinguish a safe
/// Before/After mix from an external third state.
pub(crate) fn classify_all(
    transaction_root: &Path,
    manifest: &Manifest,
    roots: &TrustedRoots,
    registry: Option<&dyn LayerRegistry>,
) -> Result<Vec<ParticipantState>, MutationError> {
    let mut states = Vec::with_capacity(manifest.files.len() + manifest.registry.len());
    for participant in &manifest.files {
        let live = roots.resolve(&participant.live_path)?;
        let actual = file_state(&live).map_err(MutationError::io)?;
        let before = matches_file_before(transaction_root, participant, actual.as_deref())?;
        let after = matches_file_after(participant, actual.as_deref());
        states.push(match (before, after) {
            (true, _) => ParticipantState::Before,
            (false, true) => ParticipantState::After,
            (false, false) => ParticipantState::Third,
        });
    }
    if !manifest.registry.is_empty() {
        let registry = registry.ok_or_else(|| {
            MutationError::conflict("registry participant requires a registry authority")
        })?;
        for participant in &manifest.registry {
            let manifest_path = roots.resolve(&participant.manifest_path)?;
            let actual = observe_registry(registry, &manifest_path)?;
            let before = actual == participant.before;
            let after = actual == participant.after;
            states.push(match (before, after) {
                (true, _) => ParticipantState::Before,
                (false, true) => ParticipantState::After,
                (false, false) => ParticipantState::Third,
            });
        }
    }
    for directory in &manifest.directories {
        states.push(classify_directory(directory, roots)?);
    }
    if auxiliaries_are_owned(transaction_root, manifest, roots)? == ParticipantState::Third {
        states.push(ParticipantState::Third);
    }
    Ok(states)
}

pub(crate) fn verify_all_after(
    transaction_root: &Path,
    manifest: &Manifest,
    roots: &TrustedRoots,
    registry: Option<&dyn LayerRegistry>,
) -> Result<(), MutationError> {
    for participant in &manifest.files {
        let live = roots.resolve(&participant.live_path)?;
        let actual = file_state(&live).map_err(MutationError::io)?;
        match (&participant.after, actual) {
            (FileAfter::Absent, None) => {}
            (FileAfter::Absent, Some(_)) => {
                return Err(MutationError::conflict(format!(
                    "after target remains present: {}",
                    live.display()
                )));
            }
            (FileAfter::Present { sha256, len }, Some(bytes))
                if bytes.len() as u64 == *len && digest_hex(&bytes) == *sha256 => {}
            (FileAfter::Present { .. }, _) => {
                return Err(MutationError::conflict(format!(
                    "after target verification failed: {}",
                    live.display()
                )));
            }
        }
    }
    if !manifest.registry.is_empty() {
        let registry = registry.ok_or_else(|| {
            MutationError::conflict("registry participant requires a registry authority")
        })?;
        for participant in &manifest.registry {
            let manifest_path = roots.resolve(&participant.manifest_path)?;
            let actual = observe_registry(registry, &manifest_path)?;
            if actual != participant.after {
                return Err(MutationError::conflict(format!(
                    "registry after verification failed: {}",
                    manifest_path.display()
                )));
            }
        }
    }
    for directory in &manifest.directories {
        if classify_directory(directory, roots)? != ParticipantState::After {
            return Err(MutationError::conflict(
                "directory after verification failed",
            ));
        }
    }
    if auxiliaries_are_owned(transaction_root, manifest, roots)? == ParticipantState::Third {
        return Err(MutationError::conflict(
            "shared Vulkan auxiliary ownership verification failed",
        ));
    }
    Ok(())
}
