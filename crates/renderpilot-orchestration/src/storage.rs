//! Catalog database path resolution and SQLite connection management.

use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Component, Path, PathBuf},
};

pub use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

/// Environment variable that overrides the SQLite catalog database path.
pub const CATALOG_DB_PATH_ENV: &str = "RENDERPILOT_DB_PATH";

const CATALOG_DB_FILE_NAME: &str = "catalog.db";

/// Opens the SQLite catalog database, creating the directory if needed.
pub fn open_catalog_storage() -> Result<SqliteStorage, ServiceError> {
    let path = catalog_db_path()?;

    validate_catalog_db_path(&path)?;
    ensure_catalog_directory(&path)?;

    SqliteStorage::open(&path).map_err(|error| {
        ServiceError::command_failed(format!(
            "failed to open catalog database `{}`: {error}",
            path.display()
        ))
    })
}

/// Resolved absolute or relative path to the SQLite catalog file.
pub fn catalog_database_path() -> Result<PathBuf, ServiceError> {
    catalog_db_path()
}

fn catalog_db_path() -> Result<PathBuf, ServiceError> {
    if let Some(paths) = crate::portable::runtime_paths() {
        return Ok(paths.catalog_db_path.clone());
    }
    catalog_db_path_from_env(|name| env::var_os(name))
}

fn catalog_db_path_from_env(
    mut get_env: impl FnMut(&str) -> Option<OsString>,
) -> Result<PathBuf, ServiceError> {
    // The catalog file has one resolver-specific knob: an explicit absolute path
    // override. Everything else is the shared app data dir, with one catalog-only
    // twist — a missing base directory degrades to a relative file rather than the
    // hard error `resolve_app_dir` returns.
    if let Some(value) = get_env(CATALOG_DB_PATH_ENV) {
        if value.as_os_str().is_empty() {
            return Err(ServiceError::command_failed(format!(
                "{CATALOG_DB_PATH_ENV} is set but empty"
            )));
        }

        return Ok(PathBuf::from(value));
    }

    match crate::app_dir::resolve_app_dir(get_env) {
        Ok(dir) => Ok(dir.join(CATALOG_DB_FILE_NAME)),
        Err(_) => Ok(PathBuf::from(CATALOG_DB_FILE_NAME)),
    }
}

