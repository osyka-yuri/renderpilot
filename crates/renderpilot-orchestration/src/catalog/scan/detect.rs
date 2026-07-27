//! Library-file detection modes for a single install root.

use renderpilot_detection::{DetectedLibraryFile, FileHashCache, LibraryPatternComponentDetector};
use renderpilot_domain::GameInstallation;
use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

use super::hash_cache::{load_hash_cache, save_hash_cache};
use super::scan_plan::DetectionMode;

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
