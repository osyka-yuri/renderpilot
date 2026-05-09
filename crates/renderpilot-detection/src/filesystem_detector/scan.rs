use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use renderpilot_application::AppResult;

use crate::error::detection_context_error;

const SYSTEM_DIRECTORY_NAMES: &[&str] = &[
    "windows",
    "system32",
    "syswow64",
    "system volume information",
    "$recycle.bin",
];

/// Predicate driving the walker's per-file early-rejection.
///
/// Implemented for both `()` (no filter — the legacy walker behaviour) and any
/// `Fn(&str) -> bool` so callers can pass a closure that pre-filters by
/// extension via [`crate::CandidateFileExtensions::allows_file_name`].
pub(super) trait FileNameFilter {
    fn should_consider(&self, file_name: &str) -> bool;
}

impl FileNameFilter for () {
    fn should_consider(&self, _file_name: &str) -> bool {
        true
    }
}

impl<F: Fn(&str) -> bool> FileNameFilter for F {
    fn should_consider(&self, file_name: &str) -> bool {
        self(file_name)
    }
}

/// Walks `root` (up to `max_depth` levels deep) and returns every file the
/// caller should consider during detection.
///
/// `name_filter` rejects leaf files using only [`fs::DirEntry::file_name`] and
/// [`fs::DirEntry::file_type`] when the entry is clearly a regular file, so
/// most non-matching assets never incur `fs::symlink_metadata`. Directories
/// are still opened and recursed; symlink targets and ambiguous entries fall
/// back to `symlink_metadata` for correctness.
pub(super) fn collect_files_filtered(
    root: &Path,
    max_depth: usize,
    name_filter: impl FileNameFilter,
) -> AppResult<Vec<PathBuf>> {
    let mut collector = FileCollector::new(max_depth, name_filter);

    collector.collect(root)?;

    Ok(collector.into_sorted_files())
}

#[derive(Debug)]
struct FileCollector<F: FileNameFilter> {
    max_depth: usize,
    name_filter: F,
    files: Vec<PathBuf>,
}

impl<F: FileNameFilter> FileCollector<F> {
    fn new(max_depth: usize, name_filter: F) -> Self {
        Self {
            max_depth,
            name_filter,
            files: Vec::new(),
        }
    }

    fn collect(&mut self, root: &Path) -> AppResult<()> {
        let metadata = read_symlink_metadata(root)?;

        if is_symlink(&metadata) {
            return Ok(());
        }

        if metadata.is_file() {
            self.visit_file_path(root, 0);
            return Ok(());
        }

        if metadata.is_dir() {
            self.visit_directory(root, 0)?;
        }

        Ok(())
    }

    fn into_sorted_files(mut self) -> Vec<PathBuf> {
        self.files.sort_unstable();
        self.files
    }

    fn visit_directory(&mut self, path: &Path, dir_depth: usize) -> AppResult<()> {
        if self.should_skip_directory(path, dir_depth) {
            return Ok(());
        }

        for entry in read_sorted_dir_entries(path)? {
            self.visit_child_entry(entry, dir_depth)?;
        }

        Ok(())
    }

    /// `parent_dir_depth` is the depth of the directory whose children we are visiting.
    fn visit_child_entry(&mut self, entry: fs::DirEntry, parent_dir_depth: usize) -> AppResult<()> {
        let path = entry.path();
        let child_depth = parent_dir_depth + 1;

        let Some(file_type) = read_entry_file_type_tolerant(&entry, &path)? else {
            return Ok(());
        };

        if file_type.is_symlink() {
            return Ok(());
        }

        if file_type.is_file() {
            if child_depth > self.max_depth {
                return Ok(());
            }

            let os_name = entry.file_name();
            let Some(name) = os_name.to_str() else {
                return Ok(());
            };

            if !self.name_filter.should_consider(name) {
                return Ok(());
            }

            self.files.push(path);
            return Ok(());
        }

        if file_type.is_dir() {
            if self.should_skip_directory(&path, child_depth) {
                return Ok(());
            }

            // Re-check symlinks/junctions that can look like directories in `file_type`.
            let Some(md) = read_symlink_metadata_tolerant(&path)? else {
                return Ok(());
            };

            if is_symlink(&md) {
                return Ok(());
            }
            if md.is_dir() {
                self.visit_directory(&path, child_depth)?;
            }
        }

        Ok(())
    }

