//! Batched auto-scan: shared open storage, detector, and prefetched hash-cache.

use renderpilot_application::AppError;
use renderpilot_detection::{FileHashCache, LibraryPatternComponentDetector};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

use super::scan::scan_auto_in_shared_batch;
use super::ScanFolderCatalogResult;

/// Reusable per-batch state produced by [`open_auto_scan_batch`] and consumed
/// by every call to [`scan_auto_in_batch`] in an auto-scan loop.
pub struct AutoScanBatch {
    context: crate::Context,
    detector: LibraryPatternComponentDetector,
    prefetched_cache: FileHashCache,
}

impl AutoScanBatch {
    /// Returns a reference to the batch's shared context.
    pub fn context(&self) -> &crate::Context {
        &self.context
    }
}

/// Opens shared resources for a batched auto-scan.
///
/// Caller must drive the loop with [`scan_auto_in_batch`] for each install
/// directory. Built once, the batch:
///
/// - holds a single open SQLite connection (one set of `PRAGMA`s + migrations),
/// - reuses the compiled library-pattern detector,
/// - prefetches the entire `file_hash_cache` table into memory in one query.
pub fn open_auto_scan_batch() -> Result<AutoScanBatch, ServiceError> {
    let context = crate::Context::open()?;
    let detector = LibraryPatternComponentDetector::windows_default()
        .map_err(|error| AppError::detection_failed(error.to_string()))?;
    let prefetched_cache = load_full_hash_cache(context.storage())?;

    Ok(AutoScanBatch {
        context,
        detector,
        prefetched_cache,
    })
}

/// Per-install entry point used inside an [`AutoScanBatch`] loop.
pub fn scan_auto_in_batch(
    batch: &AutoScanBatch,
    path: &std::path::Path,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    scan_auto_in_shared_batch(
        batch.context().storage(),
        &batch.detector,
        &batch.prefetched_cache,
        path,
    )
}

fn load_full_hash_cache(storage: &SqliteStorage) -> Result<FileHashCache, ServiceError> {
    let rows = storage.load_all_file_hash_cache()?;
    Ok(crate::catalog::scan::populate_file_hash_cache(rows))
}
