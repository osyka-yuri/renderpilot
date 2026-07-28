//! Pre-rebuild backup for file-backed catalog databases.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

/// Creates a sibling `.pre-rebuild.<unix_ms>.bak` copy when `main` is file-backed.
///
/// Uses WAL checkpoint then filesystem copy so the backup is consistent.
/// In-memory databases are skipped.
pub(super) fn backup_before_rebuild(connection: &Connection) -> AppResult<Option<PathBuf>> {
    let Some(db_path) = main_database_file_path(connection)? else {
        return Ok(None);
    };

    checkpoint_wal(connection)?;

    let backup_path = backup_path_for(&db_path);
    std::fs::copy(&db_path, &backup_path).map_err(|error| {
        storage_context(
            &format!(
                "could not create pre-rebuild catalog backup at {}",
                backup_path.display()
            ),
            error,
        )
    })?;

    log::warn!(
        "catalog schema rebuild: backed up database to {}",
        backup_path.display()
    );

    Ok(Some(backup_path))
}

pub(crate) fn main_database_file_path(connection: &Connection) -> AppResult<Option<PathBuf>> {
    let mut statement = connection
        .prepare("PRAGMA database_list")
        .map_err(|error| storage_context("could not prepare database_list", error))?;

    let mut rows = statement
        .query([])
        .map_err(|error| storage_context("could not query database_list", error))?;

    while let Some(row) = rows
        .next()
        .map_err(|error| storage_context("could not read database_list row", error))?
    {
        let name: String = row
            .get(1)
            .map_err(|error| storage_context("could not read database name", error))?;
        if name != "main" {
            continue;
        }
        let file: Option<String> = row
            .get(2)
            .map_err(|error| storage_context("could not read database file path", error))?;
        return Ok(file
            .filter(|path| !path.is_empty() && path != ":memory:")
            .map(PathBuf::from));
    }

    Ok(None)
}

pub(crate) fn checkpoint_wal(connection: &Connection) -> AppResult<()> {
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| storage_context("could not checkpoint sqlite WAL before backup", error))
}

fn backup_path_for(db_path: &Path) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let file_name = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("catalog.db");
    let backup_name = format!("{file_name}.pre-rebuild.{millis}.bak");
    match db_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(backup_name),
        _ => PathBuf::from(backup_name),
    }
}

#[cfg(test)]
mod tests {
    use super::backup_path_for;
    use std::path::Path;

    #[test]
    fn backup_path_is_sibling_with_suffix() {
        let path = backup_path_for(Path::new("C:/data/catalog.db"));
        let name = path.file_name().and_then(|n| n.to_str()).unwrap();
        assert!(name.starts_with("catalog.db.pre-rebuild."));
        assert!(name.ends_with(".bak"));
        assert_eq!(path.parent().and_then(|p| p.to_str()), Some("C:/data"));
    }
}
