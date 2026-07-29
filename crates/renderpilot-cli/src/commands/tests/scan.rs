use std::{ffi::OsString, fs, path::Path};

use renderpilot_orchestration::application::GameRepository;
use renderpilot_orchestration::domain::{
    ComponentFile, ComponentId, ComponentKind, GameId, GameIdentity, GameInstallation, GameRuntime,
    GraphicsComponent, GraphicsTechnology, Launcher, PathRef, Platform, RootAuthority,
    Swappability,
};
use renderpilot_orchestration::{Context, catalog};
use serde_json::Value;

use crate::commands::test_support::{CatalogFixture, TempGameFolder, path_string};

const ADD_GAME_COMMAND: &str = "add-game";
const DLSS_DLL_FILE_NAME: &str = "nvngx_dlss.dll";

#[test]
fn add_game_outputs_one_structured_result() {
    let fixture = CatalogFixture::new("add-game-output");
    let folder = TempGameFolder::new("cli-add-game");
    create_game_executable(folder.path(), "BlackFlag.exe");
    create_dlss_file(folder.path(), b"dlss");

    let output = run_add_game_json(&fixture, folder.path());

    assert!(output["gameId"].as_str().is_some());
    assert_eq!(output["effectiveRoot"], path_string(folder.path()));
    assert_eq!(output["disposition"], "added");
    assert_eq!(output["detectedLibraryCount"], 1);
    assert!(output["warnings"].is_array());
}

#[test]
fn add_game_is_idempotent_and_keeps_stable_game_id() {
    let fixture = CatalogFixture::new("add-game-repeat");
    let folder = TempGameFolder::new("cli-add-game-repeat");
    create_game_executable(folder.path(), "Game.exe");
    create_dlss_file(folder.path(), b"dlss");

    let first = run_add_game_json(&fixture, folder.path());
    let second = run_add_game_json(&fixture, folder.path());

    assert_eq!(first["gameId"], second["gameId"]);
    assert_eq!(second["disposition"], "unchanged");
    assert_eq!(
        catalog::list_games(&fixture.context())
            .expect("games")
            .len(),
        1
    );
}

#[test]
fn black_flag_nested_middleware_fixture_creates_exactly_one_card() {
    let fixture = CatalogFixture::new("issue-5-black-flag");
    let folder = TempGameFolder::new("Assassins-Creed-Black-Flag");
    create_game_executable(folder.path(), "AC4BFSP.exe");
    write_file(&folder.path().join("dstorage.dll"), b"direct-storage");
    write_file(&folder.path().join("D3D12").join("D3D12Core.dll"), b"d3d12");
    write_file(
        &folder
            .path()
            .join("NVStreamline")
            .join("production")
            .join(DLSS_DLL_FILE_NAME),
        b"dlss",
    );

    run_add_game_json(&fixture, folder.path());

    let games = catalog::list_games(&fixture.context()).expect("games");
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].install_path().as_str(), path_string(folder.path()));
    assert!(
        games
            .iter()
            .all(|game| !game.install_path().as_str().ends_with("/D3D12")
                && !game.install_path().as_str().contains("/NVStreamline")),
        "middleware directories must never become game roots"
    );
    let components = fixture.storage().list_all_components().expect("components");
    assert!(
        components.len() >= 2,
        "root and nested component files should remain on the one card"
    );
}

#[test]
fn inspection_does_not_promote_a_game_library_container() {
    let fixture = CatalogFixture::new("add-game-library-container-advice");
    let library = TempGameFolder::new("add-game-library-container");
    let selected = library.path().join("The Last of Us Part I");
    let sibling = library.path().join("Another Game");
    create_game_executable(&selected, "tlou-i.exe");
    create_game_executable(&sibling, "AnotherGame.exe");
    create_dlss_file(&selected, b"dlss");
    fs::create_dir_all(selected.join("a/b/c/d/e")).expect("deep selected tree");

    let inspection =
        catalog::inspect_game_install(&fixture.context(), &selected).expect("inspection");

    assert_eq!(
        inspection.selected_root.path().as_str(),
        path_string(&selected)
    );
    assert_eq!(
        inspection.recommendation, None,
        "a sibling game's executable must not turn the library container into one install"
    );
    assert!(
        inspection
            .warnings
            .iter()
            .all(|warning| !matches!(warning, catalog::AddGameWarning::FilesystemProbeError)),
        "an intentional depth limit must not be presented as an access failure: {:?}",
        inspection.warnings
    );
}

