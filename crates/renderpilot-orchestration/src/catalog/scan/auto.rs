use renderpilot_detection::{FileHashCache, LibraryPatternComponentDetector};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::catalog::ScanFolderCatalogResult;
use crate::ServiceError;

use super::scan_impl;
use super::scan_plan::{DetectionMode, InstallRootStrategy};

/// Per-install auto-scan using a shared open catalog, detector, and full
/// `file_hash_cache` prefetch (see [`crate::catalog::open_auto_scan_batch`]).
pub(crate) fn scan_auto_in_shared_batch(
    storage: &SqliteStorage,
    detector: &LibraryPatternComponentDetector,
    prefetched_cache: &FileHashCache,
    path: &std::path::Path,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    scan_impl(
        super::ScanInputs { storage, detector },
        path,
        DetectionMode::FastCachedWithFullFallback,
        InstallRootStrategy::SingleInstall,
        Some(prefetched_cache),
    )
}