    fn visit_file_path(&mut self, path: &Path, depth: usize) {
        if depth > self.max_depth {
            return;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };

        if !self.name_filter.should_consider(file_name) {
            return;
        }

        self.files.push(path.to_path_buf());
    }

    fn should_skip_directory(&self, path: &Path, depth: usize) -> bool {
        depth >= self.max_depth || is_system_directory(path)
    }
}

fn read_symlink_metadata(path: &Path) -> AppResult<fs::Metadata> {
    fs::symlink_metadata(path).map_err(|error| {
        detection_context_error(format_args!("could not read {}", path.display()), error)
    })
}

/// Returns `Ok(None)` when the entry vanished between `read_dir` and the
/// follow-up syscall (Steam updates, AV scanner, search indexer). The walker
/// then skips this entry instead of aborting the whole scan, mirroring the
/// tolerant policy of [`crate::file_metadata::try_read_detected_file_metadata`].
fn read_symlink_metadata_tolerant(path: &Path) -> AppResult<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(detection_context_error(
            format_args!("could not read {}", path.display()),
            error,
        )),
    }
}

/// Resolves a `DirEntry::file_type`, falling back to a tolerant
/// `symlink_metadata` read when `file_type()` itself fails (rare on Windows
/// when the entry already disappeared). Returns `Ok(None)` when neither call
/// can see the entry anymore.
fn read_entry_file_type_tolerant(
    entry: &fs::DirEntry,
    path: &Path,
) -> AppResult<Option<fs::FileType>> {
    match entry.file_type() {
        Ok(file_type) => Ok(Some(file_type)),
        Err(_) => Ok(read_symlink_metadata_tolerant(path)?.map(|md| md.file_type())),
    }
}

fn is_symlink(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn read_sorted_dir_entries(path: &Path) -> AppResult<Vec<fs::DirEntry>> {
    let entries = fs::read_dir(path).map_err(|error| {
        detection_context_error(
            format_args!("could not read directory {}", path.display()),
            error,
        )
    })?;

    let mut entries = entries
        .map(|entry| {
            entry.map_err(|error| {
                detection_context_error(
                    format_args!("could not enumerate directory {}", path.display()),
                    error,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    entries.sort_unstable_by_key(|entry| entry.file_name());

    Ok(entries)
}

fn is_system_directory(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };

    SYSTEM_DIRECTORY_NAMES
        .iter()
        .any(|system_name| name.eq_ignore_ascii_case(OsStr::new(system_name)))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{collect_files_filtered, read_symlink_metadata_tolerant};

    #[test]
    fn read_symlink_metadata_tolerant_returns_none_for_missing_path() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let missing = temp.path().join("vanished-during-scan.dll");

        let result = read_symlink_metadata_tolerant(&missing).expect("should not error");

        assert!(
            result.is_none(),
            "missing entry must surface as None instead of aborting the walk",
        );
    }

    #[test]
    fn name_filter_drops_files_with_other_extensions_before_metadata() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let root = temp.path();

        fs::write(root.join("nvngx_dlss.dll"), b"fake-dll").expect("write dll");
        fs::write(root.join("config.ini"), b"key=value").expect("write ini");
        fs::write(root.join("video.bik"), b"bik").expect("write bik");
        fs::write(root.join("noext"), b"foo").expect("write no-ext");

        let allow_dll = |file_name: &str| {
            let lower = file_name.to_ascii_lowercase();
            lower.ends_with(".dll")
        };

        let collected = collect_files_filtered(root, 3, allow_dll).expect("walk should succeed");

        let names = collected
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["nvngx_dlss.dll".to_owned()]);
    }

    #[test]
    fn name_filter_skips_many_wrong_extensions_under_load() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let root = temp.path();

        for index in 0..100 {
            fs::write(root.join(format!("asset_{index:03}.bik")), b"bik").expect("write bik");
        }
        fs::write(root.join("target.dll"), b"x").expect("write dll");
        fs::write(root.join("readme.ini"), b"ini").expect("write ini");

        let allow_dll = |file_name: &str| file_name.to_ascii_lowercase().ends_with(".dll");

        let collected = collect_files_filtered(root, 3, allow_dll).expect("walk should succeed");

        assert_eq!(collected.len(), 1);
        assert_eq!(
            collected[0].file_name().unwrap().to_string_lossy(),
            "target.dll"
        );
    }
}
