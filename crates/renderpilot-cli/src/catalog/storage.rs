use std::{
    env, fs,
    path::{Path, PathBuf},
};

use renderpilot_storage_sqlite::SqliteStorage;

use crate::error::CliError;

pub(crate) const CATALOG_DB_PATH_ENV: &str = "RENDERPILOT_DB_PATH";

pub(super) fn open_catalog_storage() -> Result<SqliteStorage, CliError> {
    let path = catalog_db_path();

    ensure_catalog_directory(&path)?;

    SqliteStorage::open(&path).map_err(Into::into)
}

fn ensure_catalog_directory(path: &Path) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::CommandFailed(format!(
                "failed to create catalog directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    Ok(())
}

fn catalog_db_path() -> PathBuf {
    if let Some(override_path) = env::var_os(CATALOG_DB_PATH_ENV) {
        return PathBuf::from(override_path);
    }

    if let Some(base_dir) = env::var_os("LOCALAPPDATA").or_else(|| env::var_os("APPDATA")) {
        return PathBuf::from(base_dir)
            .join("RenderPilot")
            .join("catalog.db");
    }

    PathBuf::from("catalog.db")
}
