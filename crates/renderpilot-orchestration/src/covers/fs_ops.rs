//! Cover directory orphan GC and best-effort unlink.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_storage_sqlite::SqliteStorage;

use super::basename::cover_basename_is_safe;
use super::paths::covers_directory;
use crate::ServiceError;

const PARTIAL_COVER_SUFFIX: &str = ".part";

/// Removes a cover file from disk by name, best-effort (ignores errors).
pub fn unlink_cover_file_best_effort(catalog_db_path: &Path, file_name: Option<&str>) {
    let Some(file_name) = file_name else {
        return;
    };

    unlink_cover_basename_best_effort(catalog_db_path, file_name);
}

/// Removes cover files from the covers directory that are not referenced by any catalog row.
pub fn gc_orphan_cover_files(
    catalog_db_path: &Path,
    storage: &SqliteStorage,
) -> Result<(), ServiceError> {
    let covers_dir = covers_directory(catalog_db_path);

    if !covers_dir.is_dir() {
        return Ok(());
    }

    let referenced = referenced_cover_file_names(storage)?;
    let entries = read_covers_dir(&covers_dir)?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            ServiceError::CoverIo(format!("could not read covers directory entry: {error}"))
        })?;

        let Some(file_name) = cover_file_name(&entry) else {
            continue;
        };

        if !is_orphan_gc_candidate(&file_name, &referenced) {
            continue;
        }

        remove_file_best_effort(entry.path());
    }

    Ok(())
}

fn referenced_cover_file_names(storage: &SqliteStorage) -> Result<HashSet<String>, ServiceError> {
    Ok(storage
        .list_game_cover_file_names()
        .map_err(ServiceError::from)?
        .into_iter()
        .collect())
}

fn read_covers_dir(covers_dir: &Path) -> Result<fs::ReadDir, ServiceError> {
    fs::read_dir(covers_dir)
        .map_err(|error| ServiceError::CoverIo(format!("could not read covers directory: {error}")))
}

fn cover_file_name(entry: &fs::DirEntry) -> Option<String> {
    entry.file_name().into_string().ok()
}

fn is_orphan_gc_candidate(file_name: &str, referenced: &HashSet<String>) -> bool {
    is_gc_visible_cover_name(file_name)
        && cover_basename_is_safe(file_name)
        && !referenced.contains(file_name)
}

fn is_gc_visible_cover_name(file_name: &str) -> bool {
    !file_name.starts_with('.') && !file_name.ends_with(PARTIAL_COVER_SUFFIX)
}

fn unlink_cover_basename_best_effort(catalog_db_path: &Path, file_name: &str) {
    if !cover_basename_is_safe(file_name) {
        return;
    }

    remove_file_best_effort(covers_directory(catalog_db_path).join(file_name));
}

fn remove_file_best_effort(path: PathBuf) {
    let _ = fs::remove_file(path);
}
