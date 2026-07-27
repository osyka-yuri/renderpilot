use renderpilot_detection::{FileHashCache, LibraryPatternComponentDetector};
use renderpilot_platform_windows::{InstallIdentityDetails, ManualFolderGameSource};

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
    catalog_index: &super::reconcile::CatalogInstallIndex,
    path: &std::path::Path,
    known_identity: Option<&InstallIdentityDetails>,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    let mut source = ManualFolderGameSource::new(path);
    if let Some(identity) = known_identity {
        source = source.with_known_identity(identity.clone());
    }

    scan_impl(
        super::ScanInputs { context, detector },
        &source,
        // A checkpoint miss still requires one complete metadata walk. The
        // launcher identity itself was already resolved during discovery.
        DetectionMode::FullCached,
        InstallRootStrategy::SingleInstall,
        Some(prefetched_cache),
        Some(catalog_index),
    )
}
