use renderpilot_detection::{FileHashCache, LibraryPatternComponentDetector};

use crate::ServiceError;
use crate::catalog::ScanFolderCatalogResult;

use super::scan_impl;
use super::scan_plan::{DetectionMode, InstallRootStrategy};

/// Per-install auto-scan using a shared open catalog, detector, and full
/// `file_hash_cache` prefetch
/// (see [`open_auto_scan_batch`](crate::catalog::auto_scan::open_auto_scan_batch)).
pub(crate) fn scan_auto_in_shared_batch(
    context: &crate::Context,
    detector: &LibraryPatternComponentDetector,
    prefetched_cache: &FileHashCache,
    path: &std::path::Path,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    // `ScanInputs` / `scan_impl` are private to `scan`; this sibling module may
    // construct them because it lives under the same parent.
    scan_impl(
        super::ScanInputs { context, detector },
        path,
        DetectionMode::FastCachedWithFullFallback,
        InstallRootStrategy::SingleInstall,
        Some(prefetched_cache),
    )
}
