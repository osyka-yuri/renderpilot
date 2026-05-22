//! Local filesystem helpers for library archives and manifests.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::CliError;

use super::library_error;

const APP_DIR_NAME: &str = "RenderPilot";
const LIBRARIES_DIR_NAME: &str = "libraries";
const MANIFEST_FILE_NAME: &str = "libraries_manifest.json";

pub(super) fn app_data_dir() -> Result<PathBuf, CliError> {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .ok_or_else(|| library_error("could not find app data directory"))
}

pub(super) fn app_dir() -> Result<PathBuf, CliError> {
    Ok(app_data_dir()?.join(APP_DIR_NAME))
}

pub(super) fn libraries_storage_dir() -> Result<PathBuf, CliError> {
    Ok(app_dir()?.join(LIBRARIES_DIR_NAME))
}

pub(super) fn local_manifest_path() -> Result<PathBuf, CliError> {
    Ok(app_dir()?.join(MANIFEST_FILE_NAME))
}

pub(super) fn local_archive_path(
    group_key: &str,
    archive_file_name: &str,
) -> Result<PathBuf, CliError> {
    Ok(libraries_storage_dir()?
        .join(group_key)
        .join(archive_file_name))
}

pub(super) fn local_dll_path(
    group_key: &str,
    entry_id: &str,
    file_name: &str,
) -> Result<PathBuf, CliError> {
    Ok(libraries_storage_dir()?
        .join(group_key)
        .join(sanitize_path_component(entry_id))
        .join(file_name))
}

pub(super) fn read_file(path: &Path) -> Result<Vec<u8>, CliError> {
    fs::read(path).map_err(|error| {
        library_error(format!("failed to read file `{}`: {error}", path.display()))
    })
}

pub(super) fn remove_file_if_exists(path: &Path) -> Result<(), CliError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(library_error(format!(
            "failed to delete library file `{}`: {error}",
            path.display()
        ))),
    }
}

/// Writes a sidecar `.sha256` file next to the given file.
pub(super) fn write_sha256_cache(path: &Path, sha256: &str) -> Result<(), CliError> {
    let cache_path = path.with_extension(format!(
        "{}.sha256",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    write_file_atomically(&cache_path, sha256.as_bytes())
}

/// Reads a sidecar `.sha256` file next to the given file, if it exists.
pub(super) fn read_sha256_cache(path: &Path) -> Result<Option<String>, CliError> {
    let cache_path = path.with_extension(format!(
        "{}.sha256",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    match fs::read_to_string(&cache_path) {
        Ok(content) => Ok(Some(content.trim().to_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(library_error(format!(
            "failed to read sha256 cache `{}`: {error}",
            cache_path.display()
        ))),
    }
}

pub(super) fn write_file_atomically(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let parent = path.parent().ok_or_else(|| {
        library_error(format!(
            "cannot write file `{}` because it has no parent directory",
            path.display()
        ))
    })?;

    fs::create_dir_all(parent).map_err(|error| {
        library_error(format!(
            "failed to create directory `{}`: {error}",
            parent.display()
        ))
    })?;

    let temp_path = temporary_file_path(path, "tmp");
    write_temp_file(&temp_path, bytes)?;

    replace_with_temp_file(&temp_path, path)
}

fn write_temp_file(temp_path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .map_err(|error| {
            library_error(format!(
                "failed to create temporary file `{}`: {error}",
                temp_path.display()
            ))
        })?;

    temp_file.write_all(bytes).map_err(|error| {
        let _ = fs::remove_file(temp_path);
        library_error(format!(
            "failed to write temporary file `{}`: {error}",
            temp_path.display()
        ))
    })?;

    temp_file.sync_all().map_err(|error| {
        let _ = fs::remove_file(temp_path);
        library_error(format!(
            "failed to flush temporary file `{}`: {error}",
            temp_path.display()
        ))
    })?;

    Ok(())
}

fn replace_with_temp_file(temp_path: &Path, destination_path: &Path) -> Result<(), CliError> {
    fs::rename(temp_path, destination_path).map_err(|error| {
        let _ = fs::remove_file(temp_path);
        library_error(format!(
            "failed to move temporary file `{}` to `{}`: {error}",
            temp_path.display(),
            destination_path.display()
        ))
    })
}

fn temporary_file_path(path: &Path, marker: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    path.with_file_name(format!(
        "{file_name}.{marker}-{}-{timestamp}",
        std::process::id()
    ))
}

pub(super) fn sanitize_path_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => character,
            _ => '_',
        })
        .collect();

    let sanitized = sanitized
        .trim_matches(|c| c == '.' || c == ' ' || c == '_')
        .to_owned();

    if sanitized.is_empty() {
        return "unknown".to_owned();
    }

    let stem = sanitized.split('.').next().unwrap_or_default();
    if is_windows_reserved_name(stem) {
        format!("_{sanitized}")
    } else {
        sanitized
    }
}

fn is_windows_reserved_name(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}
