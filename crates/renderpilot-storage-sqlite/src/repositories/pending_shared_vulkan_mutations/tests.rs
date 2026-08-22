use super::*;

use crate::repositories::SqliteStorage;
use renderpilot_application::{GameRepository, InstalledAddonRepository, SharedArtifactRepository};
use renderpilot_domain::{
    AddonKind, GameIdentity, GameInstallation, GameRuntime, InstalledAddon, Launcher, PathRef,
    Platform, SharedArtifactKind, SharedArtifactOrigin, SharedArtifactRecord,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn game(game_id: &str) -> GameInstallation {
    let id = renderpilot_domain::GameId::new(game_id).expect("game id");
    let identity = GameIdentity::new(id, "Shared Test", Launcher::Steam).expect("identity");
    GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Shared-Test").expect("path"),
    )
}

fn shared_record() -> SharedArtifactRecord {
    SharedArtifactRecord::new(
        SharedArtifactKind::RenoDxVulkanLayer,
        PathRef::new("C:/ProgramData/ReShade").expect("path"),
        PathRef::new("C:/ProgramData/ReShade/ReShade64.json").expect("path"),
        PathRef::new("C:/ProgramData/ReShade/ReShade64.dll").expect("path"),
        SharedArtifactOrigin::RenderPilotCreated,
    )
}

fn file_backed_catalog_path(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "renderpilot-storage-shared-{label}-{}-{nonce}.db",
        std::process::id()
    ))
}

fn begin_shared(
    id: &str,
    scope: SharedVulkanMutationScope,
    game_id: Option<renderpilot_domain::GameId>,
) -> BeginSharedVulkanMutation {
    BeginSharedVulkanMutation {
        id: id.to_owned(),
        scope,
        game_id,
        feature: "shared_test".to_owned(),
        initial_manifest_json: "{}".to_owned(),
        root_capabilities_json: "{}".to_owned(),
    }
}

#[test]
fn resource_key_is_singleton() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = renderpilot_domain::GameId::new("steam:shared-test").expect("game id");
    let begin = BeginSharedVulkanMutation {
        id: "shared-tx".to_owned(),
        scope: SharedVulkanMutationScope::GameShared,
        game_id: Some(game_id),
        feature: "test".to_owned(),
        initial_manifest_json: "{}".to_owned(),
        root_capabilities_json: "{}".to_owned(),
    };
    let reservation = storage
        .try_begin_shared_vulkan_mutation(&begin)
        .expect("reserve");
    assert!(matches!(
        reservation,
        SharedVulkanMutationReservation::Reserved(_)
    ));
}

#[test]
fn root_capabilities_are_validated_and_immutable_across_preparation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let mut invalid = begin_shared("invalid-roots", SharedVulkanMutationScope::SharedOnly, None);
    invalid.root_capabilities_json = "[]".to_owned();
    assert!(storage.try_begin_shared_vulkan_mutation(&invalid).is_err());

    let mut begin = begin_shared(
        "immutable-roots",
        SharedVulkanMutationScope::SharedOnly,
        None,
    );
    begin.root_capabilities_json = r#"{"version":1,"roots":[]}"#.to_owned();
    storage
        .try_begin_shared_vulkan_mutation(&begin)
        .expect("reserve");
    storage
        .finish_preparing_shared_vulkan_mutation(
            &begin.id,
            begin.scope,
            None,
            r#"{"version":1,"prepared":true}"#,
        )
        .expect("prepare");
    let row = storage
        .pending_shared_vulkan_mutation()
        .expect("query")
        .expect("row");
    assert_eq!(row.root_capabilities_json, begin.root_capabilities_json);
}

