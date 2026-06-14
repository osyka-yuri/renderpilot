//! Path normalization, cache-scope filtering, and ordering helpers used by the
//! detection pipeline.

use std::path::{Path, PathBuf};

use renderpilot_application::AppResult;
use renderpilot_domain::{GameInstallation, PathRef};

use crate::error::detection_error;
use crate::file_metadata::FileHashCache;

use super::DetectedLibraryFile;

/// Returns cached file paths that lexically belong under `root`.
///
/// Existence is **not** verified here. [`crate::file_metadata::try_read_detected_file_metadata`]
/// returns `Ok(None)` for missing files (stale cache entries), so those paths
/// are skipped without failing the whole scan.
pub(super) fn cached_files_under_root(cache: &FileHashCache, root: &Path) -> Vec<PathBuf> {
    let scope = normalized_scope_prefix(root);

    sorted_unique_paths(
        cache
            .keys()
            .map(PathBuf::from)
            .filter(|path| path_in_scope(path, &scope)),
    )
}

fn path_in_scope(path: &Path, scope: &str) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");

    normalized == scope
        || normalized
            .strip_prefix(scope)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn normalized_scope_prefix(root: &Path) -> String {
    let normalized = root.to_string_lossy().replace('\\', "/");

    if normalized.ends_with('/') && normalized.len() > 1 {
        normalized.trim_end_matches('/').to_owned()
    } else {
        normalized
    }
}

pub(super) fn sorted_unique_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut paths: Vec<_> = paths.into_iter().collect();

    paths.sort_unstable();
    paths.dedup();

    paths
}

pub(super) fn sort_detected_library_files(files: &mut Vec<DetectedLibraryFile>) {
    files.sort_by(|left, right| left.file_path.as_str().cmp(right.file_path.as_str()));
    files.dedup_by(|left, right| left.file_path == right.file_path);
}

pub(super) fn install_root_path(game: &GameInstallation) -> PathBuf {
    PathBuf::from(game.install_path().as_str())
}

pub(super) fn file_name_for_matching(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

pub(super) fn path_ref_from_path(path: &Path) -> AppResult<PathRef> {
    let raw_path = path.to_string_lossy();
    let normalized_path = raw_path.replace('\\', "/");

    PathRef::new(normalized_path.as_str()).map_err(detection_error)
}
