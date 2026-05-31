//! Shared filesystem traversal constants and helpers for platform detectors.

use std::{fs, path::Path};

/// The maximum allowed recursion depth when traversing game installation directories.
pub const MAX_DEPTH: u32 = 5;

/// A list of directory names to ignore during installation directory traversals.
pub const SKIP_DIRS: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    "_dlsswapper_backups",
    "_renderpilot_backups",
];

/// Recursively traverses the `dir` directory up to `MAX_DEPTH` levels deep,
/// ignoring any subdirectories whose names match entries in `SKIP_DIRS` (case-insensitive).
///
/// The `on_file` callback is invoked for every regular file found.
/// The `depth` parameter is 0 for files located directly inside `dir`.
///
/// Directory read errors are silently ignored to ensure that a single unreadable folder
/// does not interrupt the entire traversal process. This maintains the same behavior
/// as previous per-module private traversal functions.
pub(crate) fn walk_files(
    dir: &Path,
    depth: u32,
    on_file: &mut dyn FnMut(&fs::DirEntry, &Path, u32),
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if SKIP_DIRS.iter().any(|skip| name.eq_ignore_ascii_case(skip)) {
                    continue;
                }
            }
            walk_files(&path, depth + 1, on_file);
            continue;
        }

        if file_type.is_file() {
            on_file(&entry, &path, depth);
        }
    }
}
