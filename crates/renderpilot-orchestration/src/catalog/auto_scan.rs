//! Batched auto-scan: shared open storage, detector, and prefetched hash-cache.

use renderpilot_application::AppError;
use renderpilot_detection::{FileHashCache, LibraryPatternComponentDetector};
use renderpilot_platform_windows::game_libraries::DiscoveredInstall;
use renderpilot_storage_sqlite::SqliteStorage;
use std::collections::HashMap;

use crate::ServiceError;

use super::ScanFolderCatalogResult;
use super::scan::scan_auto_in_shared_batch;

/// Reusable per-batch state produced by [`open_auto_scan_batch`] and consumed
/// by every call to [`scan_auto_in_batch`] in an auto-scan loop.
pub struct AutoScanBatch<'a> {
    context: &'a crate::Context,
    detector: LibraryPatternComponentDetector,
    prefetched_cache: FileHashCache,
    catalog_index: super::scan::CatalogInstallIndex,
    checkpoints: HashMap<String, String>,
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
/// - prefetches the entire `file_hash_cache` table into memory in one query.
pub fn open_auto_scan_batch(context: &crate::Context) -> Result<AutoScanBatch<'_>, ServiceError> {
    let detector = LibraryPatternComponentDetector::windows_default()
        .map_err(|error| AppError::detection_failed(error.to_string()))?;
    let prefetched_cache = load_full_hash_cache(context.storage())?;
    let catalog_index = super::scan::CatalogInstallIndex::load(context.storage())?;
    let checkpoints = context
        .storage()
        .list_scan_source_checkpoints()?
        .into_iter()
        .collect();

    Ok(AutoScanBatch {
        context,
        detector,
        prefetched_cache,
        catalog_index,
        checkpoints,
    })
}

/// Per-install entry point used inside an [`AutoScanBatch`] loop.
pub fn scan_auto_in_batch(
    batch: &AutoScanBatch<'_>,
    install: &DiscoveredInstall,
    allow_checkpoint_skip: bool,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    let path = &install.install_path;
    if checkpoint_allows_skip(
        allow_checkpoint_skip,
        batch.catalog_index.contains_install_path(path),
        &batch.checkpoints,
        install.checkpoint.as_ref(),
    ) {
        log::debug!("auto-scan checkpoint hit for {}", path.display());
        return Ok(Vec::new());
    }

    let results = scan_auto_in_shared_batch(
        batch.context(),
        &batch.detector,
        &batch.prefetched_cache,
        &batch.catalog_index,
        install,
    )?;
    if let Some(source) = install.checkpoint.as_ref() {
        batch
            .context()
            .storage()
            .upsert_scan_source_checkpoint(&source.source_key, &source.fingerprint)?;
    }
    Ok(results)
}

fn checkpoint_allows_skip(
    allow_checkpoint_skip: bool,
    install_is_cataloged: bool,
    checkpoints: &HashMap<String, String>,
    source: Option<&renderpilot_platform_windows::SteamScanSourceFingerprint>,
) -> bool {
    allow_checkpoint_skip
        && install_is_cataloged
        && source
            .is_some_and(|source| checkpoints.get(&source.source_key) == Some(&source.fingerprint))
}

fn load_full_hash_cache(storage: &SqliteStorage) -> Result<FileHashCache, ServiceError> {
    let rows = storage.load_all_file_hash_cache()?;
    Ok(crate::catalog::scan::hash_cache::populate_file_hash_cache(
        rows,
    ))
}

#[cfg(test)]
mod tests {
    use super::checkpoint_allows_skip;
    use renderpilot_platform_windows::SteamScanSourceFingerprint;
    use std::collections::HashMap;

    #[test]
    fn checkpoint_skips_only_a_cataloged_install_with_an_exact_match() {
        let checkpoints = HashMap::from([("steam:42".to_owned(), "current".to_owned())]);
        let current = SteamScanSourceFingerprint {
            source_key: "steam:42".to_owned(),
            fingerprint: "current".to_owned(),
        };
        let stale = SteamScanSourceFingerprint {
            fingerprint: "stale".to_owned(),
            ..current.clone()
        };

        assert!(checkpoint_allows_skip(
            true,
            true,
            &checkpoints,
            Some(&current)
        ));
        assert!(!checkpoint_allows_skip(
            false,
            true,
            &checkpoints,
            Some(&current)
        ));
        assert!(!checkpoint_allows_skip(
            true,
            false,
            &checkpoints,
            Some(&current)
        ));
        assert!(!checkpoint_allows_skip(
            true,
            true,
            &checkpoints,
            Some(&stale)
        ));
        assert!(
            !checkpoint_allows_skip(true, true, &checkpoints, None),
            "manual and non-Steam scans cannot skip without a launcher checkpoint"
        );
    }
}
