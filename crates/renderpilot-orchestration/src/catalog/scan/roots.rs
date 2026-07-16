//! Multi-install root derivation and library-to-install bucketing.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use renderpilot_application::AppError;
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::GameInstallation;
use renderpilot_platform_windows::ManualFolderGameSource;

use crate::ServiceError;

use super::paths;

/// One discovered sub-install under a multi-root parent scan.
pub(super) struct DiscoveredInstall {
    pub(super) normalized_prefix: String,
    pub(super) game: GameInstallation,
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

pub(super) fn discover_sub_installations(
    install_roots: Vec<PathBuf>,
) -> Result<Vec<DiscoveredInstall>, ServiceError> {
    install_roots
        .into_iter()
        .map(discover_sub_installation)
        .collect()
}

fn discover_sub_installation(install_root: PathBuf) -> Result<DiscoveredInstall, ServiceError> {
    let game = ManualFolderGameSource::new(install_root).discover_game()?;
    let normalized_prefix = game.install_path().as_str().to_owned();

    Ok(DiscoveredInstall {
        normalized_prefix,
        game,
    })
}

/// Assigns every detected library to exactly one install.
/// The longest normalized install-path prefix wins.
///
/// Unlike the previous version, this function refuses to silently drop libraries
/// that do not match any discovered install.
pub(super) fn bucket_libraries_by_longest_install_prefix(
    libraries: Vec<DetectedLibraryFile>,
    installs: &[DiscoveredInstall],
) -> Result<Vec<Vec<DetectedLibraryFile>>, ServiceError> {
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
