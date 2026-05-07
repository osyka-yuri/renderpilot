use std::{
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

pub(super) fn collect_files(root: &Path, max_depth: usize) -> AppResult<Vec<PathBuf>> {
    let mut collector = FileCollector::new(max_depth);

    collector.collect(root)?;

    Ok(collector.into_sorted_files())
}

#[derive(Debug)]
struct FileCollector {
    max_depth: usize,
    files: Vec<PathBuf>,
}

impl FileCollector {
    fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            files: Vec::new(),
        }
    }

    fn collect(&mut self, root: &Path) -> AppResult<()> {
        self.visit_path(root, 0)
    }

    fn into_sorted_files(mut self) -> Vec<PathBuf> {
        self.files.sort_unstable();
        self.files
    }

    fn visit_path(&mut self, path: &Path, depth: usize) -> AppResult<()> {
        let metadata = read_symlink_metadata(path)?;

        if is_symlink(&metadata) {
            return Ok(());
        }

        if metadata.is_file() {
            self.visit_file(path, depth);
            return Ok(());
        }

        if metadata.is_dir() {
            self.visit_directory(path, depth)?;
        }

        Ok(())
    }

    fn visit_file(&mut self, path: &Path, depth: usize) {
        if depth <= self.max_depth {
            self.files.push(path.to_path_buf());
        }
    }

    fn visit_directory(&mut self, path: &Path, depth: usize) -> AppResult<()> {
        if self.should_skip_directory(path, depth) {
            return Ok(());
        }

        for child_path in read_sorted_directory_child_paths(path)? {
            self.visit_path(&child_path, depth + 1)?;
        }

        Ok(())
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

fn is_symlink(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn read_sorted_directory_child_paths(path: &Path) -> AppResult<Vec<PathBuf>> {
    let entries = fs::read_dir(path).map_err(|error| {
        detection_context_error(
            format_args!("could not read directory {}", path.display()),
            error,
        )
    })?;

    let mut child_paths = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|error| {
            detection_context_error(
                format_args!("could not enumerate directory {}", path.display()),
                error,
            )
        })?;

        child_paths.push(entry.path());
    }

    child_paths.sort_unstable();

    Ok(child_paths)
}

fn is_system_directory(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    SYSTEM_DIRECTORY_NAMES
        .iter()
        .any(|system_name| name.eq_ignore_ascii_case(system_name))
}