#[test]
fn second_reservation_is_occupied_without_overwriting_the_owner() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let first = begin_shared("first", SharedVulkanMutationScope::SharedOnly, None);
    let second = begin_shared("second", SharedVulkanMutationScope::SharedOnly, None);
    assert!(matches!(
        storage
            .try_begin_shared_vulkan_mutation(&first)
            .expect("first reservation"),
        SharedVulkanMutationReservation::Reserved(_)
    ));
    let occupied = storage
        .try_begin_shared_vulkan_mutation(&second)
        .expect("occupied result");
    let SharedVulkanMutationReservation::Occupied(row) = occupied else {
        panic!("second reservation must report Occupied");
    };
    assert_eq!(row.id, "first");
    assert_eq!(
        storage
            .pending_shared_vulkan_mutation()
            .expect("query")
            .expect("row")
            .id,
        "first"
    );
}

#[test]
fn file_backed_shared_reservations_have_one_reserved_winner() {
    let path = file_backed_catalog_path("shared-shared");
    let first = SqliteStorage::open(&path).expect("first file-backed storage");
    let second = SqliteStorage::open(&path).expect("second file-backed storage");
    let first_begin = begin_shared(
        "concurrent-first",
        SharedVulkanMutationScope::SharedOnly,
        None,
    );
    let second_begin = begin_shared(
        "concurrent-second",
        SharedVulkanMutationScope::SharedOnly,
        None,
    );

    let (first_result, second_result) = std::thread::scope(|scope| {
        let first_result = scope.spawn(|| first.try_begin_shared_vulkan_mutation(&first_begin));
        let second_result = scope.spawn(|| second.try_begin_shared_vulkan_mutation(&second_begin));
        (
            first_result.join().expect("first reservation thread"),
            second_result.join().expect("second reservation thread"),
        )
    });

    let outcomes = [first_result, second_result];
    let reserved = outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            Ok(SharedVulkanMutationReservation::Reserved(row)) => Some(row.id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let occupied = outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            Ok(SharedVulkanMutationReservation::Occupied(row)) => Some(row.id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(reserved.len(), 1, "exactly one shared reservation succeeds");
    assert_eq!(occupied, reserved, "the loser observes the same winner");

    drop(first);
    drop(second);
    fs::remove_file(path).expect("remove file-backed catalog");
}

#[test]
fn file_backed_shared_and_file_reservations_have_one_success_for_the_same_game() {
    let path = file_backed_catalog_path("shared-file");
    let shared_storage = SqliteStorage::open(&path).expect("shared file-backed storage");
    let file_storage = SqliteStorage::open(&path).expect("file file-backed storage");
    let game_id = renderpilot_domain::GameId::new("steam:concurrent-shared-file").expect("game id");
    let shared_begin = begin_shared(
        "concurrent-shared-file",
        SharedVulkanMutationScope::GameShared,
        Some(game_id.clone()),
    );
    let file_begin = crate::repositories::BeginFileMutationPreparation {
        id: "concurrent-file-shared".to_owned(),
        game_id,
        feature: "concurrent_test".to_owned(),
        subject_id: None,
        initial_manifest_json: "{}".to_owned(),
    };

    let (shared_result, file_result) = std::thread::scope(|scope| {
        let shared_result =
            scope.spawn(|| shared_storage.try_begin_shared_vulkan_mutation(&shared_begin));
        let file_result = scope.spawn(|| file_storage.begin_file_mutation_preparation(&file_begin));
        (
            shared_result.join().expect("shared reservation thread"),
            file_result.join().expect("file reservation thread"),
        )
    });

    let shared_success = matches!(
        shared_result,
        Ok(SharedVulkanMutationReservation::Reserved(_))
    );
    let file_success = file_result.is_ok();
    assert_eq!(
        shared_success as u8 + file_success as u8,
        1,
        "exactly one mutation kind may reserve the same game"
    );

    drop(shared_storage);
    drop(file_storage);
    fs::remove_file(path).expect("remove file-backed catalog");
}

#[test]
fn scope_constraints_reject_invalid_owner_shapes_before_writing() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = renderpilot_domain::GameId::new("steam:scope").expect("id");
    for begin in [
        begin_shared(
            "invalid-shared-owner",
            SharedVulkanMutationScope::SharedOnly,
            Some(game_id),
        ),
        begin_shared(
            "invalid-game-owner",
            SharedVulkanMutationScope::GameShared,
            None,
        ),
    ] {
        assert!(storage.try_begin_shared_vulkan_mutation(&begin).is_err());
    }
    assert!(
        storage
            .pending_shared_vulkan_mutation()
            .expect("query")
            .is_none()
    );
}

#[test]
fn cross_kind_exclusion_rejects_both_begin_orders() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = renderpilot_domain::GameId::new("steam:cross-kind").expect("id");
    storage
        .begin_file_mutation_preparation(&crate::repositories::BeginFileMutationPreparation {
            id: "ordinary-first".to_owned(),
            game_id: game_id.clone(),
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: "{}".to_owned(),
        })
        .expect("ordinary reservation");
    assert!(
        storage
            .try_begin_shared_vulkan_mutation(&begin_shared(
                "shared-after-ordinary",
                SharedVulkanMutationScope::GameShared,
                Some(game_id),
            ))
            .is_err()
    );

    let other = renderpilot_domain::GameId::new("steam:cross-kind-other").expect("id");
    storage
        .abandon_file_mutation_preparation("ordinary-first")
        .expect("abandon ordinary");
    storage
        .try_begin_shared_vulkan_mutation(&begin_shared(
            "shared-first",
            SharedVulkanMutationScope::GameShared,
            Some(other.clone()),
        ))
        .expect("shared reservation");
    assert!(
        storage
            .begin_file_mutation_preparation(&crate::repositories::BeginFileMutationPreparation {
                id: "ordinary-after-shared".to_owned(),
                game_id: other,
                feature: "test".to_owned(),
                subject_id: None,
                initial_manifest_json: "{}".to_owned(),
            })
            .is_err()
    );

    let id_collision_storage = SqliteStorage::in_memory().expect("storage");
    let shared_only = begin_shared(
        "cross-kind-id-collision",
        SharedVulkanMutationScope::SharedOnly,
        None,
    );
    id_collision_storage
        .try_begin_shared_vulkan_mutation(&shared_only)
        .expect("shared-only reservation");
    assert!(
        id_collision_storage
            .begin_file_mutation_preparation(&crate::repositories::BeginFileMutationPreparation {
                id: shared_only.id,
                game_id: renderpilot_domain::GameId::new("steam:cross-kind-id-game")
                    .expect("game id"),
                feature: "test".to_owned(),
                subject_id: None,
                initial_manifest_json: "{}".to_owned(),
            })
            .is_err(),
        "mutation ids must remain unique across both durable mutation tables"
    );
}

