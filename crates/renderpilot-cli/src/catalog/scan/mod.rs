mod install_partitioner;
mod paths;
mod prune;
mod scan_plan;

#[cfg(windows)]
mod auto;

#[cfg(windows)]
pub(super) use auto::scan_auto_in_shared_batch;
#[cfg(windows)]
pub(crate) use prune::prune_auto_scan_orphans;

use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use install_partitioner::derive_install_roots;
use renderpilot_application::{AppError, AppResult, ComponentRepository};
use renderpilot_detection::{DetectedLibraryFile, FileHashCache, LibraryPatternComponentDetector};
use renderpilot_domain::{GameId, GameInstallation, GraphicsComponent, LibraryArtifact};
use renderpilot_platform_windows::ManualFolderGameSource;
use renderpilot_storage_sqlite::{FileHashCacheRow, ScanWriteUnit, SqliteStorage};
use scan_plan::{decide_fast_scan_fallback, DetectionMode, InstallRootStrategy};

use crate::error::CliError;

use super::{storage::open_catalog_storage, ScanFolderCatalogResult};

struct DiscoveredInstall {
    normalized_prefix: String,
    game: GameInstallation,
}

/// Scans `path` for manual-folder game installations.
///
/// The scan intentionally performs one filesystem detection pass over the selected root.
/// If multiple sibling game installs are detected under the selected path, detected
/// library files are reassigned to the best matching sub-installation by longest path
/// prefix.
///
/// Example:
///
/// ```text
/// D:/SteamLibrary/
///   steamapps/common/GameA/nvngx_dlss.dll
///   steamapps/common/GameB/bin/x64/nvngx_dlss.dll
/// ```
///
/// Shared prefix:
///
/// ```text
/// steamapps/common
/// ```
///
/// First diverging components:
///
/// ```text
/// GameA
/// GameB
/// ```
///
/// Result: two separate manual game installations.
pub(super) fn scan_folder_impl(path: PathBuf) -> Result<Vec<ScanFolderCatalogResult>, CliError> {
    let storage = open_catalog_storage()?;
    let detector = LibraryPatternComponentDetector::windows_default()
        .map_err(|error| AppError::detection_failed(error.to_string()))?;

    scan_impl(
        ScanInputs {
            storage: &storage,
            detector: &detector,
        },
        path,
        DetectionMode::FullCached,
        InstallRootStrategy::SplitByFirstDiverge,
        None,
    )
}

/// Borrowed storage + detector for one [`scan_impl`] invocation.
struct ScanInputs<'a> {
    storage: &'a SqliteStorage,
    detector: &'a LibraryPatternComponentDetector,
}

fn scan_impl(
    inputs: ScanInputs<'_>,
    path: impl Into<PathBuf>,
    detection_mode: DetectionMode,
    install_root_strategy: InstallRootStrategy,
    prefetched_cache: Option<&FileHashCache>,
) -> Result<Vec<ScanFolderCatalogResult>, CliError> {
    let storage = inputs.storage;
    let detector = inputs.detector;

    let selected_game = ManualFolderGameSource::new(path).discover_game()?;
    let scope_root = selected_game.install_path().as_str().to_owned();

    let libraries = detect_libraries(
        storage,
        detector,
        &selected_game,
        detection_mode,
        prefetched_cache,
    )?;

    let install_roots = derive_install_roots(&selected_game, &libraries, install_root_strategy);

    let results = persist_scan_results(storage, selected_game, libraries, install_roots)?;

    prune::prune_stale_manual_games_under_scope(
        storage,
        &scope_root,
        &prune::game_ids_from_scan_results(&results),
    )?;

    Ok(results)
}

