//! The single resolver for RenderPilot's application data directory.
//!
//! One source of truth for where the app keeps persistent data — the catalog
//! database, downloaded library archives, and cached manifests all hang off it.
//! Resolution order: `RENDERPILOT_APP_DIR` (the portable-mode override set by the
//! launcher) → `LOCALAPPDATA\RenderPilot` → `APPDATA\RenderPilot`.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::ServiceError;

const APP_DIR_NAME: &str = "RenderPilot";

/// The application data directory (honouring portable mode).
pub(crate) fn app_dir() -> Result<PathBuf, ServiceError> {
    resolve_app_dir(|name| std::env::var_os(name))
}

/// Resolves the app data directory using the supplied environment-variable lookup,
/// so the order/precedence can be unit-tested without touching the process env.
pub(crate) fn resolve_app_dir(
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

    Err(ServiceError::CommandFailed(
        "could not find app data directory".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env_map(entries: &[(&str, &str)]) -> impl FnMut(&str) -> Option<OsString> + use<> {
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
