//! Integration tests for SQLite `file_hash_cache` persistence during installation scans.

use std::{fs, path::Path};

use renderpilot_storage_sqlite::SqliteStorage;

use crate::commands::test_support::{CatalogFixture, TempGameFolder, path_string};

use super::scan::{create_dlss_file, scan_catalog_folder};

const DLSS_DLL_FILE_NAME: &str = "nvngx_dlss.dll";

/// SHA-256 of `b"hello"` (verified against `sha256sum`).
const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

#[test]
fn first_scan_populates_sqlite_file_hash_cache() {
    let fixture = CatalogFixture::new("scan-cache-first");
    let context = fixture.context();
    let storage = fixture.storage();
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
    let fixture = CatalogFixture::new("scan-cache-rescan");
    let context = fixture.context();
    let storage = fixture.storage();
    let folder = TempGameFolder::new("scan-cache-rescan");

    create_dlss_file(folder.path(), b"hello");

    scan_catalog_folder(&context, folder.path(), "first scan");
    let sha_once = cache_sha_for_dll(storage, folder.path());

    scan_catalog_folder(&context, folder.path(), "second scan");
    let sha_twice = cache_sha_for_dll(storage, folder.path());

    assert_eq!(sha_once, sha_twice);
}

#[test]
fn scan_updates_sqlite_file_hash_cache_after_file_change() {
    let fixture = CatalogFixture::new("scan-cache-stale");
    let context = fixture.context();
    let storage = fixture.storage();
    let folder = TempGameFolder::new("scan-cache-stale");

    create_dlss_file(folder.path(), b"");
    scan_catalog_folder(&context, folder.path(), "first scan");

    let dlss_path = folder.path().join(DLSS_DLL_FILE_NAME);
    fs::write(&dlss_path, b"hello").expect("DLL contents should update");

    scan_catalog_folder(&context, folder.path(), "rescan after edit");

    let sha = cache_sha_for_dll(storage, folder.path());

    assert_eq!(sha.as_str(), HELLO_SHA256);
}

#[test]
fn failed_scan_does_not_overwrite_existing_file_hash_cache_rows() {
    let fixture = CatalogFixture::new("scan-cache-fail");
    let folder = TempGameFolder::new("scan-cache-fail");

    create_dlss_file(folder.path(), b"keep");

    let scope = path_string(folder.path());
    let dll_norm = normalized_dll_path(folder.path());

    let context = fixture.context();
    let storage = fixture.storage();
    scan_catalog_folder(&context, folder.path(), "first scan");

    let sha_before = storage
        .load_file_hash_cache(&scope)
        .expect("load cache")
        .into_iter()
        .find(|row| row.path == dll_norm)
        .expect("cache row")
        .sha256;
    let inspection =
        renderpilot_orchestration::catalog::inspect_game_install(&context, folder.path())
            .expect("inspection");

    fs::remove_dir_all(folder.path()).expect("remove scanned folder");

    let error = renderpilot_orchestration::catalog::add_game(
        &context,
        renderpilot_orchestration::catalog::AddGameRequest {
            selected_root: folder.path().to_path_buf(),
            root_choice: renderpilot_orchestration::catalog::AddGameRootChoice::Selected,
            allow_root_correction: false,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    );
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
