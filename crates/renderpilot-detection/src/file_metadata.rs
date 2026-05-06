use std::{
    fmt::Write as _,
    fs::{self, File},
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::AppResult;
use renderpilot_domain::{PathRef, Sha256Hash, Version};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{detection_context_error, detection_error};
use crate::pe_version::read_windows_file_version;

const HASH_BUFFER_SIZE: usize = 64 * 1024;
const UNIX_MILLIS_OVERFLOW_MESSAGE: &str = "modified time is too large to cache";

/// Stable file cache key for deciding whether a detected DLL needs rescanning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileCacheKey {
    path: PathRef,
    size: u64,
    modified_at: u64,
    sha256: Sha256Hash,
}

impl FileCacheKey {
    pub(crate) fn new(path: PathRef, size: u64, modified_at: u64, sha256: Sha256Hash) -> Self {
        Self {
            path,
            size,
            modified_at,
            sha256,
        }
    }

    /// Returns the normalized file path used by this cache key.
    pub fn path(&self) -> &PathRef {
        &self.path
    }

    /// Returns file size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns last modification time as Unix milliseconds.
    pub fn modified_at(&self) -> u64 {
        self.modified_at
    }

    /// Returns the SHA-256 hash used by this cache key.
    pub fn sha256(&self) -> &Sha256Hash {
        &self.sha256
    }
}

/// Status of Windows file-version metadata extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionDetectionStatus {
    /// A parseable FileVersion or ProductVersion was found.
    KnownVersion,
    /// No parseable FileVersion or ProductVersion was found.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DetectedFileMetadata {
    pub(crate) version: Option<Version>,
    pub(crate) status: VersionDetectionStatus,
    pub(crate) sha256: Sha256Hash,
    pub(crate) cache_key: FileCacheKey,
}

impl DetectedFileMetadata {
    fn new(version: Option<Version>, sha256: Sha256Hash, cache_key: FileCacheKey) -> Self {
        let status = VersionDetectionStatus::from_version(version.as_ref());

        Self {
            version,
            status,
            sha256,
            cache_key,
        }
    }
}

pub(crate) fn read_detected_file_metadata(
    path: &Path,
    path_ref: PathRef,
) -> AppResult<DetectedFileMetadata> {
    let file_stat = read_file_stat(path)?;
    let sha256 = sha256_file(path)?;
    let version = read_windows_file_version(path);
    let cache_key = file_stat.into_cache_key(path_ref, sha256.clone());

    Ok(DetectedFileMetadata::new(version, sha256, cache_key))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStat {
    size: u64,
    modified_at: u64,
}

impl FileStat {
    fn into_cache_key(self, path: PathRef, sha256: Sha256Hash) -> FileCacheKey {
        FileCacheKey::new(path, self.size, self.modified_at, sha256)
    }
}

fn read_file_stat(path: &Path) -> AppResult<FileStat> {
    let metadata = fs::metadata(path).map_err(|error| {
        detection_context_error(
            format_args!("could not read metadata for {}", path.display()),
            error,
        )
    })?;
    let modified_at = metadata.modified().map_err(|error| {
        detection_context_error(
            format_args!("could not read modified time for {}", path.display()),
            error,
        )
    })?;

    Ok(FileStat {
        size: metadata.len(),
        modified_at: system_time_to_unix_millis(path, modified_at)?,
    })
}

pub(crate) fn sha256_file(path: &Path) -> AppResult<Sha256Hash> {
    let mut file = File::open(path).map_err(|error| {
        detection_context_error(
            format_args!("could not open {} for hashing", path.display()),
            error,
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer).map_err(|error| {
            detection_context_error(format_args!("could not hash {}", path.display()), error)
        })?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Sha256Hash::new(hex_lower(&hash)).map_err(detection_error)
}

fn system_time_to_unix_millis(path: &Path, modified_at: SystemTime) -> AppResult<u64> {
    let duration = modified_at.duration_since(UNIX_EPOCH).map_err(|error| {
        detection_context_error(
            format_args!("modified time for {} is before Unix epoch", path.display()),
            error,
        )
    })?;

    u64::try_from(duration.as_millis()).map_err(|_| {
        detection_error(format!(
            "{}: {}",
            UNIX_MILLIS_OVERFLOW_MESSAGE,
            path.display()
        ))
    })
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }

    hex
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use renderpilot_domain::Version;

    use super::{sha256_file, VersionDetectionStatus};

    #[test]
    fn sha256_file_hashes_known_content() {
        let path = temp_file_path("sha256-known-content");
        fs::write(&path, b"abc").expect("test file should be written");

        let hash = sha256_file(&path).expect("hashing should succeed");

        assert_eq!(
            hash.as_str(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        fs::remove_file(path).expect("test file should be removed");
    }

    fn temp_file_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();

        std::env::temp_dir().join(format!("renderpilot-{name}-{nanos}.dll"))
    }

    #[test]
    fn version_detection_status_tracks_version_presence() {
        let version = Version::parse("1.2.3").expect("version should parse");

        assert_eq!(
            VersionDetectionStatus::from_version(Some(&version)),
            VersionDetectionStatus::KnownVersion
        );
        assert_eq!(
            VersionDetectionStatus::from_version(None),
            VersionDetectionStatus::UnknownVersion
        );
    }
}
