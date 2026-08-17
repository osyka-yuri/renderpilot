//! Path normalization, cache-scope filtering, and ordering helpers used by the
//! detection pipeline.

use std::path::{Path, PathBuf};

use renderpilot_application::AppResult;
use renderpilot_domain::{GameInstallation, PathRef};

use super::DetectedLibraryFile;
use crate::error::detection_error;

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
