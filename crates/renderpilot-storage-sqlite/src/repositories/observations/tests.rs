use std::collections::HashMap;

use renderpilot_application::{ArtifactRepository, ComponentRepository, GameRepository};
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, GameIdentity, GameInstallation, GameRuntime,
    Launcher, LibraryArtifact, LibraryTechnology, PathRef, Platform, Sha256Hash,
};

use super::super::{PendingFileMutationRow, PendingFileMutationState};
use super::{
    AuthorityCas, CatalogReadiness, ObservationOwner, SqliteStorage, StoredFileObservation,
};

fn store_game(storage: &SqliteStorage, id: &str) -> renderpilot_domain::GameId {
    let game_id = renderpilot_domain::GameId::new(id).expect("game id");
    let identity =
        GameIdentity::new(game_id.clone(), "Test Game", Launcher::Steam).expect("game identity");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Test").expect("path"),
    );
    storage.upsert_game(&game).expect("store game");
    game_id
}

fn observation(game_id: &renderpilot_domain::GameId) -> StoredFileObservation {
    StoredFileObservation {
        owner: ObservationOwner::Game(game_id.clone()),
        normalized_path: PathRef::new("C:/Games/Test/nvngx_dlss.dll").expect("path"),
        identity_kind: "test_identity".to_owned(),
        object_identity: "object-1".to_owned(),
        change_token: "token-1".to_owned(),
        size: 1,
        algorithm_revision: 1,
        sha256: Sha256Hash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .expect("sha"),
        version_observed: true,
        version: None,
        runtime_observed: true,
        runtime_json: None,
        pe_observed: true,
        pe_json: None,
    }
}

fn store_artifact(storage: &SqliteStorage, id: &str, path: &str) -> ArtifactId {
    let artifact_id = ArtifactId::new(id).expect("artifact id");
    let artifact = LibraryArtifact::new(
        artifact_id.clone(),
        LibraryTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        vec![
            ComponentFile::new(PathRef::new(path).expect("artifact path")).with_sha256(
                Sha256Hash::new("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                    .expect("artifact sha"),
            ),
        ],
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact");
    storage.upsert_artifact(&artifact).expect("store artifact");
    artifact_id
}

fn artifact_observation(
    artifact_id: &ArtifactId,
    path: &str,
    token: &str,
) -> StoredFileObservation {
    StoredFileObservation {
        owner: ObservationOwner::Artifact(artifact_id.clone()),
        normalized_path: PathRef::new(path).expect("observation path"),
        identity_kind: "test_identity".to_owned(),
        object_identity: format!("object-{token}"),
        change_token: token.to_owned(),
        size: 1,
        algorithm_revision: 1,
        sha256: Sha256Hash::new("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
            .expect("sha"),
        version_observed: false,
        version: None,
        runtime_observed: false,
        runtime_json: None,
        pe_observed: false,
        pe_json: None,
    }
}

fn complete_game_scan(storage: &SqliteStorage, game_id: &renderpilot_domain::GameId) {
    let game = storage.require_game(game_id).expect("read game");
    let observations = [observation(game_id)];
    storage
        .save_complete_scan_write_unit(super::super::CompleteScanWriteUnit {
            game: &game,
            components: &[],
            artifacts: &[],
            observations: &observations,
            authority: AuthorityCas::new(0),
            prune_empty_operations: false,
        })
        .expect("complete scan");
}

#[test]
fn new_game_is_never_completed_and_component_replacement_invalidates_it() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = store_game(&storage, "steam:authority");

    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        CatalogReadiness::NeverCompleted { authority_epoch: 0 }
    );

    storage
        .replace_components_for_game(&game_id, &[])
        .expect("replacement");
    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        CatalogReadiness::Invalidated {
            authority_epoch: 1,
            reason: "component_repository_replacement".to_owned(),
            mutation_token: None,
        }
    );
}

