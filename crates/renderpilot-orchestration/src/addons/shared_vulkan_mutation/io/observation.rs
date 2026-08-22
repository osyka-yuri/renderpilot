//! Exact observations and fence matching for SVAM participants.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use renderpilot_platform_windows::vulkan_layer::{LayerRegistry, RegistryValueState};
use sha2::{Digest, Sha256};

use super::super::MutationError;
use super::super::manifest::{FileAfter, FileBefore, FileParticipant, RegistryValue};

pub(crate) fn digest_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

pub(crate) fn file_state(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "shared Vulkan target is a symbolic link: {}",
                path.display()
            ),
        ));
    }
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "shared Vulkan target is not a regular file: {}",
                path.display()
            ),
        ));
    }
    fs::read(path).map(Some)
}

pub(crate) fn write_snapshot(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "snapshot has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub(crate) fn read_snapshot(
    transaction_root: &Path,
    relative: &str,
) -> Result<Vec<u8>, MutationError> {
    let path = transaction_root.join(relative);
    let root = transaction_root.canonicalize().map_err(MutationError::io)?;
    let metadata = fs::symlink_metadata(&path).map_err(MutationError::io)?;
    if metadata.file_type().is_symlink() {
        return Err(MutationError::conflict(
            "snapshot path must not be a symbolic link",
        ));
    }
    let canonical_path = path.canonicalize().map_err(MutationError::io)?;
    if !canonical_path.starts_with(&root) {
        return Err(MutationError::conflict(
            "snapshot path escaped transaction root",
        ));
    }
    fs::read(canonical_path).map_err(MutationError::io)
}

pub(crate) fn observe_registry(
    registry: &dyn LayerRegistry,
    path: &Path,
) -> Result<RegistryValue, MutationError> {
    registry
        .observe_canonical_registration(path)
        .map(|state| match state {
            RegistryValueState::Absent => RegistryValue::Absent,
            RegistryValueState::Present {
                value_type,
                raw_bytes,
            } => RegistryValue::Present {
                value_type,
                raw_bytes,
            },
        })
        .map_err(MutationError::io)
}

pub(crate) fn read_verified_snapshot(
    transaction_root: &Path,
    snapshot_path: &str,
    sha256: &str,
    len: u64,
) -> Result<Vec<u8>, MutationError> {
    let bytes = read_snapshot(transaction_root, snapshot_path)?;
    if bytes.len() as u64 != len || digest_hex(&bytes) != sha256 {
        return Err(MutationError::conflict(format!(
            "shared Vulkan snapshot failed its ownership fence: {snapshot_path}"
        )));
    }
    Ok(bytes)
}

pub(crate) fn matches_file_before(
    transaction_root: &Path,
    participant: &FileParticipant,
    actual: Option<&[u8]>,
) -> Result<bool, MutationError> {
    match &participant.before {
        FileBefore::Absent => Ok(actual.is_none()),
        FileBefore::Snapshot {
            snapshot_path,
            sha256,
            len,
        } => match read_verified_snapshot(transaction_root, snapshot_path, sha256, *len) {
            Ok(expected) => Ok(actual == Some(expected.as_slice())),
            Err(MutationError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(actual
                .is_some_and(|bytes| bytes.len() as u64 == *len && digest_hex(bytes) == *sha256)),
            Err(error) => Err(error),
        },
    }
}

pub(crate) fn matches_file_before_strict(
    transaction_root: &Path,
    participant: &FileParticipant,
    actual: Option<&[u8]>,
) -> Result<bool, MutationError> {
    match &participant.before {
        FileBefore::Absent => Ok(actual.is_none()),
        FileBefore::Snapshot {
            snapshot_path,
            sha256,
            len,
        } => {
            let expected = read_verified_snapshot(transaction_root, snapshot_path, sha256, *len)?;
            Ok(actual == Some(expected.as_slice()))
        }
    }
}

pub(crate) fn matches_file_after(participant: &FileParticipant, actual: Option<&[u8]>) -> bool {
    match (&participant.after, actual) {
        (FileAfter::Absent, None) => true,
        (FileAfter::Present { sha256, len }, Some(bytes)) => {
            bytes.len() as u64 == *len && digest_hex(bytes) == *sha256
        }
        _ => false,
    }
}
