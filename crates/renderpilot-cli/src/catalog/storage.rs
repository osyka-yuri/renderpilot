use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Component, Path, PathBuf},
};

use renderpilot_storage_sqlite::SqliteStorage;

use crate::error::CliError;

pub(crate) const CATALOG_DB_PATH_ENV: &str = "RENDERPILOT_DB_PATH";

const APP_DIR_NAME: &str = "RenderPilot";
const CATALOG_DB_FILE_NAME: &str = "catalog.db";
const BASE_DIR_ENV_CANDIDATES: [&str; 2] = ["LOCALAPPDATA", "APPDATA"];

pub(crate) fn open_catalog_storage() -> Result<SqliteStorage, CliError> {
    let path = catalog_db_path()?;

    validate_catalog_db_path(&path)?;
    ensure_catalog_directory(&path)?;

    SqliteStorage::open(&path).map_err(|error| {
        CliError::CommandFailed(format!(
            "failed to open catalog database `{}`: {error}",
            path.display()
        ))
    })
}

fn catalog_db_path() -> Result<PathBuf, CliError> {
    catalog_db_path_from_env(|name| env::var_os(name))
}

fn catalog_db_path_from_env(
    mut get_env: impl FnMut(&str) -> Option<OsString>,
) -> Result<PathBuf, CliError> {
    if let Some(value) = get_env(CATALOG_DB_PATH_ENV) {
        if value.as_os_str().is_empty() {
            return Err(CliError::CommandFailed(format!(
                "{CATALOG_DB_PATH_ENV} is set but empty"
            )));
        }

        return Ok(PathBuf::from(value));
    }

    for env_name in BASE_DIR_ENV_CANDIDATES {
        let Some(value) = get_env(env_name) else {
            continue;
        };

        if value.as_os_str().is_empty() {
            continue;
        }

        return Ok(PathBuf::from(value)
            .join(APP_DIR_NAME)
            .join(CATALOG_DB_FILE_NAME));
    }

    Ok(PathBuf::from(CATALOG_DB_FILE_NAME))
}

fn validate_catalog_db_path(path: &Path) -> Result<(), CliError> {
    if path.as_os_str().is_empty() {
        return Err(CliError::CommandFailed(
            "catalog database path is empty".to_owned(),
        ));
    }

    if !matches!(path.components().next_back(), Some(Component::Normal(_))) {
        return Err(CliError::CommandFailed(format!(
            "catalog database path must include a file name: `{}`",
            path.display()
        )));
    }

    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Err(CliError::CommandFailed(format!(
            "catalog database path points to a directory: `{}`",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CliError::CommandFailed(format!(
            "failed to inspect catalog database path `{}`: {error}",
            path.display()
        ))),
    }
}

fn ensure_catalog_directory(path: &Path) -> Result<(), CliError> {
    let Some(parent) = non_empty_parent(path) else {
        return Ok(());
    };

    fs::create_dir_all(parent).map_err(|error| {
        CliError::CommandFailed(format!(
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

    fn env_map(entries: &[(&str, &str)]) -> impl FnMut(&str) -> Option<OsString> {
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

    #[test]
    fn uses_local_app_data_before_app_data() {
        let path = resolved_path(&[("LOCALAPPDATA", "local-data"), ("APPDATA", "roaming-data")]);

        assert_eq!(
            path,
            PathBuf::from("local-data")
                .join(APP_DIR_NAME)
                .join(CATALOG_DB_FILE_NAME)
        );
    }

    #[test]
    fn falls_back_to_app_data_when_local_app_data_is_missing() {
        let path = resolved_path(&[("APPDATA", "roaming-data")]);

        assert_eq!(
            path,
            PathBuf::from("roaming-data")
                .join(APP_DIR_NAME)
                .join(CATALOG_DB_FILE_NAME)
        );
    }

    #[test]
    fn ignores_empty_base_dir_env_values() {
        let path = resolved_path(&[("LOCALAPPDATA", ""), ("APPDATA", "")]);

        assert_eq!(path, PathBuf::from(CATALOG_DB_FILE_NAME));
    }

    #[test]
    fn falls_back_to_relative_catalog_db() {
        let path = resolved_path(&[]);

        assert_eq!(path, PathBuf::from(CATALOG_DB_FILE_NAME));
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

        assert!(db_path
            .parent()
            .expect("db path should have parent")
            .is_dir());

        fs::remove_dir_all(&dir).expect("test directory should be removed");
    }

    #[test]
    fn ensure_catalog_directory_is_noop_for_plain_relative_file_name() {
        ensure_catalog_directory(Path::new(CATALOG_DB_FILE_NAME))
            .expect("plain relative file should not require directory creation");
    }
}
