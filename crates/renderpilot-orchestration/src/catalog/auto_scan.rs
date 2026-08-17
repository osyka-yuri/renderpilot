//! Batched auto-scan with one reconciler per discovered installation.

use renderpilot_application::AppError;
use renderpilot_detection::LibraryPatternComponentDetector;
use renderpilot_platform_windows::game_libraries::DiscoveredInstall;

use crate::ServiceError;

use super::ScanFolderCatalogResult;
use super::scan::scan_auto_in_shared_batch;

/// Reusable per-batch state produced by [`open_auto_scan_batch`] and consumed
/// by every call to [`scan_auto_in_batch`] in an auto-scan loop.
pub struct AutoScanBatch<'a> {
    context: &'a crate::Context,
    detector: LibraryPatternComponentDetector,
    catalog_index: super::scan::CatalogInstallIndex,
}

impl AutoScanBatch<'_> {
    /// Returns a reference to the batch's shared context.
    pub fn context(&self) -> &crate::Context {
        self.context
    }
}

/// Opens shared resources for a batched auto-scan.
///
/// Caller must drive the loop with [`scan_auto_in_batch`] for each install
/// directory. Built once, the batch:
///
/// - holds a single open SQLite connection (one set of `PRAGMA`s + migrations),
/// - reuses the compiled library-pattern detector,
/// - keeps only the catalog identity index needed for reconciliation.
pub fn open_auto_scan_batch(context: &crate::Context) -> Result<AutoScanBatch<'_>, ServiceError> {
    let detector = LibraryPatternComponentDetector::windows_default()
        .map_err(|error| AppError::detection_failed(error.to_string()))?;
    let catalog_index = super::scan::CatalogInstallIndex::load(context.storage())?;

    Ok(AutoScanBatch {
        context,
        detector,
        catalog_index,
    })
}

/// Per-install entry point used inside an [`AutoScanBatch`] loop.
pub fn scan_auto_in_batch(
    batch: &AutoScanBatch<'_>,
    install: &DiscoveredInstall,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    let results = scan_auto_in_shared_batch(
        batch.context(),
        &batch.detector,
        &batch.catalog_index,
        install,
    )?;
    Ok(results)
}
