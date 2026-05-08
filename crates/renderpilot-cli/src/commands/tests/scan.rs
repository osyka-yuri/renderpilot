use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use crate::{
    catalog,
    commands::test_support::{path_string, temp_db_path, CatalogEnvironmentGuard, TempGameFolder},
    run,
};

const SCAN_FOLDER_COMMAND: &str = "scan-folder";

const DLSS_DLL_FILE_NAME: &str = "nvngx_dlss.dll";
const DLSS_TECHNOLOGY: &str = "dlss_super_resolution";
const EMPTY_FILE_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn scan_folder_outputs_single_game_json_with_detected_dlss_component() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-output"));
    let folder = TempGameFolder::new("cli-scan-folder");

    create_dlss_file(folder.path(), b"");

    let output = run_scan_folder_json(folder.path());

    assert!(
        required_field(&output, "game").is_object(),
        "scan output should contain object field `game`, got: {output:#?}",
    );

    let component = single_component(&output);
    assert_dlss_component(component);
}

#[test]
fn rescan_parent_prunes_removed_manual_child_game() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-prune"));
    let parent = TempGameFolder::new("cli-scan-prune");

    let game_a = create_child_game_with_dlss(parent.path(), "GameA", b"a-bytes");
    let game_b = create_child_game_with_dlss(parent.path(), "GameB", b"b-bytes");

    scan_catalog_folder(parent.path(), "first parent scan should succeed");
    assert_catalog_game_count(2, "first parent scan should discover two games");

    fs::remove_dir_all(&game_b).unwrap_or_else(|error| {
        panic!(
            "GameB directory should be removed at `{}`: {error}",
            game_b.display()
        )
    });

    scan_catalog_folder(parent.path(), "parent rescan should succeed");

    let install_paths = catalog_install_paths();

    assert_eq!(
        install_paths.len(),
        1,
        "removed child game should be pruned, got install paths: {install_paths:#?}",
    );

    let expected_scope = path_string(parent.path());
    assert_eq!(
        install_paths[0], expected_scope,
        "when one subdirectory remains, install path stays the user-selected parent scope; got install paths: {install_paths:#?}",
    );

    assert!(
        install_paths
            .iter()
            .all(|path| !path_ends_with(path, "GameB")),
        "GameB catalog row should be removed, got install paths: {install_paths:#?}",
    );

    assert!(
        path_ends_with(&game_a, "GameA"),
        "test setup should create GameA path, got `{}`",
        game_a.display(),
    );
}

#[test]
fn scan_child_does_not_prune_sibling_manual_game() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-prune-sibling"));
    let parent = TempGameFolder::new("cli-scan-prune-sib");

    let game_a = create_child_game_with_dlss(parent.path(), "GameA", b"a-bytes");
    let _game_b = create_child_game_with_dlss(parent.path(), "GameB", b"b-bytes");

    scan_catalog_folder(parent.path(), "parent scan should succeed");
    assert_catalog_game_count(2, "parent scan should discover two games");

    scan_catalog_folder(&game_a, "child scan should succeed");

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

    let output = run_scan_folder_json(parent.path());
    let games = required_array(&output, "games");

    assert_eq!(
        games.len(),
        2,
        "two child directories should produce two game entries, got: {output:#?}",
    );

    assert_game_titles(games, ["GameAlpha", "GameBeta"]);
}

#[test]
fn scan_folder_reports_missing_folder() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-missing-folder"));
    let folder = TempGameFolder::new("missing-cli-scan-folder");
    let missing_path = folder.path().to_path_buf();

    ensure_path_does_not_exist(&missing_path);

    let error = run(scan_folder_args(&missing_path)).unwrap_err();
    let message = error.to_string();

    assert!(
        message.contains("game folder does not exist"),
        "missing folder error should explain the problem, got: {message}",
    );
}

fn create_child_game_with_dlss(parent: &Path, game_name: &str, contents: &[u8]) -> PathBuf {
    let game_path = parent.join(game_name);
    create_dlss_file(&game_path, contents);
    game_path
}

pub(super) fn create_dlss_file(folder: &Path, contents: &[u8]) {
    fs::create_dir_all(folder).unwrap_or_else(|error| {
        panic!(
            "game folder should be created at `{}`: {error}",
            folder.display()
        )
    });

    let dlss_path = folder.join(DLSS_DLL_FILE_NAME);

    fs::write(&dlss_path, contents).unwrap_or_else(|error| {
        panic!(
            "DLSS test file should be written at `{}`: {error}",
            dlss_path.display()
        )
    });
}

fn ensure_path_does_not_exist(path: &Path) {
    if !path.exists() {
        return;
    }

    fs::remove_dir_all(path).unwrap_or_else(|error| {
        panic!(
            "test path should be removable before missing-folder assertion at `{}`: {error}",
            path.display()
        )
    });
}

fn run_scan_folder_json(path: &Path) -> Value {
    let output = run_scan_folder(path);

    serde_json::from_str(&output).unwrap_or_else(|error| {
        panic!(
            "scan-folder output should be valid JSON for `{}`: {error}\nOutput:\n{output}",
            path.display(),
        )
    })
}

