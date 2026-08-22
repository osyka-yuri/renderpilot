//! Exact participant observation, staging, publication, and verification.

use std::fs;
use std::io;
use std::path::Path;

use super::manifest::{DirectoryParticipant, FileAfter, FileBefore, FileParticipant, Manifest};
use super::{MutationError, TrustedRoots};

mod observation;
mod publication;
mod staging;
mod verification;

pub(crate) use observation::{
    digest_hex, file_state, observe_registry, read_verified_snapshot, write_snapshot,
};
pub(crate) use publication::{apply_files, deactivates_registry, restore_files, restore_registry};
pub(crate) use staging::{materialize_stages, sync_prepared_artifacts};
pub(crate) use verification::{classify_all, verify_all_after, verify_all_before};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParticipantState {
    Before,
    After,
    Third,
}

pub(crate) fn sync_published_directories(
    manifest: &Manifest,
    roots: &TrustedRoots,
) -> Result<(), MutationError> {
    let mut directories = std::collections::BTreeSet::new();
    for participant in &manifest.files {
        let live = roots.resolve(&participant.live_path)?;
        if let Some(parent) = live.parent() {
            directories.insert(parent.to_path_buf());
        }
    }
    for participant in &manifest.directories {
        let directory = roots.resolve(&participant.path)?;
        directories.insert(directory.clone());
        if let Some(parent) = directory.parent() {
            directories.insert(parent.to_path_buf());
        }
    }
    for directory in directories {
        crate::fs::sync_directory_best_effort(&directory);
    }
    Ok(())
}

pub(crate) fn cleanup_artifacts(
    transaction_root: &Path,
    manifest: &Manifest,
    roots: &TrustedRoots,
    final_state: ParticipantState,
) -> Result<(), MutationError> {
    if final_state == ParticipantState::Third {
        return Err(MutationError::conflict(
            "cannot clean transaction artifacts in a third state",
        ));
    }
    verify_cleanup_ownership(transaction_root, manifest, roots)?;
    for participant in &manifest.files {
        for path in [
            participant.stage_path.as_ref(),
            participant.tomb_path.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            let path = roots.resolve(path)?;
            remove_owned_auxiliary(&path, participant, transaction_root, roots)?;
        }
    }
    for participant in &manifest.files {
        if let FileBefore::Snapshot {
            snapshot_path,
            sha256,
            len,
        } = &participant.before
        {
            let snapshot = transaction_root.join(snapshot_path);
            remove_owned_snapshot(transaction_root, &snapshot, snapshot_path, sha256, *len)?;
        }
    }
    remove_if_dir(&transaction_root.join("snapshots"))?;
    if final_state == ParticipantState::Before {
        for directory in manifest.directories.iter().rev() {
            let path = roots.resolve(&directory.path)?;
            remove_created_dir(&path)?;
        }
    }
    sync_published_directories(manifest, roots)?;
    remove_if_dir(transaction_root)?;
    if let Some(parent) = transaction_root.parent() {
        crate::fs::sync_directory_best_effort(parent);
    }
    Ok(())
}

fn verify_cleanup_ownership(
    transaction_root: &Path,
    manifest: &Manifest,
    roots: &TrustedRoots,
) -> Result<(), MutationError> {
    if auxiliaries_are_owned(transaction_root, manifest, roots)? == ParticipantState::Third {
        return Err(MutationError::conflict(
            "shared Vulkan auxiliary ownership verification failed before cleanup",
        ));
    }
    let root_metadata = match fs::symlink_metadata(transaction_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(MutationError::io(error)),
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(MutationError::conflict(
            "shared Vulkan transaction root changed type before cleanup",
        ));
    }
    let entries = fs::read_dir(transaction_root)
        .map_err(MutationError::io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(MutationError::io)?;
    for entry in &entries {
        if entry.file_name() != "snapshots" {
            return Err(MutationError::conflict(
                "shared Vulkan transaction root contains an unowned entry",
            ));
        }
    }
    let snapshots = transaction_root.join("snapshots");
    let snapshot_metadata = match fs::symlink_metadata(&snapshots) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(MutationError::io(error)),
    };
    if snapshot_metadata.file_type().is_symlink() || !snapshot_metadata.is_dir() {
        return Err(MutationError::conflict(
            "shared Vulkan snapshot container changed type before cleanup",
        ));
    }
    let expected = manifest
        .files
        .iter()
        .filter_map(|participant| match &participant.before {
            FileBefore::Absent => None,
            FileBefore::Snapshot {
                snapshot_path,
                sha256,
                len,
            } => Some((snapshot_path.as_str(), (sha256.as_str(), *len))),
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    for entry in fs::read_dir(&snapshots).map_err(MutationError::io)? {
        let entry = entry.map_err(MutationError::io)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(MutationError::io)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(MutationError::conflict(
                "shared Vulkan snapshot container contains an unowned entry",
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(transaction_root)
            .map_err(|_| MutationError::conflict("snapshot escaped its transaction root"))?
            .to_str()
            .ok_or_else(|| MutationError::conflict("snapshot path is not valid Unicode"))?
            .replace('\\', "/");
        let Some((sha256, len)) = expected.get(relative.as_str()) else {
            return Err(MutationError::conflict(
                "shared Vulkan snapshot container contains an unowned file",
            ));
        };
        read_verified_snapshot(transaction_root, &relative, sha256, *len)?;
    }
    Ok(())
}

fn remove_owned_snapshot(
    transaction_root: &Path,
    absolute: &Path,
    relative: &str,
    sha256: &str,
    len: u64,
) -> Result<(), MutationError> {
    match file_state(absolute).map_err(MutationError::io)? {
        None => Ok(()),
        Some(_) => {
            read_verified_snapshot(transaction_root, relative, sha256, len)?;
            remove_if_file(absolute)
        }
    }
}

fn remove_if_file(path: &Path) -> Result<(), MutationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(MutationError::io(error)),
    }
}

fn remove_if_dir(path: &Path) -> Result<(), MutationError> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.kind() == io::ErrorKind::NotADirectory =>
        {
            Ok(())
        }
        Err(error) => Err(MutationError::io(error)),
    }
}

