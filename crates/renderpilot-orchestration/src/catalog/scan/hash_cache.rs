//! SQLite-backed per-file hash cache load/save for scan detection.

use renderpilot_detection::{DetectedLibraryFile, FileHashCache};
use renderpilot_storage_sqlite::{FileHashCacheRow, SqliteStorage};

use crate::ServiceError;

/// Builds an in-memory [`FileHashCache`] from persisted rows.
pub(crate) fn populate_file_hash_cache(rows: Vec<FileHashCacheRow>) -> FileHashCache {
    let mut cache = FileHashCache::with_capacity(rows.len());

    for row in rows {
        cache.insert(row.path, row.size, row.modified_at, row.sha256, row.version);
    }

    cache
}

pub(super) fn load_hash_cache(
    storage: &SqliteStorage,
    prefix: &str,
) -> Result<FileHashCache, ServiceError> {
    let rows = storage.load_file_hash_cache(prefix)?;
    Ok(populate_file_hash_cache(rows))
}

/// Persists per-file metadata for detected libraries into SQLite (`file_hash_cache`).
///
/// Invoked only after successful detection. Each row matches [`DetectedLibraryFile`]:
/// cache hits reuse stored SHA-256 when size and `modified_at` still match the file;
/// cache misses and stale entries persist the newly computed hash and PE version.
/// If detection fails, this function is not called, so the table is not overwritten
/// with partial or garbage data from an aborted scan.
pub(super) fn save_hash_cache(
    storage: &SqliteStorage,
    libraries: &[DetectedLibraryFile],
) -> Result<(), ServiceError> {
    if libraries.is_empty() {
        return Ok(());
    }

    let entries = libraries
        .iter()
        .map(|library| FileHashCacheRow {
            path: library.file_path().as_str().to_owned(),
            size: library.cache_key().size(),
            modified_at: library.cache_key().modified_at(),
            sha256: library.sha256().clone(),
            version: library.version().cloned(),
        })
        .collect::<Vec<_>>();

    storage.save_file_hash_cache(&entries).map_err(Into::into)
}
