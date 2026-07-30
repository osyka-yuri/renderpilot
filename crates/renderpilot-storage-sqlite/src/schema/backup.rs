//! Pre-change backups for file-backed catalog databases.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::{storage_context, storage_error};

/// Creates a sibling pre-rebuild backup when `main` is file-backed.
pub(super) fn backup_before_rebuild(connection: &Connection) -> AppResult<Option<PathBuf>> {
    backup_before_schema_change(connection, "pre-rebuild", "catalog schema rebuild")
}

/// Creates a sibling backup before a non-additive schema migration.
pub(super) fn backup_before_migration(
    connection: &Connection,
    target_version: i32,
) -> AppResult<Option<PathBuf>> {
    backup_before_schema_change(
        connection,
        &format!("pre-migration-v{target_version}"),
        &format!("catalog schema migration to v{target_version}"),
    )
}

/// Uses SQLite's online backup API so committed WAL state is included in one
/// consistent destination database. In-memory databases are skipped.
fn backup_before_schema_change(
    connection: &Connection,
    qualifier: &str,
    operation: &str,
) -> AppResult<Option<PathBuf>> {
    let Some(db_path) = main_database_file_path(connection)? else {
        return Ok(None);
    };

    let backup_path = backup_path_for(&db_path, qualifier);
    connection
        .backup(rusqlite::MAIN_DB, &backup_path, None)
        .map_err(|error| {
            storage_context(
                &format!(
                    "could not create catalog backup before {operation} at {}",
                    backup_path.display()
                ),
                error,
            )
        })?;
    validate_backup(&backup_path, operation)?;

    log::warn!(
        "{operation}: backed up database to {}",
        backup_path.display()
    );

    Ok(Some(backup_path))
}

fn validate_backup(path: &Path, operation: &str) -> AppResult<()> {
    let backup = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| {
            storage_context(
                &format!(
                    "could not open catalog backup before {operation} at {}",
                    path.display()
                ),
                error,
            )
        })?;
    let mut statement = backup.prepare("PRAGMA integrity_check").map_err(|error| {
        storage_context(
            &format!(
                "could not prepare integrity validation for catalog backup before {operation} at {}",
                path.display()
            ),
            error,
        )
    })?;
    let results = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| {
            storage_context(
                &format!(
                    "could not validate catalog backup before {operation} at {}",
                    path.display()
                ),
                error,
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            storage_context(
                &format!(
                    "could not read integrity results for catalog backup before {operation} at {}",
                    path.display()
                ),
                error,
            )
        })?;
    if results == ["ok"] {
        Ok(())
    } else {
        Err(storage_error(format!(
            "catalog backup before {operation} failed integrity validation at {}: {}",
            path.display(),
            results.join("; ")
        )))
    }
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

fn backup_path_for(db_path: &Path, qualifier: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let file_name = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("catalog.db");
    let backup_name = format!("{file_name}.{qualifier}.{millis}.bak");
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
        let path = backup_path_for(Path::new("C:/data/catalog.db"), "pre-rebuild");
        let name = path.file_name().and_then(|n| n.to_str()).unwrap();
        assert!(name.starts_with("catalog.db.pre-rebuild."));
        assert!(name.ends_with(".bak"));
        assert_eq!(path.parent().and_then(|p| p.to_str()), Some("C:/data"));
    }
}
