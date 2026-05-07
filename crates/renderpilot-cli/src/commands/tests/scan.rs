use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use crate::{catalog, run};

use super::{temp_db_path, CatalogEnvironmentGuard, TempGameFolder};

const SCAN_FOLDER_COMMAND: &str = "scan-folder";
const DLSS_DLL_FILE_NAME: &str = "nvngx_dlss.dll";
const EMPTY_FILE_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn scan_folder_outputs_single_game_json_with_detected_dlss_component() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-output"));
    let folder = TempGameFolder::new("cli-scan-folder");

    create_dlss_file(folder.path(), b"");

    let json = run_scan_folder_json(folder.path());

    assert!(
        json.get("game").is_some(),
        "scan output should contain `game`"
    );

    let components = json_array(&json, "components");
    assert_eq!(components.len(), 1, "expected exactly one component");

    assert_dlss_component(&components[0]);
}

#[test]
fn rescan_parent_prunes_removed_manual_child_game() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-prune"));
    let parent = TempGameFolder::new("cli-scan-prune");

    let _game_a = create_child_game_with_dlss(parent.path(), "GameA", b"a-bytes");
    let game_b = create_child_game_with_dlss(parent.path(), "GameB", b"b-bytes");

    catalog::scan_folder(parent.path().to_path_buf()).expect("first parent scan should succeed");
    assert_catalog_game_count(2, "first parent scan should discover two games");

    fs::remove_dir_all(&game_b).expect("GameB directory should be removed");

    catalog::scan_folder(parent.path().to_path_buf()).expect("parent rescan should succeed");

    let games = catalog::list_games().expect("catalog games should be listed");
    assert_eq!(games.len(), 1, "removed child game should be pruned");

    let remaining_install_path = games[0].install_path().as_str();

    assert!(
        !remaining_install_path.contains("GameB"),
        "GameB catalog row should be removed, got {remaining_install_path}"
    );
}

#[test]
fn scan_child_does_not_prune_sibling_manual_game() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-prune-sibling"));
    let parent = TempGameFolder::new("cli-scan-prune-sib");

    let game_a = create_child_game_with_dlss(parent.path(), "GameA", b"a-bytes");
    let _game_b = create_child_game_with_dlss(parent.path(), "GameB", b"b-bytes");

    catalog::scan_folder(parent.path().to_path_buf()).expect("parent scan should succeed");
    assert_catalog_game_count(2, "parent scan should discover two games");

    catalog::scan_folder(game_a).expect("child scan should succeed");

    assert_catalog_game_count(
        2,
        "sibling manual game must remain when only one subdirectory is scanned",
    );
}

#[test]
fn scan_parent_outputs_games_array_for_multiple_child_installs() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-multi"));
    let parent = TempGameFolder::new("cli-scan-parent");

    create_child_game_with_dlss(parent.path(), "GameAlpha", b"alpha-bytes");
    create_child_game_with_dlss(parent.path(), "GameBeta", b"beta-bytes");

    let json = run_scan_folder_json(parent.path());

    let games = json_array(&json, "games");

    assert_eq!(
        games.len(),
        2,
        "two child directories should produce two game entries",
    );

    let titles = game_titles(games);

    assert!(
        titles.contains(&"GameAlpha"),
        "GameAlpha should be present in scan output, got {titles:?}",
    );

    assert!(
        titles.contains(&"GameBeta"),
        "GameBeta should be present in scan output, got {titles:?}",
    );
}

#[test]
fn scan_folder_reports_missing_folder() {
    let folder = TempGameFolder::new("missing-cli-scan-folder");

    let error = run(scan_folder_args(folder.path())).expect_err("missing folder should fail");

    assert!(
        error.to_string().contains("game folder does not exist"),
        "missing folder error should explain the problem, got: {error}",
    );
}

fn create_child_game_with_dlss(parent: &Path, game_name: &str, contents: &[u8]) -> PathBuf {
    let game_path = parent.join(game_name);

    create_dlss_file(&game_path, contents);

    game_path
}

fn create_dlss_file(folder: &Path, contents: &[u8]) {
    fs::create_dir_all(folder).expect("game folder should be created");

    fs::write(folder.join(DLSS_DLL_FILE_NAME), contents).expect("DLSS test file should be written");
}

fn run_scan_folder_json(path: &Path) -> Value {
    let output = run_scan_folder(path);

    serde_json::from_str(&output).expect("scan-folder output should be valid JSON")
}

fn run_scan_folder(path: &Path) -> String {
    run(scan_folder_args(path)).expect("scan-folder command should succeed")
}

fn scan_folder_args(path: &Path) -> Vec<OsString> {
    vec![
        OsString::from(SCAN_FOLDER_COMMAND),
        path.as_os_str().to_owned(),
    ]
}

fn assert_catalog_game_count(expected: usize, message: &str) {
    let games = catalog::list_games().expect("catalog games should be listed");

    assert_eq!(games.len(), expected, "{message}");
}

fn assert_dlss_component(component: &Value) {
    assert_json_string(component, "file_name", DLSS_DLL_FILE_NAME);
    assert_json_string(component, "technology", "DlssSuperResolution");
    assert_json_string(component, "kind", "NativeLibrary");
    assert_json_string(component, "detection_confidence", "High");
    assert_json_string(component, "swappability", "Swappable");
    assert_json_string(component, "status", "unknown_version");
    assert_json_string(component, "sha256", EMPTY_FILE_SHA256);

    assert!(
        component.get("version").map_or(false, Value::is_null),
        "expected component.version to be null, got {:?}",
        component.get("version"),
    );

    let cache_key = component
        .get("cache_key")
        .expect("component should contain cache_key");

    assert_json_string(cache_key, "sha256", EMPTY_FILE_SHA256);
    assert_json_number(cache_key, "size", 0);

    assert!(
        cache_key.get("modified_at").is_some(),
        "cache_key should contain modified_at",
    );

    assert!(
        cache_key.get("path").is_some(),
        "cache_key should contain path",
    );
}

fn json_array<'a>(json: &'a Value, field: &str) -> &'a [Value] {
    json.get(field)
        .and_then(Value::as_array)
        .map(|items| items.as_slice())
        .unwrap_or_else(|| panic!("expected top-level JSON field `{field}` to be an array"))
}

fn json_string_field<'a>(json: &'a Value, field: &str) -> &'a str {
    json.get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected JSON field `{field}` to be a string"))
}

fn assert_json_string(json: &Value, field: &str, expected: &str) {
    assert_eq!(
        json_string_field(json, field),
        expected,
        "unexpected value for JSON field `{field}`",
    );
}

fn assert_json_number(json: &Value, field: &str, expected: i64) {
    let actual = json
        .get(field)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("expected JSON field `{field}` to be an integer"));

    assert_eq!(
        actual, expected,
        "unexpected value for JSON field `{field}`",
    );
}

fn game_titles(games: &[Value]) -> Vec<&str> {
    games.iter().map(game_title).collect()
}

fn game_title(game_entry: &Value) -> &str {
    game_entry
        .get("game")
        .and_then(|game| game.get("identity"))
        .and_then(|identity| identity.get("title"))
        .and_then(Value::as_str)
        .expect("game entry should contain game.identity.title")
}