#[test]
fn inspection_recommends_parent_with_a_root_level_game_executable() {
    let fixture = CatalogFixture::new("add-game-parent-root-advice");
    let library = TempGameFolder::new("add-game-parent-root");
    let game_root = library.path().join("Black Flag");
    let selected = game_root.join("D3D12");
    create_game_executable(&game_root, "AC4BFSP.exe");
    write_file(&selected.join("D3D12Core.dll"), b"d3d12");

    let inspection =
        catalog::inspect_game_install(&fixture.context(), &selected).expect("inspection");

    assert_eq!(
        inspection
            .recommendation
            .as_ref()
            .map(|recommendation| recommendation.root.path().as_str().to_owned()),
        Some(path_string(&game_root)),
        "root-local executable evidence should still recover a nested middleware selection"
    );
    assert!(
        inspection
            .warnings
            .iter()
            .all(|warning| !matches!(warning, catalog::AddGameWarning::InsideExistingInstall)),
        "a recommendation is presented as a root choice, not as a speculative warning"
    );
}

#[cfg(windows)]
#[test]
fn black_flag_parent_scan_consolidates_proven_false_legacy_children() {
    let (fixture, folder, output) = run_black_flag_legacy_consolidation_fixture();

    assert_eq!(
        output["consolidatedGameIds"]
            .as_array()
            .expect("consolidated ids")
            .len(),
        2,
    );
    let games = catalog::list_games(&fixture.context()).expect("games");
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].install_path().as_str(), path_string(folder.path()));
}

#[cfg(not(windows))]
#[test]
fn black_flag_parent_scan_retains_legacy_children_without_launcher_inventory() {
    let (fixture, _folder, output) = run_black_flag_legacy_consolidation_fixture();

    assert_eq!(
        output["consolidatedGameIds"]
            .as_array()
            .expect("consolidated ids")
            .len(),
        0,
        "missing Windows launcher inventory must never be interpreted as proof that no child is launcher-owned",
    );
    assert!(
        output["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| {
                warning["code"] == "legacy_cards_retained" && warning["parameters"]["count"] == 2
            }),
        "the fail-closed retention must remain visible to callers",
    );
    let games = catalog::list_games(&fixture.context()).expect("games");
    assert_eq!(games.len(), 3);
}

fn run_black_flag_legacy_consolidation_fixture() -> (CatalogFixture, TempGameFolder, Value) {
    let fixture = CatalogFixture::new("issue-5-black-flag-legacy");
    let folder = TempGameFolder::new("Assassins-Creed-Black-Flag-Legacy");
    create_game_executable(folder.path(), "AC4BFSP.exe");
    let d3d12 = folder.path().join("D3D12").join("D3D12Core.dll");
    let streamline = folder
        .path()
        .join("NVStreamline")
        .join("production")
        .join(DLSS_DLL_FILE_NAME);
    write_file(&d3d12, b"d3d12");
    write_file(&streamline, b"dlss");
    seed_legacy_child(
        &fixture,
        "manual:false-d3d12",
        &folder.path().join("D3D12"),
        &d3d12,
        GraphicsTechnology::D3D12Agility,
    );
    seed_legacy_child(
        &fixture,
        "manual:false-streamline",
        &folder.path().join("NVStreamline"),
        &streamline,
        GraphicsTechnology::DlssSuperResolution,
    );

    let output = run_add_game_json(&fixture, folder.path());

    (fixture, folder, output)
}