#[cfg(feature = "test-instrumentation")]
#[test]
fn complete_fixture_publication_is_ready_but_public_component_replacement_is_not() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = store_game(&storage, "steam:fixture-complete");
    let game = storage.require_game(&game_id).expect("read game");

    storage
        .store_complete_components_for_test(&game, &[])
        .expect("fixture publishes complete scan");
    assert!(matches!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        CatalogReadiness::Complete(_)
    ));

    storage
        .replace_components_for_game(&game_id, &[])
        .expect("ordinary component replacement");
    assert!(matches!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        CatalogReadiness::Invalidated { .. }
    ));
}

#[test]
fn component_replacement_deletes_game_observations_with_invalidation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = store_game(&storage, "steam:component-observations");
    complete_game_scan(&storage, &game_id);
    assert_eq!(
        storage
            .list_game_observations(&game_id)
            .expect("observations")
            .len(),
        1
    );

    storage
        .replace_components_for_game(&game_id, &[])
        .expect("component replacement");

    assert!(
        storage
            .list_game_observations(&game_id)
            .expect("observations")
            .is_empty()
    );
    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        CatalogReadiness::Invalidated {
            authority_epoch: 2,
            reason: "component_repository_replacement".to_owned(),
            mutation_token: None,
        }
    );
}

#[test]
fn component_replacement_rejects_an_active_file_mutation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = store_game(&storage, "steam:component-pending");
    storage
        .prepare_file_mutation(&PendingFileMutationRow {
            id: "tx-component-pending".to_owned(),
            game_id: game_id.clone(),
            feature: "test".to_owned(),
            subject_id: None,
            state: PendingFileMutationState::Preparing,
            manifest_json: r#"{"snapshots":[]}"#.to_owned(),
        })
        .expect("prepare mutation");

    storage
        .replace_components_for_game(&game_id, &[])
        .expect_err("component replacement is blocked while recovery is active");

    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        CatalogReadiness::NeverCompleted { authority_epoch: 0 }
    );
}

#[test]
fn complete_observation_refresh_replaces_only_the_matching_ready_epoch() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = store_game(&storage, "steam:observation-refresh");
    complete_game_scan(&storage, &game_id);

    storage
        .replace_complete_game_observations(&game_id, &[], AuthorityCas::new(0))
        .expect_err("stale authority cannot refresh observations");
    assert_eq!(
        storage
            .list_game_observations(&game_id)
            .expect("observations")
            .len(),
        1
    );

    storage
        .replace_complete_game_observations(&game_id, &[], AuthorityCas::new(1))
        .expect("current complete authority refreshes observations");
    assert!(
        storage
            .list_game_observations(&game_id)
            .expect("observations")
            .is_empty()
    );
    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        CatalogReadiness::Complete(super::CatalogReadyProjection {
            game_id,
            authority_epoch: 1,
        })
    );
}

#[test]
fn scan_publication_rolls_back_catalog_observations_and_epoch_when_observation_insert_aborts() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = store_game(&storage, "steam:observation-atomic-abort");
    complete_game_scan(&storage, &game_id);
    let before_game = storage.require_game(&game_id).expect("game");
    let before_observations = storage
        .list_game_observations(&game_id)
        .expect("observations");
    let before_readiness = storage.catalog_readiness(&game_id).expect("readiness");
    storage
        .with_connection(|connection| {
            connection
                .execute_batch(
                    "CREATE TRIGGER abort_observation_publish
                         BEFORE INSERT ON file_observations
                         BEGIN SELECT RAISE(ABORT, 'test observation abort'); END;",
                )
                .map_err(crate::error::storage_error)?;
            Ok(())
        })
        .expect("trigger");
    let changed_game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "Changed title", Launcher::Steam).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Test").expect("path"),
    );
    let fresh_observations = [observation(&game_id)];

    storage
        .save_complete_scan_write_unit(super::super::CompleteScanWriteUnit {
            game: &changed_game,
            components: &[],
            artifacts: &[],
            observations: &fresh_observations,
            authority: AuthorityCas::new(1),
            prune_empty_operations: false,
        })
        .expect_err("observation abort rolls back the entire scan publication");

    assert_eq!(storage.require_game(&game_id).expect("game"), before_game);
    assert_eq!(
        storage
            .list_game_observations(&game_id)
            .expect("observations"),
        before_observations
    );
    assert_eq!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        before_readiness
    );
}

