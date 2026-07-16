//! Library-file detection modes for a single install root.

use renderpilot_detection::{DetectedLibraryFile, FileHashCache, LibraryPatternComponentDetector};
use renderpilot_domain::GameInstallation;
use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

use super::hash_cache::{load_hash_cache, save_hash_cache};
use super::scan_plan::DetectionMode;
#[cfg(windows)]
use super::scan_plan::decide_fast_scan_fallback;
#[cfg(windows)]
use renderpilot_application::ComponentRepository;

pub(super) fn detect_libraries(
    storage: &SqliteStorage,
    detector: &LibraryPatternComponentDetector,
    game: &GameInstallation,
    mode: DetectionMode,
    prefetched_cache: Option<&FileHashCache>,
) -> Result<Vec<DetectedLibraryFile>, ServiceError> {
    // When a batch caller has already loaded the entire cache into memory,
    // skip the per-game `SELECT ... LIKE` round-trip and use that view
    // directly. Per-install bytes that aren't under `game.install_path()` are
    // ignored downstream by `cached_files_under_root`.
    let owned_cache;
    let hash_cache: &FileHashCache = match prefetched_cache {
        Some(cache) => cache,
        None => {
            owned_cache = load_hash_cache(storage, game.install_path().as_str())?;
            &owned_cache
        }
    };

    let libraries = match mode {
        DetectionMode::FullCached => detect_libraries_full_cached(detector, game, hash_cache)?,
        #[cfg(windows)]
        DetectionMode::FastCachedWithFullFallback => {
            let existing_component_count = storage.list_components_for_game(game.id())?.len();
            detect_libraries_fast_cached_with_full_fallback(
                detector,
                game,
                hash_cache,
                existing_component_count,
            )?
        }
    };

    save_hash_cache(storage, &libraries)?;

    Ok(libraries)
}

fn detect_libraries_full_cached(
    detector: &LibraryPatternComponentDetector,
    game: &GameInstallation,
    hash_cache: &FileHashCache,
) -> Result<Vec<DetectedLibraryFile>, ServiceError> {
    if hash_cache.is_empty() {
        detector.detect_library_files(game).map_err(Into::into)
    } else {
        detector
            .detect_library_files_with_cache(game, hash_cache)
            .map_err(Into::into)
    }
}

#[cfg(windows)]
fn detect_libraries_fast_cached_with_full_fallback(
    detector: &LibraryPatternComponentDetector,
    game: &GameInstallation,
    hash_cache: &FileHashCache,
    existing_component_count: usize,
) -> Result<Vec<DetectedLibraryFile>, ServiceError> {
    if hash_cache.is_empty() {
        return detector.detect_library_files(game).map_err(Into::into);
    }

    let fast_report = detector.detect_library_files_fast_cached_with_evidence(game, hash_cache)?;
    let fast_libraries = fast_report.libraries();
    let expected_detectable_count = fast_report.detectable_count();
    let decision = decide_fast_scan_fallback(
        fast_libraries.len(),
        expected_detectable_count,
        existing_component_count,
    );

    if decision.should_fallback() || legacy_fast_fallback_forces_full_scan() {
        detector
            .detect_library_files_with_cache(game, hash_cache)
            .map_err(Into::into)
    } else {
        Ok(fast_report.into_libraries())
    }
}

#[cfg(windows)]
fn legacy_fast_fallback_forces_full_scan() -> bool {
    std::env::var_os("RENDERPILOT_SCAN_FAST_FALLBACK_LEGACY").is_some()
}
