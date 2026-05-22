use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, ErrorKind, Read},
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

/// Metadata that is relatively expensive to recompute for a file.
///
/// This is safe to reuse only when the cheap freshness guards still match:
/// file size and modification time.
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

/// Cheap file identity used to decide whether cached expensive metadata
/// can be reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStat {
    size: u64,
    modified_at: u64,
}

impl FileStat {
    fn new(size: u64, modified_at: u64) -> Self {
        Self { size, modified_at }
    }

    /// Returns `Ok(None)` when the file no longer exists.
    ///
    /// Used for detection paths that skip an explicit `path.is_file()` check.
    /// Stale hash-cache entries surface as `NotFound` and are skipped.
    fn try_read(path: &Path) -> AppResult<Option<Self>> {
        match fs::metadata(path) {
            Ok(metadata) => {
                let modified_at = read_modified_time(path, &metadata)?;
                Ok(Some(Self::new(
                    metadata.len(),
                    system_time_to_unix_millis(path, modified_at)?,
                )))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(detection_context_error(
                format_args!("could not read metadata for {}", path.display()),
                error,
            )),
        }
    }

    fn into_cache_key(self, path: PathRef, sha256: Sha256Hash) -> FileCacheKey {
        FileCacheKey::new(path, self.size, self.modified_at, sha256)
    }
}

/// One cache entry for a previously scanned file.
///
/// `stat` is the freshness guard.
/// `data` is reused only when `stat` still matches the current file stat.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedFileEntry {
    stat: FileStat,
    data: CachedFileData,
}

impl CachedFileEntry {
    fn new(stat: FileStat, sha256: Sha256Hash, version: Option<Version>) -> Self {
        Self {
            stat,
            data: CachedFileData::new(sha256, version),
        }
    }

    fn is_fresh_for(&self, current: FileStat) -> bool {
        self.stat == current
    }
}

/// In-memory map of normalized path → hash/version from prior scans.
///
/// On re-scan, unchanged size and mtime skip SHA-256 and PE version work.
#[derive(Debug, Clone, Default)]
pub struct FileHashCache {
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
        let stat = FileStat::new(size, modified_at);
        let entry = CachedFileEntry::new(stat, sha256, version);

        self.entries.insert(path, entry);
    }

    /// Returns cached hash/version only when the stored size and mtime still match.
    fn fresh_data_for(&self, path: &str, stat: FileStat) -> Option<&CachedFileData> {
        let entry = self.entries.get(path)?;

        entry.is_fresh_for(stat).then_some(&entry.data)
    }

    /// Returns the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns an iterator over the normalized paths in the cache.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
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
    fn from_parts(
        path_ref: PathRef,
        stat: FileStat,
        sha256: Sha256Hash,
        version: Option<Version>,
    ) -> Self {
        let status = VersionDetectionStatus::from_version(version.as_ref());
        let cache_key = stat.into_cache_key(path_ref, sha256.clone());

        Self {
            version,
            status,
            sha256,
            cache_key,
        }
    }

    fn from_cached(path_ref: PathRef, stat: FileStat, cached: &CachedFileData) -> Self {
        Self::from_parts(
            path_ref,
            stat,
            cached.sha256.clone(),
            cached.version.clone(),
        )
    }
}

/// Reads size/mtime, reuses cached hash/version when fresh, otherwise hashes the file.
///
/// Returns `Ok(None)` when the path no longer exists (e.g. race between directory listing
/// and this read, or a stale hash-cache entry). Other I/O errors propagate.
pub(crate) fn try_read_detected_file_metadata(
    path: &Path,
    path_ref: PathRef,
    hash_cache: Option<&FileHashCache>,
) -> AppResult<Option<DetectedFileMetadata>> {
    let Some(stat) = FileStat::try_read(path)? else {
        return Ok(None);
    };

    if let Some(metadata) = try_read_cached_metadata(path_ref.clone(), stat, hash_cache) {
        return Ok(Some(metadata));
    }

    read_uncached_metadata(path, path_ref, stat).map(Some)
}

fn try_read_cached_metadata(
    path_ref: PathRef,
    stat: FileStat,
    hash_cache: Option<&FileHashCache>,
) -> Option<DetectedFileMetadata> {
    let cached = hash_cache?.fresh_data_for(path_ref.as_str(), stat)?;

    Some(DetectedFileMetadata::from_cached(path_ref, stat, cached))
}

fn read_uncached_metadata(
    path: &Path,
    path_ref: PathRef,
    stat: FileStat,
) -> AppResult<DetectedFileMetadata> {
    let sha256 = sha256_file(path)?;
    let version = read_windows_file_version(path);

    Ok(DetectedFileMetadata::from_parts(
        path_ref, stat, sha256, version,
    ))
}