#[test]
fn artifact_observation_batch_is_owner_scoped_and_rolls_back_as_one_transaction() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let first_id = store_artifact(
        &storage,
        "artifact:batch-first",
        "C:/Libraries/First/nvngx_dlss.dll",
    );
    let second_id = store_artifact(
        &storage,
        "artifact:batch-second",
        "C:/Libraries/Second/nvngx_dlss.dll",
    );
    let first_old =
        artifact_observation(&first_id, "C:/Libraries/First/nvngx_dlss.dll", "first-old");
    let second_old = artifact_observation(
        &second_id,
        "C:/Libraries/Second/nvngx_dlss.dll",
        "second-old",
    );
    storage
        .replace_artifact_observations(&first_id, std::slice::from_ref(&first_old))
        .expect("first old scope");
    storage
        .replace_artifact_observations(&second_id, std::slice::from_ref(&second_old))
        .expect("second old scope");

    let first_new =
        artifact_observation(&first_id, "C:/Libraries/First/nvngx_dlss.dll", "first-new");
    let mut wrong_second = artifact_observation(
        &second_id,
        "C:/Libraries/Second/nvngx_dlss.dll",
        "second-new",
    );
    wrong_second.owner = ObservationOwner::Artifact(first_id.clone());
    let invalid = HashMap::from([
        (first_id.clone(), vec![first_new.clone()]),
        (second_id.clone(), vec![wrong_second]),
    ]);
    storage
        .replace_artifact_observation_scopes(&invalid)
        .expect_err("mixed owner batch must fail closed");
    assert_eq!(
        storage
            .list_artifact_observations(&first_id)
            .expect("first preserved"),
        vec![first_old.clone()]
    );
    assert_eq!(
        storage
            .list_artifact_observations(&second_id)
            .expect("second preserved"),
        vec![second_old.clone()]
    );

    let second_new = artifact_observation(
        &second_id,
        "C:/Libraries/Second/nvngx_dlss.dll",
        "second-new",
    );
    let valid = HashMap::from([
        (first_id.clone(), vec![first_new.clone()]),
        (second_id.clone(), vec![second_new.clone()]),
    ]);
    storage
        .with_connection(|connection| {
            connection
                .execute_batch(
                    "CREATE TRIGGER abort_second_artifact_observation
                         BEFORE INSERT ON file_observations
                         WHEN NEW.artifact_id = 'artifact:batch-second'
                              AND NEW.change_token = 'second-new'
                         BEGIN SELECT RAISE(ABORT, 'test second artifact abort'); END;",
                )
                .map_err(crate::error::storage_error)?;
            Ok(())
        })
        .expect("abort trigger");
    storage
        .replace_artifact_observation_scopes(&valid)
        .expect_err("late insert failure must roll back every owner scope");
    assert_eq!(
        storage
            .list_artifact_observations(&first_id)
            .expect("first rolled back"),
        vec![first_old]
    );
    assert_eq!(
        storage
            .list_artifact_observations(&second_id)
            .expect("second rolled back"),
        vec![second_old]
    );
    storage
        .with_connection(|connection| {
            connection
                .execute_batch("DROP TRIGGER abort_second_artifact_observation")
                .map_err(crate::error::storage_error)?;
            Ok(())
        })
        .expect("drop abort trigger");
    storage
        .replace_artifact_observation_scopes(&valid)
        .expect("valid batch");
    let grouped = storage
        .list_all_artifact_observations()
        .expect("grouped observations");
    assert_eq!(grouped.get(&first_id), Some(&vec![first_new]));
    assert_eq!(grouped.get(&second_id), Some(&vec![second_new]));
}