#[test]
fn shared_commit_is_atomic_across_addon_and_artifact_rows() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let installation = game("steam:atomic-shared");
    let game_id = installation.id().clone();
    storage.upsert_game(&installation).expect("game");
    let begin = begin_shared(
        "atomic-shared",
        SharedVulkanMutationScope::GameShared,
        Some(game_id.clone()),
    );
    storage
        .try_begin_shared_vulkan_mutation(&begin)
        .expect("reserve");
    storage
        .finish_preparing_shared_vulkan_mutation(
            &begin.id,
            begin.scope,
            begin.game_id.as_ref(),
            r#"{"snapshots":[]}"#,
        )
        .expect("prepare");
    storage
        .with_connection(|connection| {
            connection
                .execute_batch(
                    "CREATE TRIGGER abort_shared_artifact_insert
                     BEFORE INSERT ON shared_artifacts
                     BEGIN SELECT RAISE(ABORT, 'injected shared artifact failure'); END;",
                )
                .map_err(crate::error::storage_error)
        })
        .expect("failure trigger");
    let addon = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new("C:/Games/Shared-Test/luma.addon").expect("path"),
    );
    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: super::super::game_mutations::InstalledAddonMutation::Upsert(&addon),
                shared_artifact: SharedArtifactMutation::Upsert(&shared_record()),
            })
            .is_err()
    );
    assert!(
        storage
            .get_installed_addon(&game_id)
            .expect("addon")
            .is_none()
    );
    assert!(
        storage
            .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
            .expect("artifact")
            .is_none()
    );
    assert_eq!(
        storage
            .get_pending_shared_vulkan_mutation(&begin.id)
            .expect("row")
            .expect("row")
            .state,
        PendingSharedVulkanMutationState::Prepared
    );
}

