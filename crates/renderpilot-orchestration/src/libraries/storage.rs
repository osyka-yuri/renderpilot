//! Local filesystem helpers for library archives and manifests.

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::ServiceError;

use super::library_error;

const APP_DIR_NAME: &str = "RenderPilot";
const LIBRARIES_DIR_NAME: &str = "libraries";
const MANIFEST_FILE_NAME: &str = "libraries_manifest.json";

pub(super) fn app_dir() -> Result<PathBuf, ServiceError> {
    resolve_app_dir(|name| std::env::var_os(name))
}

/// Resolves the application data directory using the supplied environment
/// variable lookup.  Checks in order:
/// 1. `RENDERPILOT_APP_DIR` — portable-mode override
/// 2. `LOCALAPPDATA\RenderPilot`
/// 3. `APPDATA\RenderPilot`
fn resolve_app_dir(
    mut get_env: impl FnMut(&str) -> Option<OsString>,
) -> Result<PathBuf, ServiceError> {
    if let Some(value) = get_env(crate::portable::APP_DIR_ENV) {
        if !value.as_os_str().is_empty() {
            return Ok(PathBuf::from(value));
        }
    }

    for candidate in ["LOCALAPPDATA", "APPDATA"] {
        let Some(value) = get_env(candidate) else {
            continue;
        };
        if value.as_os_str().is_empty() {
            continue;
        }
        return Ok(PathBuf::from(value).join(APP_DIR_NAME));
    }

    Err(library_error("could not find app data directory"))
}

pub(super) fn libraries_storage_dir() -> Result<PathBuf, ServiceError> {
    Ok(app_dir()?.join(LIBRARIES_DIR_NAME))
}

pub(super) fn local_manifest_path() -> Result<PathBuf, ServiceError> {
    Ok(app_dir()?.join(MANIFEST_FILE_NAME))
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
        .join(sanitize_path_component(entry_id))
        .join(file_name))
}

pub(super) fn read_file(path: &Path) -> Result<Vec<u8>, ServiceError> {
    fs::read(path).map_err(|error| {
        library_error(format!("failed to read file `{}`: {error}", path.display()))
    })
}

pub(super) fn remove_file_if_exists(path: &Path) -> Result<(), ServiceError> {
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
pub(super) fn write_sha256_cache(path: &Path, sha256: &str) -> Result<(), ServiceError> {
    let cache_path = path.with_extension(format!(
        "{}.sha256",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    write_file_atomically(&cache_path, sha256.as_bytes())
}

/// Reads a sidecar `.sha256` file next to the given file, if it exists.
pub(super) fn read_sha256_cache(path: &Path) -> Result<Option<String>, ServiceError> {
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

pub(super) fn write_file_atomically(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
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

fn write_temp_file(temp_path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
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

fn replace_with_temp_file(temp_path: &Path, destination_path: &Path) -> Result<(), ServiceError> {
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, ffi::OsString, path::PathBuf};

    use crate::ServiceError;

    use super::{resolve_app_dir, APP_DIR_NAME};

    fn env_map(entries: &[(&str, &str)]) -> impl FnMut(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn resolved_dir(entries: &[(&str, &str)]) -> Result<PathBuf, ServiceError> {
        resolve_app_dir(env_map(entries))
    }

    #[test]
    fn uses_portable_app_dir_when_set() {
        let dir = resolved_dir(&[(crate::portable::APP_DIR_ENV, "D:\\portable")])
            .expect("app dir should resolve");
        assert_eq!(dir, PathBuf::from("D:\\portable"));
    }

    #[test]
    fn portable_app_dir_takes_precedence_over_local_app_data() {
        let dir = resolved_dir(&[
            (crate::portable::APP_DIR_ENV, "D:\\portable"),
            ("LOCALAPPDATA", "C:\\Users\\foo\\AppData\\Local"),
        ])
        .expect("app dir should resolve");
        assert_eq!(dir, PathBuf::from("D:\\portable"));
    }

    #[test]
    fn ignores_empty_portable_app_dir_falls_back_to_local_app_data() {
        let dir = resolved_dir(&[
            (crate::portable::APP_DIR_ENV, ""),
            ("LOCALAPPDATA", "C:\\local"),
        ])
        .expect("app dir should resolve");
        assert_eq!(dir, PathBuf::from("C:\\local").join(APP_DIR_NAME));
    }

    #[test]
    fn uses_local_app_data_before_app_data() {
        let dir = resolved_dir(&[("LOCALAPPDATA", "C:\\local"), ("APPDATA", "C:\\roaming")])
            .expect("app dir should resolve");
        assert_eq!(dir, PathBuf::from("C:\\local").join(APP_DIR_NAME));
    }

    #[test]
    fn falls_back_to_app_data_when_local_app_data_missing() {
        let dir = resolved_dir(&[("APPDATA", "C:\\roaming")]).expect("app dir should resolve");
        assert_eq!(dir, PathBuf::from("C:\\roaming").join(APP_DIR_NAME));
    }

    #[test]
    fn errors_when_no_base_dir_available() {
        assert!(resolved_dir(&[]).is_err());
    }
}
