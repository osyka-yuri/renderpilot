mod paths;
mod prune;

use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use renderpilot_application::{AppError, AppResult};
use renderpilot_detection::{DetectedLibraryFile, FileHashCache, LibraryPatternComponentDetector};
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, GameId, GameInstallation, GraphicsComponent,
    LibraryArtifact,
};
use renderpilot_platform_windows::ManualFolderGameSource;
use renderpilot_storage_sqlite::{FileHashCacheRow, SqliteStorage};

use crate::error::CliError;

use super::{storage::open_catalog_storage, ScanFolderCatalogResult};

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
    let detector = LibraryPatternComponentDetector::windows_default()?;

    let selected_game = ManualFolderGameSource::new(path).discover_game()?;
    let scope_root = selected_game.install_path().as_str().to_owned();

    let libraries = detect_libraries_with_cache(&storage, &detector, &selected_game)?;

    let install_roots =
        detect_game_install_roots(&normalized_install_path_buf(&selected_game), &libraries);

    let results = if install_roots.len() <= 1 {
        vec![persist_scan_result(&storage, selected_game, libraries)?]
    } else {
        persist_split_scan_results(&storage, &selected_game, libraries, install_roots)?
    };

    prune::prune_stale_manual_games_under_scope(
        &storage,
        &scope_root,
        &prune::game_ids_from_scan_results(&results),
    )?;

    Ok(results)
}

fn persist_split_scan_results(
    storage: &SqliteStorage,
    selected_game: &GameInstallation,
    libraries: Vec<DetectedLibraryFile>,
    install_roots: Vec<PathBuf>,
) -> Result<Vec<ScanFolderCatalogResult>, CliError> {
    let installs = discover_sub_installations(install_roots)?;
    let buckets = bucket_libraries_by_longest_install_prefix(libraries, &installs);

    let mut results = Vec::with_capacity(installs.len());

    for ((_, game), libraries) in installs.into_iter().zip(buckets) {
        results.push(persist_scan_result(storage, game, libraries)?);
    }

    delete_stale_parent_game_if_needed(storage, selected_game, &results)?;

    Ok(results)
}

fn discover_sub_installations(
    install_roots: Vec<PathBuf>,
) -> Result<Vec<(String, GameInstallation)>, CliError> {
    let mut installs = Vec::with_capacity(install_roots.len());

    for install_root in install_roots {
        let game = ManualFolderGameSource::new(install_root).discover_game()?;
        let normalized_prefix = game.install_path().as_str().to_owned();

        installs.push((normalized_prefix, game));
    }

    Ok(installs)
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
        storage.delete_game(selected_game.id())?;
    }

    Ok(())
}

/// Assigns every detected library to exactly one install.
/// The longest normalized install-path prefix wins.
fn bucket_libraries_by_longest_install_prefix(
    libraries: Vec<DetectedLibraryFile>,
    installs: &[(String, GameInstallation)],
) -> Vec<Vec<DetectedLibraryFile>> {
    let mut buckets = empty_library_buckets(installs.len());

    for library in libraries {
        if let Some(bucket_idx) = best_install_bucket_idx(&library, installs) {
            buckets[bucket_idx].push(library);
        }
    }

    buckets
}

fn empty_library_buckets(count: usize) -> Vec<Vec<DetectedLibraryFile>> {
    (0..count).map(|_| Vec::new()).collect()
}

fn best_install_bucket_idx(
    library: &DetectedLibraryFile,
    installs: &[(String, GameInstallation)],
) -> Option<usize> {
    let library_path = library.file_path().as_str();

    installs
        .iter()
        .enumerate()
        .filter(|(_, (install_prefix, _))| {
            paths::normalized_path_within_scope(library_path, install_prefix)
        })
        .max_by_key(|(_, (install_prefix, _))| install_prefix.len())
        .map(|(idx, _)| idx)
}

fn detect_libraries_with_cache(
    storage: &SqliteStorage,
    detector: &LibraryPatternComponentDetector,
    game: &GameInstallation,
) -> Result<Vec<DetectedLibraryFile>, CliError> {
    let hash_cache = load_hash_cache(storage, game.install_path().as_str())?;

    let libraries = if hash_cache.is_empty() {
        detector.detect_library_files(game)?
    } else {
        detector.detect_library_files_with_cache(game, &hash_cache)?
    };

    save_hash_cache(storage, &libraries)?;

    Ok(libraries)
}

fn persist_scan_result(
    storage: &SqliteStorage,
    game: GameInstallation,
    libraries: Vec<DetectedLibraryFile>,
) -> Result<ScanFolderCatalogResult, CliError> {
    let components = build_graphics_components(&game, &libraries)?;
    let artifacts = build_library_artifacts(game.id(), &libraries)?;

    storage.save_scan_result(&game, &components, &artifacts)?;

    Ok(ScanFolderCatalogResult { game, libraries })
}

fn build_graphics_components(
    game: &GameInstallation,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<GraphicsComponent>> {
    libraries
        .iter()
        .cloned()
        .map(|library| library.into_component(game))
        .collect()
}

fn build_library_artifacts(
    game_id: &GameId,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<LibraryArtifact>> {
    libraries
        .iter()
        .map(|library| build_scanned_artifact(library, game_id))
        .collect()
}

fn normalized_install_path_buf(game: &GameInstallation) -> PathBuf {
    PathBuf::from(game.install_path().as_str())
}

fn library_file_path(library: &DetectedLibraryFile) -> &Path {
    Path::new(library.file_path().as_str())
}

/// Detects sub-directory roots that should be treated as separate game installs.
///
/// Returns `[root]` when the scan result looks like a single installation.
fn detect_game_install_roots(root: &Path, libraries: &[DetectedLibraryFile]) -> Vec<PathBuf> {
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

fn build_scanned_artifact(
    library: &DetectedLibraryFile,
    game_id: &GameId,
) -> AppResult<LibraryArtifact> {
    let artifact_id = ArtifactId::new(format!("artifact:{}", library.sha256().as_str()))
        .map_err(domain_to_detection_error)?;

    let file = component_file_from_detected_library(library);

    LibraryArtifact::new(
        artifact_id,
        library.technology(),
        library.file_name(),
        file,
        ArtifactTrustLevel::LocalObserved,
    )
    .map_err(domain_to_detection_error)?
    .with_source("scan-folder")
    .map_err(domain_to_detection_error)
    .map(|artifact| artifact.with_source_game_id(game_id.clone()))
}

fn component_file_from_detected_library(library: &DetectedLibraryFile) -> ComponentFile {
    let mut file =
        ComponentFile::new(library.file_path().clone()).with_sha256(library.sha256().clone());

    if let Some(version) = library.version().cloned() {
        file = file.with_version(version);
    }

    file
}

fn domain_to_detection_error(error: impl std::fmt::Display) -> AppError {
    AppError::detection_failed(error.to_string())
}

fn load_hash_cache(storage: &SqliteStorage, prefix: &str) -> Result<FileHashCache, CliError> {
    let rows = storage.load_file_hash_cache(prefix)?;
    let mut cache = FileHashCache::with_capacity(rows.len());

    for row in rows {
        cache.insert(row.path, row.size, row.modified_at, row.sha256, row.version);
    }

    Ok(cache)
}

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