#[test]
fn parent_with_an_independent_pe_install_is_rejected_without_consolidation() {
    let fixture = CatalogFixture::new("issue-5-independent-child");
    let folder = TempGameFolder::new("Parent-With-Independent-Child");
    create_game_executable(folder.path(), "Parent.exe");
    let false_component = folder.path().join("D3D12").join("D3D12Core.dll");
    let independent_root = folder.path().join("Independent");
    let independent_component = independent_root.join(DLSS_DLL_FILE_NAME);
    write_file(&false_component, b"d3d12");
    write_file(&independent_component, b"dlss");
    create_game_executable(&independent_root, "Independent.exe");
    seed_legacy_child(
        &fixture,
        "manual:false-child",
        &folder.path().join("D3D12"),
        &false_component,
        GraphicsTechnology::D3D12Agility,
    );
    seed_legacy_child(
        &fixture,
        "manual:independent-child",
        &independent_root,
        &independent_component,
        GraphicsTechnology::DlssSuperResolution,
    );

    let error = fixture.run(add_game_args(folder.path())).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("contains_multiple_catalog_installs"),
        "unexpected error: {error}"
    );
    let games = catalog::list_games(&fixture.context()).expect("games");
    assert_eq!(games.len(), 2);
    assert!(
        games
            .iter()
            .any(|game| game.id().as_str() == "manual:false-child"),
        "a rejected parent must not consolidate even a proven-false child"
    );
    assert!(
        games
            .iter()
            .any(|game| game.id().as_str() == "manual:independent-child"),
    );
}

#[test]
fn add_game_treats_selected_parent_as_one_install_not_a_batch() {
    let fixture = CatalogFixture::new("add-game-parent");
    let folder = TempGameFolder::new("cli-add-game-parent");
    create_game_executable(folder.path(), "ParentGame.exe");
    create_dlss_file(&folder.path().join("D3D12"), b"nested");
    create_dlss_file(&folder.path().join("NVStreamline"), b"nested");

    run_add_game_json(&fixture, folder.path());

    let games = catalog::list_games(&fixture.context()).expect("games");
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].install_path().as_str(), path_string(folder.path()));
}

#[test]
fn add_game_reports_missing_folder() {
    let fixture = CatalogFixture::new("add-game-missing-folder");
    let folder = TempGameFolder::new("missing-cli-add-game");
    let missing_path = folder.path().to_path_buf();

    let error = fixture.run(add_game_args(&missing_path)).unwrap_err();
    assert!(error.to_string().contains("game folder does not exist"));
}

#[test]
fn add_game_explicit_executable_is_persisted_when_ranking_rejects_it() {
    let fixture = CatalogFixture::new("add-game-explicit-executable");
    let folder = TempGameFolder::new("explicit-executable");
    let executable = folder.path().join("CustomLauncher.exe");
    create_game_executable(folder.path(), "CustomLauncher.exe");

    fixture
        .run(vec![
            OsString::from(ADD_GAME_COMMAND),
            folder.path().as_os_str().to_owned(),
            OsString::from("--executable"),
            executable.as_os_str().to_owned(),
            OsString::from("--root-choice"),
            OsString::from("selected"),
        ])
        .expect("explicit executable should make the install addable");
    fixture
        .run(vec![
            OsString::from(ADD_GAME_COMMAND),
            folder.path().as_os_str().to_owned(),
            OsString::from("--root-choice"),
            OsString::from("selected"),
        ])
        .expect("idempotent refresh should preserve the explicit executable");

    let games = catalog::list_games(&fixture.context()).expect("games");
    assert_eq!(games.len(), 1);
    assert_eq!(
        games[0]
            .confirmed_executable()
            .map(|candidate| candidate.as_str()),
        Some("CustomLauncher.exe")
    );
    assert!(
        games[0]
            .executable_candidates()
            .iter()
            .any(|candidate| candidate.as_str() == "CustomLauncher.exe"),
    );
}