fn validate_catalog_db_path(path: &Path) -> Result<(), ServiceError> {
    if path.as_os_str().is_empty() {
        return Err(ServiceError::command_failed(
            "catalog database path is empty",
        ));
    }

    if !matches!(path.components().next_back(), Some(Component::Normal(_))) {
        return Err(ServiceError::command_failed(format!(
            "catalog database path must include a file name: `{}`",
            path.display()
        )));
    }

    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Err(ServiceError::command_failed(format!(
            "catalog database path points to a directory: `{}`",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ServiceError::command_failed(format!(
            "failed to inspect catalog database path `{}`: {error}",
            path.display()
        ))),
    }
}

fn ensure_catalog_directory(path: &Path) -> Result<(), ServiceError> {
    let Some(parent) = non_empty_parent(path) else {
        return Ok(());
    };

    fs::create_dir_all(parent).map_err(|error| {
        ServiceError::command_failed(format!(
            "failed to create catalog directory `{}` for database `{}`: {error}",
            parent.display(),
            path.display()
        ))
    })
}

fn non_empty_parent(path: &Path) -> Option<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        collections::HashMap,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn env_map(entries: &[(&str, &str)]) -> impl FnMut(&str) -> Option<OsString> + use<> {
        let entries: HashMap<String, OsString> = entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), OsString::from(value)))
            .collect();

        move |key| entries.get(key).cloned()
    }

    fn resolved_path(entries: &[(&str, &str)]) -> PathBuf {
        catalog_db_path_from_env(env_map(entries)).expect("catalog db path should resolve")
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();

        env::temp_dir().join(format!("renderpilot-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn uses_explicit_catalog_path_override() {
        let path = resolved_path(&[(CATALOG_DB_PATH_ENV, "custom.db")]);

        assert_eq!(path, PathBuf::from("custom.db"));
    }

    #[test]
    fn rejects_empty_explicit_catalog_path_override() {
        let result = catalog_db_path_from_env(env_map(&[(CATALOG_DB_PATH_ENV, "")]));

        assert!(result.is_err());
    }

    /// Without an explicit override the catalog file is the shared app data dir
    /// (whose precedence is owned and tested by [`crate::app_dir`]) joined with the
    /// catalog file name. Deriving the expected path from `resolve_app_dir` ties the
    /// two together by construction instead of re-asserting the precedence here.
    #[test]
    fn places_the_catalog_file_under_the_resolved_app_dir() {
        for entries in [
            [(crate::portable::APP_DIR_ENV, "D:\\portable")].as_slice(),
            [("LOCALAPPDATA", "C:\\local"), ("APPDATA", "C:\\roaming")].as_slice(),
        ] {
            let expected = crate::app_dir::resolve_app_dir(env_map(entries))
                .expect("app dir resolves")
                .join(CATALOG_DB_FILE_NAME);

            assert_eq!(resolved_path(entries), expected);
        }
    }

    #[test]
    fn falls_back_to_relative_catalog_db_when_no_base_dir_is_available() {
        // The catalog-specific degradation: where `resolve_app_dir` hard-errors, the
        // catalog drops to a relative file so the app can still open a database.
        assert_eq!(resolved_path(&[]), PathBuf::from(CATALOG_DB_FILE_NAME));
        assert_eq!(
            resolved_path(&[("LOCALAPPDATA", ""), ("APPDATA", "")]),
            PathBuf::from(CATALOG_DB_FILE_NAME)
        );
    }

    #[test]
    fn skips_empty_parent_for_relative_file_name() {
        let path = Path::new(CATALOG_DB_FILE_NAME);

        assert!(non_empty_parent(path).is_none());
    }

    #[test]
    fn returns_parent_for_nested_relative_path() {
        let path = Path::new("data").join(CATALOG_DB_FILE_NAME);

        assert_eq!(non_empty_parent(&path), Some(Path::new("data")));
    }

    #[test]
    fn validate_accepts_missing_regular_file_path() {
        let path = unique_temp_path("missing-db").join(CATALOG_DB_FILE_NAME);

        assert!(validate_catalog_db_path(&path).is_ok());
    }

    #[test]
    fn validate_rejects_empty_path() {
        assert!(validate_catalog_db_path(Path::new("")).is_err());
    }

    #[test]
    fn validate_rejects_path_without_file_name() {
        assert!(validate_catalog_db_path(Path::new(".")).is_err());
        assert!(validate_catalog_db_path(Path::new("..")).is_err());
    }

    #[test]
    fn validate_rejects_existing_directory() {
        let dir = unique_temp_path("directory-db-path");
        fs::create_dir_all(&dir).expect("test directory should be created");

        let result = validate_catalog_db_path(&dir);

        fs::remove_dir_all(&dir).expect("test directory should be removed");

        assert!(result.is_err());
    }

    #[test]
    fn validate_accepts_existing_file() {
        let dir = unique_temp_path("existing-file-parent");
        let file = dir.join(CATALOG_DB_FILE_NAME);

        fs::create_dir_all(&dir).expect("test directory should be created");
        fs::write(&file, b"").expect("test file should be created");

        let result = validate_catalog_db_path(&file);

        fs::remove_dir_all(&dir).expect("test directory should be removed");

        assert!(result.is_ok());
    }

    #[test]
    fn ensure_catalog_directory_creates_missing_parent_directories() {
        let dir = unique_temp_path("catalog-parent");
        let db_path = dir.join("nested").join(CATALOG_DB_FILE_NAME);

        ensure_catalog_directory(&db_path).expect("catalog directory should be created");

        assert!(
            db_path
                .parent()
                .expect("db path should have parent")
                .is_dir()
        );

        fs::remove_dir_all(&dir).expect("test directory should be removed");
    }

    #[test]
    fn ensure_catalog_directory_is_noop_for_plain_relative_file_name() {
        ensure_catalog_directory(Path::new(CATALOG_DB_FILE_NAME))
            .expect("plain relative file should not require directory creation");
    }
}
