//! Integration tests for owner-scoped strong observations published by scans.

use std::{fs, path::Path};

use renderpilot_orchestration::{application::ComponentRepository, catalog, domain::GameId};
use renderpilot_storage_sqlite::{CatalogReadiness, ObservationOwner};

use crate::commands::test_support::{CatalogFixture, TempGameFolder, path_string};

use super::scan::{create_dlss_file, scan_catalog_folder};

const DLSS_DLL_FILE_NAME: &str = "nvngx_dlss.dll";

/// SHA-256 of `b"hello"` (verified against `sha256sum`).
const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

#[test]
fn first_scan_publishes_owner_scoped_complete_observation() {
    let fixture = CatalogFixture::new("scan-observation-first");
    let context = fixture.context();
    let storage = fixture.storage();
    let folder = TempGameFolder::new("scan-observation-first");

    create_dlss_file(folder.path(), b"hello");
    scan_catalog_folder(&context, folder.path(), "first scan");

    let game_id = game_id_for_folder(&context, folder.path());
    let observations = storage
        .list_game_observations(&game_id)
        .expect("game observations");
    if let Some(observation) = observations.iter().find(|observation| {
        observation.normalized_path.as_str() == normalized_dll_path(folder.path())
    }) {
        assert_eq!(observation.owner, ObservationOwner::Game(game_id.clone()));
        assert_eq!(observation.sha256.as_str(), HELLO_SHA256);
        assert_eq!(observation.size, 5);
        assert_eq!(observation.algorithm_revision, 2);
        assert!(!observation.identity_kind.is_empty());
        assert!(!observation.object_identity.is_empty());
        assert!(!observation.change_token.is_empty());
        assert!(observation.version_observed);
        assert!(observation.runtime_observed);
        assert!(observation.pe_observed);
    } else {
        // This filesystem cannot produce a FILE_ID_INFO+USN continuity key.
        // The full scan is still healthy, but owner replacement persists zero
        // reuse rows instead of retaining a weak timestamp/size observation.
        assert!(
            storage
                .list_components_for_game(&game_id)
                .expect("components")
                .iter()
                .flat_map(|component| component.files())
                .any(
                    |file| file.path().as_str() == normalized_dll_path(folder.path())
                        && file
                            .sha256()
                            .is_some_and(|sha| sha.as_str() == HELLO_SHA256)
                ),
            "an uncacheable file still publishes its component/hash facts"
        );
        assert!(observations.is_empty());
    }
    assert!(matches!(
        storage
            .catalog_readiness(&game_id)
            .expect("catalog readiness"),
        CatalogReadiness::Complete(_)
    ));
}

#[test]
fn observations_are_scoped_to_the_game_that_published_them() {
    let fixture = CatalogFixture::new("scan-observation-owner");
    let context = fixture.context();
    let storage = fixture.storage();
    let first = TempGameFolder::new("scan-observation-owner-first");
    let second = TempGameFolder::new("scan-observation-owner-second");

    create_dlss_file(first.path(), b"first");
    create_dlss_file(second.path(), b"second");
    scan_catalog_folder(&context, first.path(), "first game scan");
    scan_catalog_folder(&context, second.path(), "second game scan");

    let first_id = game_id_for_folder(&context, first.path());
    let second_id = game_id_for_folder(&context, second.path());
    let first_observations = storage
        .list_game_observations(&first_id)
        .expect("first game observations");
    let second_observations = storage
        .list_game_observations(&second_id)
        .expect("second game observations");

    assert!(first_observations.iter().all(|observation| {
        observation.owner == ObservationOwner::Game(first_id.clone())
            && observation
                .normalized_path
                .as_str()
                .starts_with(&path_string(first.path()))
    }));
    assert!(second_observations.iter().all(|observation| {
        observation.owner == ObservationOwner::Game(second_id.clone())
            && observation
                .normalized_path
                .as_str()
                .starts_with(&path_string(second.path()))
    }));
    assert!(first_observations.iter().all(|observation| {
        observation
            .normalized_path
            .as_str()
            .starts_with(&path_string(first.path()))
    }));
    assert!(second_observations.iter().all(|observation| {
        observation
            .normalized_path
            .as_str()
            .starts_with(&path_string(second.path()))
    }));
}

#[test]
fn unchanged_rescan_retains_the_complete_strong_observation() {
    let fixture = CatalogFixture::new("scan-observation-rescan");
    let context = fixture.context();
    let storage = fixture.storage();
    let folder = TempGameFolder::new("scan-observation-rescan");

    create_dlss_file(folder.path(), b"hello");
    scan_catalog_folder(&context, folder.path(), "first scan");

    let game_id = game_id_for_folder(&context, folder.path());
    let observations_before = storage
        .list_game_observations(&game_id)
        .expect("first scan observations");

    scan_catalog_folder(&context, folder.path(), "unchanged rescan");

    let observations_after = storage
        .list_game_observations(&game_id)
        .expect("second scan observations");
    assert_eq!(observations_after, observations_before);
    assert!(observations_after.iter().all(|observation| {
        observation.algorithm_revision == 2
            && observation.version_observed
            && observation.runtime_observed
            && observation.pe_observed
            && !observation.identity_kind.is_empty()
            && !observation.object_identity.is_empty()
            && !observation.change_token.is_empty()
    }));
    assert!(matches!(
        storage
            .catalog_readiness(&game_id)
            .expect("catalog readiness"),
        CatalogReadiness::Complete(_)
    ));
}

#[test]
fn failed_scan_does_not_overwrite_existing_observations() {
    let fixture = CatalogFixture::new("scan-observation-fail");
    let folder = TempGameFolder::new("scan-observation-fail");

    create_dlss_file(folder.path(), b"keep");

    let context = fixture.context();
    let storage = fixture.storage();
    scan_catalog_folder(&context, folder.path(), "first scan");

    let game_id = game_id_for_folder(&context, folder.path());
    let observations_before = storage
        .list_game_observations(&game_id)
        .expect("first scan observations");
    let inspection = catalog::inspect_game_install(&context, folder.path()).expect("inspection");

    fs::remove_dir_all(folder.path()).expect("remove scanned folder");

    let error = catalog::add_game(
        &context,
        catalog::AddGameRequest {
            selected_root: folder.path().to_path_buf(),
            root_choice: catalog::AddGameRootChoice::Selected,
            allow_root_correction: false,
            chosen_executable: None,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    );
    assert!(
        error.is_err(),
        "scan should fail when the game folder no longer exists",
    );

    let observations_after = storage
        .list_game_observations(&game_id)
        .expect("observations after failed scan");
    assert_eq!(observations_after, observations_before);
    assert!(matches!(
        storage
            .catalog_readiness(&game_id)
            .expect("catalog readiness"),
        CatalogReadiness::Complete(_)
    ));
}

fn game_id_for_folder(context: &renderpilot_orchestration::Context, folder: &Path) -> GameId {
    catalog::list_games(context)
        .expect("games")
        .into_iter()
        .find(|game| game.install_path().as_str() == path_string(folder))
        .unwrap_or_else(|| panic!("game for `{}`", folder.display()))
        .id()
        .clone()
}

fn normalized_dll_path(folder: &Path) -> String {
    path_string(&folder.join(DLSS_DLL_FILE_NAME))
}