#[test]
fn prepared_resolution_fence_deletes_only_exact_row_and_keeps_catalog_invalidated() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let installation = game("steam:shared-fence");
    let game_id = installation.id().clone();
    storage.upsert_game(&installation).expect("game");
    let begin = begin_shared(
        "shared-fence",
        SharedVulkanMutationScope::GameShared,
        Some(game_id.clone()),
    );
    storage
        .try_begin_shared_vulkan_mutation(&begin)
        .expect("reserve");
    storage
        .finish_preparing_shared_vulkan_mutation(
            &begin.id,
            begin.scope,
            begin.game_id.as_ref(),
            r#"{"snapshots":[]}"#,
        )
        .expect("prepare");
    let fence = storage
        .fence_prepared_shared_vulkan_mutation_resolution(
            &begin.id,
            begin.scope,
            begin.game_id.as_ref(),
        )
        .expect("fence");
    storage
        .complete_prepared_shared_vulkan_mutation_restored(fence)
        .expect("complete restore");
    assert!(
        storage
            .get_pending_shared_vulkan_mutation(&begin.id)
            .expect("row")
            .is_none()
    );
    assert!(matches!(
        storage.catalog_readiness(&game_id).expect("readiness"),
        crate::repositories::CatalogReadiness::Invalidated {
            mutation_token: Some(token),
            ..
        } if token == begin.id
    ));
}

#[test]
fn pre_catalog_shared_owner_can_commit_only_addon_lifecycle_effects() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = renderpilot_domain::GameId::new("steam:shared-pre-catalog").expect("id");
    let begin = begin_shared(
        "shared-pre-catalog",
        SharedVulkanMutationScope::GameShared,
        Some(game_id.clone()),
    );
    storage
        .try_begin_shared_vulkan_mutation(&begin)
        .expect("reserve");
    storage
        .finish_preparing_shared_vulkan_mutation(
            &begin.id,
            begin.scope,
            begin.game_id.as_ref(),
            r#"{"snapshots":[]}"#,
        )
        .expect("prepare");
    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: super::super::game_mutations::InstalledAddonMutation::Keep,
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err()
    );
    let addon = InstalledAddon::new(
        game_id,
        AddonKind::Luma,
        PathRef::new("C:/Games/Shared-Test/luma.addon").expect("path"),
    );
    storage
        .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
            id: &begin.id,
            scope: begin.scope,
            game_id: begin.game_id.as_ref(),
            addon: super::super::game_mutations::InstalledAddonMutation::Upsert(&addon),
            shared_artifact: SharedArtifactMutation::Keep,
        })
        .expect("addon-only commit");
}

#[test]
fn committed_cleanup_is_exact() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let shared = begin_shared(
        "shared-cleanup",
        SharedVulkanMutationScope::SharedOnly,
        None,
    );
    storage
        .try_begin_shared_vulkan_mutation(&shared)
        .expect("shared reservation");
    storage
        .finish_preparing_shared_vulkan_mutation(
            &shared.id,
            shared.scope,
            None,
            r#"{"snapshots":[]}"#,
        )
        .expect("prepare shared");
    storage
        .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
            id: &shared.id,
            scope: shared.scope,
            game_id: None,
            addon: super::super::game_mutations::InstalledAddonMutation::Keep,
            shared_artifact: SharedArtifactMutation::Keep,
        })
        .expect("commit shared");
    assert!(
        storage
            .cleanup_committed_shared_vulkan_mutation("wrong")
            .is_err()
    );
    storage
        .cleanup_committed_shared_vulkan_mutation(&shared.id)
        .expect("cleanup shared");
    assert!(
        storage
            .pending_shared_vulkan_mutation()
            .expect("query")
            .is_none()
    );
}
