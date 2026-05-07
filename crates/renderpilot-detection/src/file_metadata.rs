use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::AppResult;
use renderpilot_domain::{PathRef, Sha256Hash, Version};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    error::{detection_context_error, detection_error},
    pe_version::read_windows_file_version,
};

const HASH_BUFFER_SIZE: usize = 64 * 1024;
const UNIX_MILLIS_OVERFLOW_MESSAGE: &str = "modified time is too large to cache";

/// Cached metadata that is expensive to recompute.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedFileData {
    sha256: Sha256Hash,
    version: Option<Version>,
}

impl CachedFileData {
    fn new(sha256: Sha256Hash, version: Option<Version>) -> Self {
        Self { sha256, version }
    }
}

/// One cache entry for a previously scanned file.
///
/// `size` and `modified_at` are the freshness guards.
/// `data` is reused only when both guards still match the current file stat.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedFileEntry {
    size: u64,
    modified_at: u64,
    data: CachedFileData,
}

impl CachedFileEntry {
    fn new(size: u64, modified_at: u64, sha256: Sha256Hash, version: Option<Version>) -> Self {
        Self {
            size,
            modified_at,
            data: CachedFileData::new(sha256, version),
        }
    }

    fn is_fresh_for(&self, stat: FileStat) -> bool {
        self.size == stat.size && self.modified_at == stat.modified_at
    }
}

/// In-memory map of path → hash/version from prior scans.
///
/// On re-scan, unchanged size and mtime skip SHA-256 and PE version work.
#[derive(Debug, Clone, Default)]
pub struct FileHashCache {
    /// Normalized path → cached stat/data.
    entries: HashMap<String, CachedFileEntry>,
}

impl FileHashCache {
    /// Returns an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns an empty cache with `entries` pre-allocated to `capacity`.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
        }
    }

    /// Inserts or replaces an entry.
    pub fn insert(
        &mut self,
        path: String,
        size: u64,
        modified_at: u64,
        sha256: Sha256Hash,
        version: Option<Version>,
    ) {
        self.entries.insert(
            path,
            CachedFileEntry::new(size, modified_at, sha256, version),
        );
    }

    /// Returns cached hash/version only when the stored size and mtime still match.
    fn fresh_data_for(&self, path: &str, stat: FileStat) -> Option<&CachedFileData> {
        let entry = self.entries.get(path)?;

        if entry.is_fresh_for(stat) {
            Some(&entry.data)
        } else {
            None
        }
    }

    /// Returns the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Stable file cache key for deciding whether a detected DLL needs rescanning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileCacheKey {
    /// Normalized absolute path of the file.
    path: PathRef,
    /// File size in bytes.
    size: u64,
    /// Last modification time as Unix milliseconds.
    modified_at: u64,
    /// SHA-256 of file contents.
    sha256: Sha256Hash,
}

impl FileCacheKey {
    /// Constructs a key from path, size, mtime, and content hash.
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
        Self {
            status: VersionDetectionStatus::from_version(version.as_ref()),
            version,
            sha256,
            cache_key,
        }
    }

    fn from_cache(path_ref: PathRef, stat: FileStat, cached: &CachedFileData) -> Self {
        let cache_key = stat.into_cache_key(path_ref, cached.sha256.clone());

        Self::new(cached.version.clone(), cached.sha256.clone(), cache_key)
    }

    fn from_fresh_read(
        path_ref: PathRef,
        stat: FileStat,
        sha256: Sha256Hash,
        version: Option<Version>,
    ) -> Self {
        let cache_key = stat.into_cache_key(path_ref, sha256.clone());

        Self::new(version, sha256, cache_key)
    }
}

pub(crate) fn read_detected_file_metadata(
    path: &Path,
    path_ref: PathRef,
    hash_cache: Option<&FileHashCache>,
) -> AppResult<DetectedFileMetadata> {
    let stat = read_file_stat(path)?;

    if let Some(metadata) = metadata_from_cache(path_ref.clone(), stat, hash_cache) {
        return Ok(metadata);
    }

    read_metadata_from_file(path, path_ref, stat)
}