fn detect_libraries(
    storage: &SqliteStorage,
    detector: &LibraryPatternComponentDetector,
    game: &GameInstallation,
    mode: DetectionMode,
    prefetched_cache: Option<&FileHashCache>,
) -> Result<Vec<DetectedLibraryFile>, CliError> {
    // When a batch caller has already loaded the entire cache into memory,
    // skip the per-game `SELECT … LIKE` round-trip and use that view
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

    let existing_component_count = match mode {
        DetectionMode::FastCachedWithFullFallback => {
            storage.list_components_for_game(game.id())?.len()
        }
        DetectionMode::FullCached => 0,
    };

    let libraries = match mode {
        DetectionMode::FullCached => detect_libraries_full_cached(detector, game, hash_cache)?,
        DetectionMode::FastCachedWithFullFallback => {
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
) -> Result<Vec<DetectedLibraryFile>, CliError> {
    if hash_cache.is_empty() {
        detector.detect_library_files(game).map_err(Into::into)
    } else {
        detector
            .detect_library_files_with_cache(game, hash_cache)
            .map_err(Into::into)
    }
}

fn detect_libraries_fast_cached_with_full_fallback(
    detector: &LibraryPatternComponentDetector,
    game: &GameInstallation,
    hash_cache: &FileHashCache,
    existing_component_count: usize,
) -> Result<Vec<DetectedLibraryFile>, CliError> {
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

fn legacy_fast_fallback_forces_full_scan() -> bool {
    std::env::var_os("RENDERPILOT_SCAN_FAST_FALLBACK_LEGACY").is_some()
}

fn persist_scan_results(
    storage: &SqliteStorage,
    selected_game: GameInstallation,
    libraries: Vec<DetectedLibraryFile>,
    install_roots: Vec<PathBuf>,
) -> Result<Vec<ScanFolderCatalogResult>, CliError> {
    if install_roots.len() <= 1 {
        return persist_single_scan_result(storage, selected_game, libraries);
    }

    persist_split_scan_results(storage, &selected_game, libraries, install_roots)
}

fn persist_single_scan_result(
    storage: &SqliteStorage,
    game: GameInstallation,
    libraries: Vec<DetectedLibraryFile>,
) -> Result<Vec<ScanFolderCatalogResult>, CliError> {
    Ok(vec![persist_scan_result(storage, game, libraries)?])
}

fn persist_split_scan_results(
    storage: &SqliteStorage,
    selected_game: &GameInstallation,
    libraries: Vec<DetectedLibraryFile>,
    install_roots: Vec<PathBuf>,
) -> Result<Vec<ScanFolderCatalogResult>, CliError> {
    let installs = discover_sub_installations(install_roots)?;
    let buckets = bucket_libraries_by_longest_install_prefix(libraries, &installs)?;

    let mut results = Vec::with_capacity(installs.len());

    for (install, libraries) in installs.into_iter().zip(buckets) {
        results.push(persist_scan_result(storage, install.game, libraries)?);
    }

    delete_stale_parent_game_if_needed(storage, selected_game, &results)?;

    Ok(results)
}

fn discover_sub_installations(
    install_roots: Vec<PathBuf>,
) -> Result<Vec<DiscoveredInstall>, CliError> {
    install_roots
        .into_iter()
        .map(discover_sub_installation)
        .collect()
}

fn discover_sub_installation(install_root: PathBuf) -> Result<DiscoveredInstall, CliError> {
    let game = ManualFolderGameSource::new(install_root).discover_game()?;
    let normalized_prefix = game.install_path().as_str().to_owned();

    Ok(DiscoveredInstall {
        normalized_prefix,
        game,
    })
}

/// Deletes an old parent scan row only when the selected game was not also detected
/// as one of the current scan results.
///
/// Without this guard, a split scan can accidentally delete a freshly saved result
/// if one of the derived install roots is equal to the originally selected root.
fn delete_stale_parent_game_if_needed(
    storage: &SqliteStorage,
    selected_game: &GameInstallation,
    results: &[ScanFolderCatalogResult],
) -> Result<(), CliError> {
    let selected_game_is_current = results
        .iter()
        .any(|result| result.game.id() == selected_game.id());

    if !selected_game_is_current {
        let catalog_path = crate::catalog::storage::catalog_database_path()?;
        let deleted = storage.delete_game(selected_game.id())?;
        crate::catalog::covers::unlink_cover_file_best_effort(
            &catalog_path,
            deleted.old_cover_file_name,
        );
    }

    Ok(())
}

/// Assigns every detected library to exactly one install.
/// The longest normalized install-path prefix wins.
///
/// Unlike the previous version, this function refuses to silently drop libraries
/// that do not match any discovered install.
fn bucket_libraries_by_longest_install_prefix(
    libraries: Vec<DetectedLibraryFile>,
    installs: &[DiscoveredInstall],
) -> Result<Vec<Vec<DetectedLibraryFile>>, CliError> {
    let mut buckets = empty_library_buckets(installs.len());
    let mut unmatched_paths = Vec::new();

    for library in libraries {
        match best_install_bucket_idx(&library, installs) {
            Some(bucket_idx) => buckets[bucket_idx].push(library),
            None => unmatched_paths.push(library.file_path().as_str().to_owned()),
        }
    }

    if !unmatched_paths.is_empty() {
        return Err(AppError::detection_failed(format!(
            "detected libraries could not be assigned to any discovered install: {}",
            unmatched_paths.join(", ")
        ))
        .into());
    }

    Ok(buckets)
}

fn empty_library_buckets(count: usize) -> Vec<Vec<DetectedLibraryFile>> {
    (0..count).map(|_| Vec::new()).collect()
}

fn best_install_bucket_idx(
    library: &DetectedLibraryFile,
    installs: &[DiscoveredInstall],
) -> Option<usize> {
    let library_path = library.file_path().as_str();

    installs
        .iter()
        .enumerate()
        .filter(|(_, install)| {
            paths::normalized_path_within_scope(library_path, &install.normalized_prefix)
        })
        .max_by_key(|(_, install)| install.normalized_prefix.len())
        .map(|(idx, _)| idx)
}

fn persist_scan_result(
    storage: &SqliteStorage,
    game: GameInstallation,
    libraries: Vec<DetectedLibraryFile>,
) -> Result<ScanFolderCatalogResult, CliError> {
    let components = build_graphics_components(&game, &libraries)?;
    let artifacts = build_library_artifacts(game.id(), &libraries)?;

    storage.save_scan_write_unit(ScanWriteUnit {
        game: &game,
        components: &components,
        artifacts: &artifacts,
    })?;

    Ok(ScanFolderCatalogResult { game, libraries })
}

fn build_graphics_components(
    game: &GameInstallation,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<GraphicsComponent>> {
    // Components and artifacts are grouped by the same `(directory, family)` rule
    // so a detected bundle (e.g. FSR 4's three DLLs) yields one component and one
    // matching artifact instead of three independent single-file entries.
    renderpilot_detection::group_into_components(game, libraries)
}

fn build_library_artifacts(
    game_id: &GameId,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<LibraryArtifact>> {
    renderpilot_detection::group_into_artifacts(game_id, libraries)
}

pub(super) fn normalized_install_path_buf(game: &GameInstallation) -> PathBuf {
    PathBuf::from(game.install_path().as_str())
}

fn library_file_path(library: &DetectedLibraryFile) -> &Path {
    Path::new(library.file_path().as_str())
}

/// Detects sub-directory roots that should be treated as separate game installs.
///
/// Returns `[root]` when the scan result looks like a single installation.
pub(super) fn detect_game_install_roots(
    root: &Path,
    libraries: &[DetectedLibraryFile],
) -> Vec<PathBuf> {
    let relative_library_dirs = relative_library_parent_dirs(root, libraries);

    if relative_library_dirs.is_empty() {
        return vec![root.to_path_buf()];
    }

    let common_prefix_len = longest_common_prefix_len(&relative_library_dirs);

    let install_roots =
        split_dirs_by_first_diverging_component(root, &relative_library_dirs, common_prefix_len);

    if install_roots.len() <= 1 {
        // Single install: keep the user-selected root instead of replacing it
        // with a derived common-prefix folder.
        vec![root.to_path_buf()]
    } else {
        install_roots
    }
}

fn relative_library_parent_dirs(
    root: &Path,
    libraries: &[DetectedLibraryFile],
) -> Vec<Vec<OsString>> {
    libraries
        .iter()
        .filter_map(|library| relative_library_parent_dir(root, library))
        .collect()
}

fn relative_library_parent_dir(
    root: &Path,
    library: &DetectedLibraryFile,
) -> Option<Vec<OsString>> {
    let parent = library_file_path(library).parent()?;
    let relative_parent = parent.strip_prefix(root).ok()?;

    Some(path_components(relative_parent))
}

fn path_components(path: &Path) -> Vec<OsString> {
    path.components()
        .map(|component| component.as_os_str().to_os_string())
        .collect()
}

fn split_dirs_by_first_diverging_component(
    root: &Path,
    relative_dirs: &[Vec<OsString>],
    common_prefix_len: usize,
) -> Vec<PathBuf> {
    let mut install_roots_by_key = BTreeMap::new();

    for relative_dir in relative_dirs {
        let install_root = install_root_for_relative_dir(root, relative_dir, common_prefix_len);
        let key = install_root_key(relative_dir, common_prefix_len);

        install_roots_by_key.entry(key).or_insert(install_root);
    }

    install_roots_by_key.into_values().collect()
}

fn install_root_for_relative_dir(
    root: &Path,
    relative_dir: &[OsString],
    common_prefix_len: usize,
) -> PathBuf {
    let mut install_root = root.to_path_buf();

    for component in &relative_dir[..common_prefix_len] {
        install_root.push(Path::new(component));
    }

    if let Some(diverging_component) = relative_dir.get(common_prefix_len) {
        install_root.push(Path::new(diverging_component));
    }

    install_root
}

fn install_root_key(relative_dir: &[OsString], common_prefix_len: usize) -> String {
    relative_dir
        .get(common_prefix_len)
        .map(|component| component.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Longest shared prefix length across parallel component lists.
fn longest_common_prefix_len(dirs: &[Vec<OsString>]) -> usize {
    let Some(first) = dirs.first() else {
        return 0;
    };

    dirs.iter()
        .skip(1)
        .map(|dir| shared_prefix_len(first, dir))
        .fold(first.len(), usize::min)
}

fn shared_prefix_len(left: &[OsString], right: &[OsString]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(a, b)| a == b)
        .count()
}

pub(super) fn populate_file_hash_cache(rows: Vec<FileHashCacheRow>) -> FileHashCache {
    let mut cache = FileHashCache::with_capacity(rows.len());

    for row in rows {
        cache.insert(row.path, row.size, row.modified_at, row.sha256, row.version);
    }

    cache
}

fn load_hash_cache(storage: &SqliteStorage, prefix: &str) -> Result<FileHashCache, CliError> {
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
fn save_hash_cache(
    storage: &SqliteStorage,
    libraries: &[DetectedLibraryFile],
) -> Result<(), CliError> {
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

#[cfg(test)]
mod tests {
    use super::{decide_fast_scan_fallback, scan_plan::FastScanFallbackReason};

    #[test]
    fn fallback_when_fast_result_is_empty() {
        let decision = decide_fast_scan_fallback(0, 3, 2);
        assert_eq!(
            decision.fallback_reason,
            Some(FastScanFallbackReason::EmptyFastResult)
        );
    }

    #[test]
    fn fallback_when_fast_result_is_incomplete_against_detectable_count() {
        let decision = decide_fast_scan_fallback(2, 3, 0);
        assert_eq!(
            decision.fallback_reason,
            Some(FastScanFallbackReason::IncompleteFastResult)
        );
    }

    #[test]
    fn fallback_when_fast_result_degrades_existing_catalog() {
        let decision = decide_fast_scan_fallback(2, 2, 3);
        assert_eq!(
            decision.fallback_reason,
            Some(FastScanFallbackReason::DegradedComparedToCatalog)
        );
    }

    #[test]
    fn keep_fast_result_when_complete_and_not_degraded() {
        let decision = decide_fast_scan_fallback(3, 3, 3);
        assert_eq!(decision.fallback_reason, None);
    }
}