fn remove_created_dir(path: &Path) -> Result<(), MutationError> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.kind() == io::ErrorKind::NotADirectory =>
        {
            Ok(())
        }
        Err(error) => Err(MutationError::io(error)),
    }
}

fn classify_directory(
    participant: &DirectoryParticipant,
    roots: &TrustedRoots,
) -> Result<ParticipantState, MutationError> {
    let path = roots.resolve(&participant.path)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ParticipantState::Before);
        }
        Err(error) => return Err(MutationError::io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(ParticipantState::Third);
    }
    let allowed = participant
        .allowed_direct_children
        .iter()
        .map(super::CapabilityPath::normalized_key)
        .collect::<std::collections::BTreeSet<_>>();
    for entry in fs::read_dir(&path).map_err(MutationError::io)? {
        let entry = entry.map_err(MutationError::io)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(MutationError::io)?;
        if metadata.file_type().is_symlink() {
            return Ok(ParticipantState::Third);
        }
        let child = match roots.authorize(&entry.path()) {
            Ok(child) => child,
            Err(_) => return Ok(ParticipantState::Third),
        };
        if !allowed.contains(&child.normalized_key()) {
            return Ok(ParticipantState::Third);
        }
    }
    Ok(ParticipantState::After)
}

fn auxiliaries_are_owned(
    transaction_root: &Path,
    manifest: &Manifest,
    roots: &TrustedRoots,
) -> Result<ParticipantState, MutationError> {
    for participant in &manifest.files {
        for path in [
            participant.stage_path.as_ref(),
            participant.tomb_path.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            let absolute = roots.resolve(path)?;
            let Some(bytes) = file_state(&absolute).map_err(MutationError::io)? else {
                continue;
            };
            if !auxiliary_matches(&absolute, &bytes, participant, transaction_root, roots)? {
                return Ok(ParticipantState::Third);
            }
        }
    }
    Ok(ParticipantState::After)
}

fn auxiliary_matches(
    path: &Path,
    bytes: &[u8],
    participant: &FileParticipant,
    transaction_root: &Path,
    roots: &TrustedRoots,
) -> Result<bool, MutationError> {
    if let Some(stage) = &participant.stage_path
        && roots.resolve(stage)? == path
    {
        return Ok(matches!(
            &participant.after,
            FileAfter::Present { sha256, len }
                if bytes.len() as u64 == *len && digest_hex(bytes) == *sha256
        ));
    }
    if let Some(tomb) = &participant.tomb_path
        && roots.resolve(tomb)? == path
    {
        return match &participant.before {
            FileBefore::Absent => Ok(false),
            FileBefore::Snapshot {
                snapshot_path,
                sha256,
                len,
            } => {
                let before = read_verified_snapshot(transaction_root, snapshot_path, sha256, *len)?;
                Ok(bytes == before.as_slice())
            }
        };
    }
    Ok(false)
}

fn remove_owned_auxiliary(
    path: &Path,
    participant: &FileParticipant,
    transaction_root: &Path,
    roots: &TrustedRoots,
) -> Result<(), MutationError> {
    let Some(bytes) = file_state(path).map_err(MutationError::io)? else {
        return Ok(());
    };
    if !auxiliary_matches(path, &bytes, participant, transaction_root, roots)? {
        return Err(MutationError::conflict(format!(
            "refusing to remove an unowned shared Vulkan auxiliary: {}",
            path.display()
        )));
    }
    remove_if_file(path)
}
