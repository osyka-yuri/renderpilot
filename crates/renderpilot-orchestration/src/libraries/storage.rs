//! Library-specific on-disk layout for archives, DLLs, and manifests, built on the
//! shared [`crate::fs`] (atomic write, safe names) and [`crate::app_dir`] (app data
//! root) primitives.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::ServiceError;

use super::library_error;

const LIBRARIES_DIR_NAME: &str = "libraries";

pub(super) fn libraries_storage_dir() -> Result<PathBuf, ServiceError> {
    Ok(crate::app_dir::app_dir()?.join(LIBRARIES_DIR_NAME))
}

/// Returns the path for a locally stored preset manifest file.
pub fn local_preset_manifest_path(file_name: &str) -> Result<PathBuf, ServiceError> {
    Ok(libraries_storage_dir()?.join(file_name))
}

pub(super) fn local_archive_path(
    group_key: &str,
    archive_file_name: &str,
) -> Result<PathBuf, ServiceError> {
    Ok(libraries_storage_dir()?
        .join(group_key)
        .join(archive_file_name))
}

pub(super) fn local_dll_path(
    group_key: &str,
    entry_id: &str,
    file_name: &str,
) -> Result<PathBuf, ServiceError> {
    Ok(libraries_storage_dir()?
        .join(group_key)
        .join(crate::fs::sanitize_path_component(entry_id))
        .join(file_name))
}

/// Writes a sidecar `.sha256` file next to the given file.
pub(super) fn write_sha256_cache(path: &Path, sha256: &str) -> Result<(), ServiceError> {
    crate::fs::write_file_atomically(&sha256_cache_path(path), sha256.as_bytes())
}

/// Reads a sidecar `.sha256` file next to the given file, if it exists.
pub(super) fn read_sha256_cache(path: &Path) -> Result<Option<String>, ServiceError> {
    let cache_path = sha256_cache_path(path);
    match fs::read_to_string(&cache_path) {
        Ok(content) => Ok(Some(content.trim().to_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(library_error(format!(
            "failed to read sha256 cache `{}`: {error}",
            cache_path.display()
        ))),
    }
}

pub(super) fn sha256_cache_path(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "{}.sha256",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ))
}
