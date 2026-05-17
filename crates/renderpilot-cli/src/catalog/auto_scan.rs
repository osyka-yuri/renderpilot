use renderpilot_application::AppError;
use renderpilot_detection::{FileHashCache, LibraryPatternComponentDetector};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::catalog::scan::scan_auto_in_shared_batch;
use crate::catalog::{open_catalog_storage, ScanFolderCatalogResult};
use crate::error::CliError;

/// Reusable per-batch state produced by [`open_auto_scan_batch`] and consumed
/// by every call to [`scan_auto_in_batch`] in an auto-scan loop.
pub(crate) struct AutoScanBatch {
    pub(crate) storage: SqliteStorage,
    pub(crate) detector: LibraryPatternComponentDetector,
    pub(crate) prefetched_cache: FileHashCache,
}

/// Opens shared resources for a batched auto-scan.
///
/// Caller must drive the loop with [`scan_auto_in_batch`] for each install
/// directory. Built once, the batch:
///
/// - holds a single open SQLite connection (one set of `PRAGMA`s + migrations),
/// - reuses the compiled library-pattern detector,
/// - prefetches the entire `file_hash_cache` table into memory in one query.
pub(crate) fn open_auto_scan_batch() -> Result<AutoScanBatch, CliError> {
    let storage = open_catalog_storage()?;
    let detector = LibraryPatternComponentDetector::windows_default()
        .map_err(|error| AppError::detection_failed(error.to_string()))?;
    let prefetched_cache = load_full_hash_cache(&storage)?;

    Ok(AutoScanBatch {
        storage,
        detector,
        prefetched_cache,
    })
}

/// Per-install entry point used inside an [`AutoScanBatch`] loop.
pub(crate) fn scan_auto_in_batch(
    batch: &AutoScanBatch,
    path: &std::path::Path,
) -> Result<Vec<ScanFolderCatalogResult>, CliError> {
    scan_auto_in_shared_batch(
        &batch.storage,
        &batch.detector,
        &batch.prefetched_cache,
        path,
    )
}

fn load_full_hash_cache(storage: &SqliteStorage) -> Result<FileHashCache, CliError> {
    let rows = storage.load_all_file_hash_cache()?;
    Ok(crate::catalog::scan::populate_file_hash_cache(rows))
}