#[test]
fn root_correction_remaps_and_preserves_confirmed_executable() {
    let fixture = CatalogFixture::new("add-game-root-correction-executable");
    let parent = TempGameFolder::new("root-correction-parent");
    let child = parent.path().join("Bin");
    create_game_executable(&child, "CustomLauncher.exe");
    write_file(&parent.path().join("Data/game.pak"), b"distribution");

    let stable_id = GameId::new("game:root-correction-stable").expect("game id");
    let existing = GameInstallation::new(
        GameIdentity::new(stable_id.clone(), "Nested install", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(path_string(&child)).expect("child root"),
    )
    .with_root_authority(RootAuthority::UserConfirmed)
    .with_confirmed_executable(PathRef::new("CustomLauncher.exe").expect("confirmed executable"));
    fixture
        .storage()
        .upsert_game(&existing)
        .expect("seed existing card");

    let output = fixture
        .run(vec![
            OsString::from(ADD_GAME_COMMAND),
            parent.path().as_os_str().to_owned(),
            OsString::from("--root-choice"),
            OsString::from("selected"),
            OsString::from("--allow-root-correction"),
        ])
        .expect("explicit root correction");
    let output: Value = serde_json::from_str(&output).expect("root-correction JSON");
    assert_eq!(output["gameId"], stable_id.as_str());
    assert_eq!(output["disposition"], "root_corrected");

    let games = catalog::list_games(&fixture.context()).expect("games");
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].id(), &stable_id);
    assert_eq!(games[0].install_path().as_str(), path_string(parent.path()));
    assert_eq!(
        games[0]
            .confirmed_executable()
            .map(|candidate| candidate.as_str()),
        Some("Bin/CustomLauncher.exe"),
    );
}

pub(super) fn create_dlss_file(folder: &Path, contents: &[u8]) {
    write_file(&folder.join(DLSS_DLL_FILE_NAME), contents);
}

pub(super) fn create_game_executable(folder: &Path, name: &str) {
    let mut bytes = vec![0_u8; 0x84];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    write_file(&folder.join(name), &bytes);
}

pub(super) fn scan_catalog_folder(context: &Context, path: &Path, message: &str) {
    if !path.join("RenderPilotTestGame.exe").is_file() {
        create_game_executable(path, "RenderPilotTestGame.exe");
    }
    let inspection = catalog::inspect_game_install(context, path)
        .unwrap_or_else(|error| panic!("{message} for `{}`: {error}", path.display()));
    catalog::add_game(
        context,
        catalog::AddGameRequest {
            selected_root: path.to_path_buf(),
            root_choice: catalog::AddGameRootChoice::Selected,
            allow_root_correction: false,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )
    .unwrap_or_else(|error| panic!("{message} for `{}`: {error}", path.display()));
}

fn run_add_game_json(fixture: &CatalogFixture, path: &Path) -> Value {
    let output = fixture.run(add_game_args(path)).unwrap_or_else(|error| {
        panic!(
            "add-game command should succeed for `{}`: {error}",
            path.display()
        )
    });
    serde_json::from_str(&output).unwrap_or_else(|error| {
        panic!(
            "add-game output should be valid JSON for `{}`: {error}\n{output}",
            path.display()
        )
    })
}

fn add_game_args(path: &Path) -> Vec<OsString> {
    vec![
        OsString::from(ADD_GAME_COMMAND),
        path.as_os_str().to_owned(),
    ]
}

fn write_file(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("fixture directory {}: {error}", parent.display()));
    }
    fs::write(path, contents)
        .unwrap_or_else(|error| panic!("fixture file {}: {error}", path.display()));
}

fn seed_legacy_child(
    fixture: &CatalogFixture,
    id: &str,
    root: &Path,
    component_path: &Path,
    technology: GraphicsTechnology,
) {
    let game = GameInstallation::new(
        GameIdentity::new(
            GameId::new(id).expect("legacy game id"),
            "Legacy false child",
            Launcher::Manual,
        )
        .expect("legacy identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(path_string(root)).expect("legacy root"),
    );
    let component = GraphicsComponent::new(
        ComponentId::new(format!("{id}:component")).expect("component id"),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        technology,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(
        PathRef::new(path_string(component_path)).expect("component path"),
    ));
    fixture.store_game(&game);
    fixture.store_components(game.id(), &[component]);
}
