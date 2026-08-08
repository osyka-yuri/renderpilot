//! The single resolver for RenderPilot's application data directory.
//!
//! One source of truth for where the app keeps persistent data — the catalog
//! database, downloaded library archives, and cached manifests all hang off it.
//!
//! Resolution order:
//! 1. authenticated portable [`crate::portable::RuntimePathsV1`]
//! 2. `RENDERPILOT_APP_DIR` compatibility override for ordinary launches
//! 3. Windows: `%LOCALAPPDATA%\RenderPilot`, then `%APPDATA%\RenderPilot`
//! 4. Unix: `$XDG_DATA_HOME/RenderPilot`, then `$HOME/.local/share/RenderPilot`

use std::ffi::OsString;
use std::path::PathBuf;

use crate::ServiceError;

const APP_DIR_NAME: &str = "RenderPilot";

/// The application data directory (honouring portable mode).
pub(crate) fn app_dir() -> Result<PathBuf, ServiceError> {
    if let Some(path) = crate::portable::portable_data_root() {
        return Ok(path.to_owned());
    }
    resolve_app_dir(|name| std::env::var_os(name))
}

/// Resolves the app data directory using the supplied environment-variable lookup,
/// so the order/precedence can be unit-tested without touching the process env.
pub(crate) fn resolve_app_dir(
    mut get_env: impl FnMut(&str) -> Option<OsString>,
) -> Result<PathBuf, ServiceError> {
    if let Some(value) = get_env(crate::portable::APP_DIR_ENV)
        && !value.as_os_str().is_empty()
    {
        return Ok(PathBuf::from(value));
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

    // Linux CI / WSL / portable CLI without Windows AppData vars.
    if let Some(value) = get_env("XDG_DATA_HOME")
        && !value.as_os_str().is_empty()
    {
        return Ok(PathBuf::from(value).join(APP_DIR_NAME));
    }
    if let Some(value) = get_env("HOME")
        && !value.as_os_str().is_empty()
    {
        return Ok(PathBuf::from(value)
            .join(".local")
            .join("share")
            .join(APP_DIR_NAME));
    }

    Err(ServiceError::command_failed(
        "could not find app data directory",
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
    fn uses_xdg_data_home_when_windows_app_data_missing() {
        let dir = resolved_dir(&[("XDG_DATA_HOME", "/var/data")]).expect("app dir should resolve");
        assert_eq!(dir, PathBuf::from("/var/data").join(APP_DIR_NAME));
    }

    #[test]
    fn falls_back_to_home_local_share_when_xdg_missing() {
        let dir = resolved_dir(&[("HOME", "/home/user")]).expect("app dir should resolve");
        assert_eq!(
            dir,
            PathBuf::from("/home/user")
                .join(".local")
                .join("share")
                .join(APP_DIR_NAME)
        );
    }

    #[test]
    fn xdg_takes_precedence_over_home() {
        let dir = resolved_dir(&[("XDG_DATA_HOME", "/var/data"), ("HOME", "/home/user")])
            .expect("app dir should resolve");
        assert_eq!(dir, PathBuf::from("/var/data").join(APP_DIR_NAME));
    }

    #[test]
    fn windows_app_data_takes_precedence_over_unix_fallbacks() {
        let dir = resolved_dir(&[
            ("LOCALAPPDATA", "C:\\local"),
            ("XDG_DATA_HOME", "/var/data"),
            ("HOME", "/home/user"),
        ])
        .expect("app dir should resolve");
        assert_eq!(dir, PathBuf::from("C:\\local").join(APP_DIR_NAME));
    }

    #[test]
    fn errors_when_no_base_dir_available() {
        assert!(resolved_dir(&[]).is_err());
    }
}