fn run_scan_folder(path: &Path) -> String {
    run(scan_folder_args(path)).unwrap_or_else(|error| {
        panic!(
            "scan-folder command should succeed for `{}`: {error}",
            path.display()
        )
    })
}

pub(super) fn scan_catalog_folder(path: &Path, context: &str) {
    catalog::scan_folder(path.to_path_buf())
        .unwrap_or_else(|error| panic!("{context} for `{}`: {error}", path.display()));
}

fn scan_folder_args(path: &Path) -> Vec<OsString> {
    vec![
        OsString::from(SCAN_FOLDER_COMMAND),
        path.as_os_str().to_owned(),
    ]
}

fn assert_catalog_game_count(expected: usize, message: &str) {
    let actual = catalog_install_paths().len();

    assert_eq!(actual, expected, "{message}");
}

fn catalog_install_paths() -> Vec<String> {
    catalog::list_games()
        .expect("catalog games should be listed")
        .into_iter()
        .map(|game| game.install_path().as_str().to_owned())
        .collect()
}

fn single_component(output: &Value) -> &Value {
    let components = required_array(output, "components");

    assert_eq!(
        components.len(),
        1,
        "expected exactly one component, got: {output:#?}",
    );

    &components[0]
}

fn assert_dlss_component(component: &Value) {
    assert_json_string(component, "file_name", DLSS_DLL_FILE_NAME);
    assert_json_string(component, "technology", DLSS_TECHNOLOGY);
    assert_json_string(component, "kind", "NativeLibrary");
    assert_json_string(component, "detection_confidence", "High");
    assert_json_string(component, "swappability", "Swappable");
    assert_json_string(component, "status", "unknown_version");
    assert_json_string(component, "sha256", EMPTY_FILE_SHA256);
    assert_json_null(component, "version");

    let cache_key = required_field(component, "cache_key");

    assert_json_string(cache_key, "sha256", EMPTY_FILE_SHA256);
    assert_json_number(cache_key, "size", 0);
    assert_required_non_null(cache_key, "modified_at");

    let cache_path = json_string_field(cache_key, "path");

    assert!(
        path_ends_with(cache_path, DLSS_DLL_FILE_NAME),
        "cache_key.path should point to the DLSS DLL, got `{cache_path}`",
    );
}

fn assert_game_titles<const N: usize>(games: &[Value], expected: [&str; N]) {
    let actual_titles = game_titles(games);
    let expected_titles = expected.into_iter().collect::<BTreeSet<_>>();

    assert_eq!(
        actual_titles, expected_titles,
        "unexpected game titles in scan output",
    );
}

fn game_titles(games: &[Value]) -> BTreeSet<&str> {
    games.iter().map(game_title).collect()
}

fn game_title(game_entry: &Value) -> &str {
    game_entry
        .get("game")
        .and_then(|game| game.get("identity"))
        .and_then(|identity| identity.get("title"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!("game entry should contain game.identity.title, got: {game_entry:#?}")
        })
}

fn required_array<'a>(json: &'a Value, field: &str) -> &'a [Value] {
    required_field(json, field)
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("expected JSON field `{field}` to be an array, got: {json:#?}"))
}

fn required_field<'a>(json: &'a Value, field: &str) -> &'a Value {
    json.get(field)
        .unwrap_or_else(|| panic!("expected JSON field `{field}` to be present, got: {json:#?}"))
}

fn json_string_field<'a>(json: &'a Value, field: &str) -> &'a str {
    required_field(json, field)
        .as_str()
        .unwrap_or_else(|| panic!("expected JSON field `{field}` to be a string, got: {json:#?}"))
}

fn json_number_field(json: &Value, field: &str) -> i64 {
    required_field(json, field)
        .as_i64()
        .unwrap_or_else(|| panic!("expected JSON field `{field}` to be an integer, got: {json:#?}"))
}

fn assert_json_string(json: &Value, field: &str, expected: &str) {
    let actual = json_string_field(json, field);

    assert_eq!(
        actual, expected,
        "unexpected value for JSON field `{field}`",
    );
}

fn assert_json_number(json: &Value, field: &str, expected: i64) {
    let actual = json_number_field(json, field);

    assert_eq!(
        actual, expected,
        "unexpected value for JSON field `{field}`",
    );
}

fn assert_json_null(json: &Value, field: &str) {
    assert!(
        matches!(json.get(field), Some(Value::Null)),
        "expected JSON field `{field}` to be null, got: {:?}",
        json.get(field),
    );
}

fn assert_required_non_null(json: &Value, field: &str) {
    let value = required_field(json, field);

    assert!(
        !value.is_null(),
        "expected JSON field `{field}` to be present and non-null, got: {json:#?}",
    );
}

fn path_ends_with(path: impl AsRef<Path>, child: impl AsRef<Path>) -> bool {
    path.as_ref().ends_with(child)
}
