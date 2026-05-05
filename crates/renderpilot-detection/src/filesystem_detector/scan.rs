use std::{
    fs,
    path::{Path, PathBuf},
};

use renderpilot_application::AppResult;

use crate::error::detection_context_error;

pub(super) fn collect_files(root: &Path, max_depth: usize) -> AppResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_inner(root, 0, max_depth, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_inner(
    path: &Path,
    depth: usize,
    max_depth: usize,
    files: &mut Vec<PathBuf>,
) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        detection_context_error(format_args!("could not read {}", path.display()), error)
    })?;

    if metadata.file_type().is_symlink() {
        return Ok(());
    }

    if metadata.is_file() {
        if depth <= max_depth {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    if !metadata.is_dir() || depth >= max_depth || is_system_directory(path) {
        return Ok(());
    }

    let mut entries = read_directory_entries(path)?;

    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        collect_files_inner(&entry.path(), depth + 1, max_depth, files)?;
    }

    Ok(())
}

fn read_directory_entries(path: &Path) -> AppResult<Vec<fs::DirEntry>> {
    fs::read_dir(path)
        .map_err(|error| {
            detection_context_error(
                format_args!("could not read directory {}", path.display()),
                error,
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            detection_context_error(
                format_args!("could not enumerate directory {}", path.display()),
                error,
            )
        })
}

fn is_system_directory(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    matches!(
        name.to_ascii_lowercase().as_str(),
        "windows" | "system32" | "syswow64" | "system volume information" | "$recycle.bin"
    )
}