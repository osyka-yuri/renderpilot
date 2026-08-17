//! Metadata derived from one stable file observation.

use std::{
    fs::File,
    io::{self, ErrorKind, Read},
    path::Path,
};

use renderpilot_application::AppResult;
use renderpilot_domain::{PathRef, Sha256Hash, Version};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    error::{detection_context_error, detection_error},
    file_observation::{FileObservationResult, FileObservationSource, StrongFileCacheKey},
    pe::inspect_pe_bytes,
};

const HASH_BUFFER_SIZE: usize = 256 * 1024;

/// Status of file-version metadata extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionDetectionStatus {
    /// A parseable FileVersion or ProductVersion was found.
    KnownVersion,
    /// Version was read from the same object and no version resource existed.
    UnknownVersion,
}

impl VersionDetectionStatus {
    fn from_version(version: Option<&Version>) -> Self {
        match version {
            Some(_) => Self::KnownVersion,
            None => Self::UnknownVersion,
        }
    }
}

/// Detection facts from one stable source lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DetectedFileMetadata {
    pub(crate) status: VersionDetectionStatus,
    pub(crate) sha256: Sha256Hash,
    pub(crate) cache_key: Option<StrongFileCacheKey>,
    pub(crate) pe: crate::PeInspection,
}

/// Reads SHA-256, PE and version facts from one stable object. A missing file
/// returns None. Identity instability is an error, so a caller cannot publish a
/// partial scan as complete.
pub(crate) fn try_read_detected_file_metadata(
    path: &Path,
    source: &dyn FileObservationSource,
) -> AppResult<Option<DetectedFileMetadata>> {
    match source.observe(path)? {
        FileObservationResult::Missing => Ok(None),
        FileObservationResult::Unavailable => Err(detection_error(format!(
            "file observation was unavailable or unstable for {}",
            path.display()
        ))),
        FileObservationResult::Available(snapshot) => {
            let pe = inspect_pe_bytes(&snapshot.bytes);
            Ok(Some(DetectedFileMetadata {
                status: VersionDetectionStatus::from_version(pe.version.as_ref()),
                sha256: snapshot.sha256,
                cache_key: snapshot.cache_key,
                pe,
            }))
        }
    }
}

/// Computes SHA-256 of a file. This utility is intentionally independent of
/// scan reuse; callers requiring persisted facts use a strong observation.
pub fn sha256_file(path: &Path) -> AppResult<Sha256Hash> {
    let file = File::open(path).map_err(|error| {
        detection_context_error(
            format_args!("could not open {} for hashing", path.display()),
            error,
        )
    })?;

    let hash = sha256_reader_hex(file).map_err(|error| {
        detection_context_error(format_args!("could not hash {}", path.display()), error)
    })?;
    Sha256Hash::new(hash).map_err(detection_error)
}

/// Computes SHA-256 of in-memory bytes.
pub fn sha256_bytes(bytes: &[u8]) -> AppResult<Sha256Hash> {
    Sha256Hash::new(hex::encode(Sha256::digest(bytes))).map_err(detection_error)
}

fn sha256_reader_hex(mut reader: impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; HASH_BUFFER_SIZE];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Strong observation facts exposed with every detected library file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileObservation {
    /// Normalized path that was reopened after the lease read.
    pub path: PathRef,
    /// Stable object identity kind.
    pub identity_kind: String,
    /// Stable object identifier.
    pub object_identity: String,
    /// Change token for the observed object.
    pub change_token: String,
    /// Object size from the identity read.
    pub size: u64,
    /// SHA-256 read from that same object.
    pub sha256: Sha256Hash,
}

impl FileObservation {
    pub(crate) fn from_metadata(
        path: PathRef,
        metadata: &mut DetectedFileMetadata,
    ) -> Option<Self> {
        let cache_key = metadata.cache_key.take()?;
        Some(Self {
            path,
            identity_kind: cache_key.kind,
            object_identity: cache_key.object_identity,
            change_token: cache_key.change_token,
            size: cache_key.size,
            sha256: metadata.sha256.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{VersionDetectionStatus, sha256_bytes};

    #[test]
    fn hashes_known_bytes() {
        assert_eq!(
            sha256_bytes(b"abc").expect("hash").as_str(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn unknown_version_is_an_observed_fact() {
        assert_eq!(
            VersionDetectionStatus::from_version(None),
            VersionDetectionStatus::UnknownVersion
        );
    }
}