fn read_modified_time(path: &Path, metadata: &fs::Metadata) -> AppResult<SystemTime> {
    metadata.modified().map_err(|error| {
        detection_context_error(
            format_args!("could not read modified time for {}", path.display()),
            error,
        )
    })
}

/// Computes the SHA-256 hash of the file at the given path.
pub fn sha256_file(path: &Path) -> AppResult<Sha256Hash> {
    let file = open_file_for_hashing(path)?;

    #[cfg(test)]
    sha256_test_hooks::record_sha256_file_call();

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

fn sha256_reader_hex(mut reader: impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => hasher.update(&buffer[..bytes_read]),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
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

/// Test-only instrumentation for [`sha256_file`] (parallel-safe per OS thread).
#[cfg(test)]
#[allow(clippy::missing_const_for_thread_local)]
mod sha256_test_hooks {
    use std::cell::Cell;

    thread_local! {
        static SHA256_FILE_CALL_COUNT: Cell<usize> = Cell::new(0);
    }

    pub(super) fn record_sha256_file_call() {
        SHA256_FILE_CALL_COUNT.with(|count| count.set(count.get() + 1));
    }

    pub(super) fn reset_sha256_file_call_count() {
        SHA256_FILE_CALL_COUNT.with(|count| count.set(0));
    }

    pub(super) fn sha256_file_call_count() -> usize {
        SHA256_FILE_CALL_COUNT.with(|count| count.get())
    }
}

/// Resets the test-only counter used by [`sha256_file_call_count_for_tests`].
#[cfg(test)]
pub(crate) fn reset_sha256_file_call_count_for_tests() {
    sha256_test_hooks::reset_sha256_file_call_count();
}

/// Returns how often [`sha256_file`] ran a SHA-256 pass (test builds only).
#[cfg(test)]
pub(crate) fn sha256_file_call_count_for_tests() -> usize {
    sha256_test_hooks::sha256_file_call_count()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{self, ErrorKind, Read},
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use renderpilot_domain::{Sha256Hash, Version};

    use super::{sha256_file, sha256_reader_hex, FileHashCache, FileStat, VersionDetectionStatus};

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn sha256_file_hashes_known_content() {
        let file = TempFile::new("sha256-known-content");

        fs::write(file.path(), b"abc").expect("test file should be written");

        let hash = sha256_file(file.path()).expect("hashing should succeed");

        assert_eq!(hash.as_str(), ABC_SHA256);
    }

    #[test]
    fn sha256_reader_retries_interrupted_reads() {
        let reader = InterruptedOnceReader::new(b"abc");

        let hash = sha256_reader_hex(reader).expect("hashing should succeed after interruption");

        assert_eq!(hash, ABC_SHA256);
    }

    #[test]
    fn cache_returns_fresh_data_when_path_size_and_mtime_match() {
        let mut cache = FileHashCache::new();
        let hash = test_hash();

        cache.insert("C:/test.dll".to_owned(), 123, 456, hash.clone(), None);

        let cached = cache
            .fresh_data_for("C:/test.dll", FileStat::new(123, 456))
            .expect("cache entry should be fresh");

        assert_eq!(cached.sha256, hash);
        assert_eq!(cached.version, None);
    }

    #[test]
    fn cache_misses_when_size_differs() {
        let mut cache = FileHashCache::new();

        cache.insert("C:/test.dll".to_owned(), 123, 456, test_hash(), None);

        assert!(
            cache
                .fresh_data_for("C:/test.dll", FileStat::new(124, 456))
                .is_none(),
            "cache should miss when size changed"
        );
    }

    #[test]
    fn cache_misses_when_modified_time_differs() {
        let mut cache = FileHashCache::new();

        cache.insert("C:/test.dll".to_owned(), 123, 456, test_hash(), None);

        assert!(
            cache
                .fresh_data_for("C:/test.dll", FileStat::new(123, 457))
                .is_none(),
            "cache should miss when modified time changed"
        );
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

    fn test_hash() -> Sha256Hash {
        Sha256Hash::new(ABC_SHA256.to_owned()).expect("test hash should be valid")
    }

    struct InterruptedOnceReader {
        bytes: &'static [u8],
        interrupted: bool,
    }

    impl InterruptedOnceReader {
        fn new(bytes: &'static [u8]) -> Self {
            Self {
                bytes,
                interrupted: false,
            }
        }
    }

    impl Read for InterruptedOnceReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::Error::new(ErrorKind::Interrupted, "interrupted"));
            }

            if self.bytes.is_empty() {
                return Ok(0);
            }

            let bytes_to_copy = self.bytes.len().min(buffer.len());
            buffer[..bytes_to_copy].copy_from_slice(&self.bytes[..bytes_to_copy]);
            self.bytes = &self.bytes[bytes_to_copy..];

            Ok(bytes_to_copy)
        }
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