fn metadata_from_cache(
    path_ref: PathRef,
    stat: FileStat,
    hash_cache: Option<&FileHashCache>,
) -> Option<DetectedFileMetadata> {
    let cached = hash_cache?.fresh_data_for(path_ref.as_str(), stat)?;

    Some(DetectedFileMetadata::from_cache(path_ref, stat, cached))
}

fn read_metadata_from_file(
    path: &Path,
    path_ref: PathRef,
    stat: FileStat,
) -> AppResult<DetectedFileMetadata> {
    let sha256 = sha256_file(path)?;
    let version = read_windows_file_version(path);

    Ok(DetectedFileMetadata::from_fresh_read(
        path_ref, stat, sha256, version,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStat {
    size: u64,
    modified_at: u64,
}

impl FileStat {
    fn new(size: u64, modified_at: u64) -> Self {
        Self { size, modified_at }
    }

    fn into_cache_key(self, path: PathRef, sha256: Sha256Hash) -> FileCacheKey {
        FileCacheKey::new(path, self.size, self.modified_at, sha256)
    }
}

fn read_file_stat(path: &Path) -> AppResult<FileStat> {
    let metadata = read_fs_metadata(path)?;
    let modified_at = read_modified_time(path, &metadata)?;

    Ok(FileStat::new(
        metadata.len(),
        system_time_to_unix_millis(path, modified_at)?,
    ))
}

fn read_fs_metadata(path: &Path) -> AppResult<fs::Metadata> {
    fs::metadata(path).map_err(|error| {
        detection_context_error(
            format_args!("could not read metadata for {}", path.display()),
            error,
        )
    })
}

fn read_modified_time(path: &Path, metadata: &fs::Metadata) -> AppResult<SystemTime> {
    metadata.modified().map_err(|error| {
        detection_context_error(
            format_args!("could not read modified time for {}", path.display()),
            error,
        )
    })
}

pub(crate) fn sha256_file(path: &Path) -> AppResult<Sha256Hash> {
    let file = open_file_for_hashing(path)?;
    let hash = sha256_reader_hex(file).map_err(|error| {
        detection_context_error(format_args!("could not hash {}", path.display()), error)
    })?;

    Sha256Hash::new(hash).map_err(detection_error)
}

fn open_file_for_hashing(path: &Path) -> AppResult<File> {
    File::open(path).map_err(|error| {
        detection_context_error(
            format_args!("could not open {} for hashing", path.display()),
            error,
        )
    })
}

fn sha256_reader_hex(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        let bytes_read = reader.read(&mut buffer)?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

fn system_time_to_unix_millis(path: &Path, time: SystemTime) -> AppResult<u64> {
    let duration = time.duration_since(UNIX_EPOCH).map_err(|error| {
        detection_context_error(
            format_args!("modified time for {} is before Unix epoch", path.display()),
            error,
        )
    })?;

    u64::try_from(duration.as_millis()).map_err(|_| unix_millis_overflow_error(path))
}

fn unix_millis_overflow_error(path: &Path) -> renderpilot_application::AppError {
    detection_error(format!(
        "{}: {}",
        UNIX_MILLIS_OVERFLOW_MESSAGE,
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use renderpilot_domain::Version;

    use super::{sha256_file, VersionDetectionStatus};

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn sha256_file_hashes_known_content() {
        let file = TempFile::new("sha256-known-content");

        fs::write(file.path(), b"abc").expect("test file should be written");

        let hash = sha256_file(file.path()).expect("hashing should succeed");

        assert_eq!(hash.as_str(), ABC_SHA256);
    }

    #[test]
    fn version_detection_status_tracks_version_presence() {
        let version = Version::parse("1.2.3").expect("version should parse");

        assert_eq!(
            VersionDetectionStatus::from_version(Some(&version)),
            VersionDetectionStatus::KnownVersion,
        );

        assert_eq!(
            VersionDetectionStatus::from_version(None),
            VersionDetectionStatus::UnknownVersion,
        );
    }

    struct TempFile {
        path: PathBuf,
    }

    impl TempFile {
        fn new(name: &str) -> Self {
            Self {
                path: temp_file_path(name),
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn temp_file_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();

        std::env::temp_dir().join(format!("renderpilot-{name}-{nanos}.dll"))
    }
}
