//! Shared low-level filesystem helpers used by apply and rollback paths.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::errors;
use crate::ServiceError;

/// Returns the path of an existing file in `game_dir` whose name equals `name`
/// case-insensitively.
pub(crate) fn existing_case_insensitive(game_dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(game_dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        {
            return Some(entry.path());
        }
    }
    None
}

/// Validates and joins a bare file name.
pub(crate) fn safe_join(game_dir: &Path, field: &str, name: &str) -> Result<PathBuf, ServiceError> {
    ensure_bare_file_name(field, name)?;
    Ok(game_dir.join(name))
}

pub(crate) fn ensure_bare_file_name(field: &str, name: &str) -> Result<(), ServiceError> {
    if !crate::paths::is_safe_file_name(name) {
        return Err(errors::invalid(format!(
            "unsafe {field} `{name}`: must be a bare file name"
        )));
    }
    Ok(())
}

/// Maximum path components for nested payload files (e.g. Luma shader trees).
/// Zip-slip safety still comes from bare-component checks; this only bounds depth.
const MAX_RELATIVE_PATH_DEPTH: usize = 16;

pub(crate) fn ensure_safe_relative_path(field: &str, raw: &str) -> Result<PathBuf, ServiceError> {
    if raw.is_empty() {
        return Err(errors::invalid(format!(
            "unsafe {field} `{raw}`: must not be empty"
        )));
    }
    if raw.starts_with('/') || raw.starts_with('\\') || raw.contains(':') {
        return Err(errors::invalid(format!(
            "unsafe {field} `{raw}`: must be a relative path"
        )));
    }
    let components: Vec<&str> = raw.split(['/', '\\']).collect();
    if components.len() > MAX_RELATIVE_PATH_DEPTH {
        return Err(errors::invalid(format!(
            "unsafe {field} `{raw}`: exceeds the maximum path depth ({MAX_RELATIVE_PATH_DEPTH})"
        )));
    }
    let mut relative = PathBuf::new();
    for c in components {
        ensure_bare_file_name(field, c)?;
        relative.push(c);
    }
    Ok(relative)
}

/// Best-effort remove file.
pub(crate) fn remove_file_if_exists(path: &Path) -> Result<(), ServiceError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(errors::io("remove", path, &error)),
    }
}

pub(crate) fn insert_parent(dirs: &mut std::collections::HashSet<PathBuf>, path: &Path) {
    if let Some(p) = path.parent() {
        dirs.insert(p.to_path_buf());
    }
}

pub(crate) fn remove_dir_if_empty(dir: &Path) -> io::Result<()> {
    match fs::remove_dir(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) if is_directory_not_empty(&e) => Ok(()),
        Err(e) => Err(e),
    }
}

fn is_directory_not_empty(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::DirectoryNotEmpty || error.raw_os_error() == Some(145)
}
