//! Integration tests for SQLite `file_hash_cache` persistence during `scan-folder`.

use std::{fs, path::Path};

use renderpilot_orchestration::Context;
use renderpilot_storage_sqlite::SqliteStorage;

use crate::{
    catalog,
    commands::test_support::{
        open_storage, path_string, temp_db_path, CatalogEnvironmentGuard, TempGameFolder,
    },
};

use super::scan::{create_dlss_file, scan_catalog_folder};

const DLSS_DLL_FILE_NAME: &str = "nvngx_dlss.dll";

/// SHA-256 of `b"hello"` (verified against `sha256sum`).
const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

#[test]
fn first_scan_populates_sqlite_file_hash_cache() {
    let db_path = temp_db_path("scan-cache-first");
    let _catalog = CatalogEnvironmentGuard::new(&db_path);
    let context = Context::open_at(&db_path).expect("catalog sqlite should open");
    let storage = open_storage(&db_path);
    let folder = TempGameFolder::new("scan-cache-first");

    create_dlss_file(folder.path(), b"hello");

    scan_catalog_folder(
        &context,
        folder.path(),
        "first scan should populate file_hash_cache",
    );

    let dll_norm = normalized_dll_path(folder.path());
    let rows = storage
        .load_file_hash_cache(&path_string(folder.path()))
        .expect("load file_hash_cache");

    let row = rows
        .iter()
        .find(|row| row.path == dll_norm)
        .expect("expected a cache row for the detected DLL path");

    assert_eq!(row.sha256.as_str(), HELLO_SHA256);
    assert_eq!(row.size, 5);
}

#[test]
fn rescan_unchanged_file_keeps_consistent_file_hash_cache_entry() {
    let db_path = temp_db_path("scan-cache-rescan");
    let _catalog = CatalogEnvironmentGuard::new(&db_path);
    let context = Context::open_at(&db_path).expect("catalog sqlite should open");
    let storage = open_storage(&db_path);
    let folder = TempGameFolder::new("scan-cache-rescan");

    create_dlss_file(folder.path(), b"hello");

    scan_catalog_folder(&context, folder.path(), "first scan");
    let sha_once = cache_sha_for_dll(&storage, folder.path());

    scan_catalog_folder(&context, folder.path(), "second scan");
    let sha_twice = cache_sha_for_dll(&storage, folder.path());

    assert_eq!(sha_once, sha_twice);
}

#[test]
fn scan_updates_sqlite_file_hash_cache_after_file_change() {
    let db_path = temp_db_path("scan-cache-stale");
    let _catalog = CatalogEnvironmentGuard::new(&db_path);
    let context = Context::open_at(&db_path).expect("catalog sqlite should open");
    let storage = open_storage(&db_path);
    let folder = TempGameFolder::new("scan-cache-stale");

    create_dlss_file(folder.path(), b"");
    scan_catalog_folder(&context, folder.path(), "first scan");

    let dlss_path = folder.path().join(DLSS_DLL_FILE_NAME);
    fs::write(&dlss_path, b"hello").expect("DLL contents should update");

    scan_catalog_folder(&context, folder.path(), "rescan after edit");

    let sha = cache_sha_for_dll(&storage, folder.path());

    assert_eq!(sha.as_str(), HELLO_SHA256);
}

#[test]
fn failed_scan_does_not_overwrite_existing_file_hash_cache_rows() {
    let db_path = temp_db_path("scan-cache-fail");
    let _catalog = CatalogEnvironmentGuard::new(&db_path);
    let folder = TempGameFolder::new("scan-cache-fail");

    create_dlss_file(folder.path(), b"keep");

    let scope = path_string(folder.path());
    let dll_norm = normalized_dll_path(folder.path());

    let context = Context::open_at(&db_path).expect("catalog sqlite should open");
    let storage = open_storage(&db_path);
    scan_catalog_folder(&context, folder.path(), "first scan");

    let sha_before = storage
        .load_file_hash_cache(&scope)
        .expect("load cache")
        .into_iter()
        .find(|row| row.path == dll_norm)
        .expect("cache row")
        .sha256;

    fs::remove_dir_all(folder.path()).expect("remove scanned folder");

    let error = catalog::scan_folder(&context, folder.path().to_path_buf());
    assert!(
        error.is_err(),
        "scan should fail when the game folder no longer exists",
    );

    let sha_after = storage
        .load_file_hash_cache(&scope)
        .expect("load cache after failed scan")
        .into_iter()
        .find(|row| row.path == dll_norm)
        .expect("cache row should remain")
        .sha256;

    assert_eq!(sha_before, sha_after);
}

fn normalized_dll_path(folder: &Path) -> String {
    path_string(&folder.join(DLSS_DLL_FILE_NAME))
}

fn cache_sha_for_dll(
    storage: &SqliteStorage,
    folder: &Path,
) -> renderpilot_orchestration::domain::Sha256Hash {
    let dll_norm = normalized_dll_path(folder);
    storage
        .load_file_hash_cache(&path_string(folder))
        .expect("load file_hash_cache")
        .into_iter()
        .find(|row| row.path == dll_norm)
        .expect("cache row for DLL")
        .sha256
}
