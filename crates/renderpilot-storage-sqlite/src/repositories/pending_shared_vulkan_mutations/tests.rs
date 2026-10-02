use super::super::game_mutations::InstalledAddonMutation;
use super::*;

use crate::repositories::SqliteStorage;
use renderpilot_application::{GameRepository, InstalledAddonRepository, SharedArtifactRepository};
use renderpilot_domain::{
    AddonKind, EngineConfigJournal, EngineConfigReceipt, GameIdentity, GameInstallation,
    GameProxyTopology, GameRuntime, InstalledAddon, InstalledAddonHostKind, Launcher, PathRef,
    Platform, ProxyImplementation, ProxyLink, ProxyRootPrestate, Sha256Hash, SharedArtifactKind,
    SharedArtifactOrigin, SharedArtifactRecord,
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

fn store_active_optiscaler_topology(storage: &SqliteStorage, game_id: &renderpilot_domain::GameId) {
    let root_slot = PathRef::new("C:/Games/Shared-Test/dxgi.dll").expect("root slot");
    let topology = GameProxyTopology {
        id: format!("optiscaler:{}", game_id.as_str()),
        game_id: game_id.clone(),
        root_slot: root_slot.clone(),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: root_slot,
            receipt: renderpilot_domain::FileReceipt::owned(
                "test:optiscaler-outer",
                Sha256Hash::new("a".repeat(64)).expect("hash"),
            )
            .expect("receipt"),
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    };
    storage
        .with_transaction(|transaction| {
            super::super::proxy_topologies::upsert_within_transaction(transaction, &topology)
        })
        .expect("topology");
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

fn game_shared_replacement(
    storage: &SqliteStorage,
    game: &GameInstallation,
    id: &str,
) -> (BeginSharedVulkanMutation, InstalledAddon, InstalledAddon) {
    let expected = InstalledAddon::new(
        game.id().clone(),
        AddonKind::Luma,
        PathRef::new("D:/External/Luma.addon").expect("external payload"),
    )
    .with_host_kind(InstalledAddonHostKind::Proxy);
    storage
        .upsert_installed_addon(&expected)
        .expect("old Proxy owner");
    let expected = storage
        .get_installed_addon(game.id())
        .expect("old owner read")
        .expect("old owner persisted");
    let replacement = InstalledAddon::new(
        game.id().clone(),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Shared-Test/renodx.addon64").expect("new payload"),
    )
    .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
    let begin = begin_shared(
        id,
        SharedVulkanMutationScope::GameShared,
        Some(game.id().clone()),
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
        .expect("prepare and invalidate catalog");
    (begin, expected, replacement)
}

fn stable_engine_journal() -> EngineConfigJournal {
    EngineConfigJournal {
        stable: Some(EngineConfigReceipt {
            schema_version: 1,
            path: "C:/Users/Shared/Engine.ini".to_owned(),
            file_created: false,
            encoding: "utf8".to_owned(),
            before_digest: "0".repeat(64),
            after_digest: "1".repeat(64),
            recipe_fingerprint: "2".repeat(64),
            contributions: Vec::new(),
            created_headers: Vec::new(),
            created_header_prefixes: Vec::new(),
            created_header_groups: Vec::new(),
            created_header_ordinals: Vec::new(),
        }),
        pending: None,
    }
}

fn seed_engine_journal(
    storage: &SqliteStorage,
    game_id: &renderpilot_domain::GameId,
    journal: &EngineConfigJournal,
) {
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO game_engine_config_journals
                (game_id, addon_kind, journal_json, created_at, updated_at)
             VALUES (?1, 'luma', ?2, 1, 1)",
            rusqlite::params![
                game_id.as_str(),
                serde_json::to_string(journal).expect("journal")
            ],
        )
        .expect("seed independent Engine.ini owner");
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
        u8::from(shared_success) + u8::from(file_success),
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
                addon: InstalledAddonMutation::Upsert(&addon),
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
fn game_shared_expected_replacement_commits_proxy_release_new_owner_and_engine_independently() {
    for (label, old_kind) in [("renodx", AddonKind::RenoDx), ("luma", AddonKind::Luma)] {
        let storage = SqliteStorage::in_memory().expect("storage");
        let installation = game(&format!("steam:shared-replace-{label}"));
        let game_id = installation.id().clone();
        storage.upsert_game(&installation).expect("game");
        let expected = InstalledAddon::new(
            game_id.clone(),
            old_kind,
            PathRef::new(format!("D:/External/{label}.addon")).expect("external payload"),
        )
        .with_host_kind(InstalledAddonHostKind::Proxy);
        storage
            .upsert_installed_addon(&expected)
            .expect("old Proxy owner");
        let journal = stable_engine_journal();
        seed_engine_journal(&storage, &game_id, &journal);
        let expected = storage
            .get_installed_addon(&game_id)
            .expect("old owner lookup")
            .expect("old owner");

        let begin = begin_shared(
            &format!("replace-{label}"),
            SharedVulkanMutationScope::GameShared,
            Some(game_id.clone()),
        );
        storage
            .try_begin_shared_vulkan_mutation(&begin)
            .expect("reserve exact game owner");
        storage
            .finish_preparing_shared_vulkan_mutation(
                &begin.id,
                begin.scope,
                begin.game_id.as_ref(),
                r#"{"snapshots":[]}"#,
            )
            .expect("prepared row invalidates catalog with exact token");
        let replacement = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new("C:/Games/Shared-Test/renodx.addon64").expect("new payload"),
        )
        .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);

        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &expected,
                    replacement: &replacement,
                },
                shared_artifact: SharedArtifactMutation::Upsert(&shared_record()),
            })
            .expect("composed expected owner replacement");

        let current = storage
            .get_installed_addon(&game_id)
            .expect("new owner lookup")
            .expect("new owner committed");
        assert_eq!(current.kind(), AddonKind::RenoDx);
        assert_eq!(
            current.host_kind(),
            Some(InstalledAddonHostKind::SharedVulkanLayer)
        );
        assert_eq!(current.addon_file(), replacement.addon_file());
        assert_eq!(
            storage
                .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
                .expect("shared owner read")
                .expect("shared owner committed")
                .dll_path(),
            shared_record().dll_path()
        );
        let retained_engine = storage
            .engine_config_journal_owner(&game_id)
            .expect("Engine owner read")
            .expect("Engine owner remains independent of file row");
        assert_eq!(retained_engine.kind, AddonKind::Luma);
        assert_eq!(retained_engine.journal, journal);
        assert_eq!(
            storage
                .get_pending_shared_vulkan_mutation(&begin.id)
                .expect("prepared row lookup")
                .expect("durable row")
                .state,
            PendingSharedVulkanMutationState::Committed
        );
    }
}

#[test]
fn game_shared_expected_replacement_requires_exact_owner_token_scope_and_prepared_state() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let installation = game("steam:shared-replace-guards");
    let game_id = installation.id().clone();
    storage.upsert_game(&installation).expect("game");
    let begin = begin_shared(
        "replace-guarded",
        SharedVulkanMutationScope::GameShared,
        Some(game_id.clone()),
    );
    let expected = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new("D:/External/old.addon64").expect("old payload"),
    )
    .with_host_kind(InstalledAddonHostKind::Proxy);
    storage
        .upsert_installed_addon(&expected)
        .expect("old Proxy owner");
    let expected = storage
        .get_installed_addon(&game_id)
        .expect("old owner lookup")
        .expect("old owner");
    let replacement = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Shared-Test/renodx.addon64").expect("new payload"),
    )
    .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
    storage
        .try_begin_shared_vulkan_mutation(&begin)
        .expect("reserve");

    let preparing_commit = SharedVulkanMutationCommit {
        id: &begin.id,
        scope: begin.scope,
        game_id: begin.game_id.as_ref(),
        addon: InstalledAddonMutation::ReplaceExpected {
            expected: &expected,
            replacement: &replacement,
        },
        shared_artifact: SharedArtifactMutation::Keep,
    };
    assert!(
        storage
            .commit_shared_vulkan_mutation(preparing_commit)
            .is_err(),
        "Preparing is not sufficient authority to publish"
    );
    storage
        .finish_preparing_shared_vulkan_mutation(
            &begin.id,
            begin.scope,
            begin.game_id.as_ref(),
            r#"{"snapshots":[]}"#,
        )
        .expect("prepare");
    let other_game_id =
        renderpilot_domain::GameId::new("steam:wrong-reserved-game").expect("other game id");
    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: SharedVulkanMutationScope::SharedOnly,
                game_id: None,
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &expected,
                    replacement: &replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "a GameShared reservation cannot be committed as SharedOnly"
    );
    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: Some(&other_game_id),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &expected,
                    replacement: &replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "reservation game id must match exactly"
    );
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "UPDATE catalog_scan_authority SET mutation_token = 'wrong-token' WHERE game_id = ?1",
            [game_id.as_str()],
        )
        .expect("drift exact prepared authority token");
    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &expected,
                    replacement: &replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "prepared owner must retain the exact invalidation token"
    );
    assert_eq!(
        storage
            .get_installed_addon(&game_id)
            .expect("owner preserved"),
        Some(expected)
    );
    assert_eq!(
        storage
            .get_pending_shared_vulkan_mutation(&begin.id)
            .expect("prepared row")
            .expect("row")
            .state,
        PendingSharedVulkanMutationState::Prepared
    );
}

#[test]
fn game_shared_expected_replacement_rejects_pre_catalog_legacy_game_rows() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id =
        renderpilot_domain::GameId::new("steam:shared-replace-pre-catalog").expect("game id");
    let expected = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new("D:/External/luma.addon").expect("old payload"),
    )
    .with_host_kind(InstalledAddonHostKind::Proxy);
    storage
        .upsert_installed_addon(&expected)
        .expect("legacy owner without catalog game");
    let expected = storage
        .get_installed_addon(&game_id)
        .expect("old owner lookup")
        .expect("old owner");
    let replacement = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Shared-Test/renodx.addon64").expect("new payload"),
    )
    .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
    let begin = begin_shared(
        "replace-pre-catalog",
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
        .expect("prepared pre-catalog reservation");

    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &expected,
                    replacement: &replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "pre-catalog owner replacement has no exact catalog invalidation token"
    );
    assert_eq!(
        storage
            .get_installed_addon(&game_id)
            .expect("old owner preserved"),
        Some(expected)
    );
    assert_eq!(
        storage
            .get_pending_shared_vulkan_mutation(&begin.id)
            .expect("pending row")
            .expect("row")
            .state,
        PendingSharedVulkanMutationState::Prepared
    );
}

#[test]
fn game_shared_expected_replacement_rejects_owner_shape_ids_topology_and_peer_reservation() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let installation = game("steam:shared-replace-invalid");
    let game_id = installation.id().clone();
    storage.upsert_game(&installation).expect("game");
    let (begin, expected, replacement) =
        game_shared_replacement(&storage, &installation, "replace-invalid");

    let wrong_old_kind = InstalledAddon::new(
        game_id.clone(),
        AddonKind::OptiScaler,
        PathRef::new("D:/External/opti.dll").expect("old payload"),
    );
    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &wrong_old_kind,
                    replacement: &replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "OptiScaler cannot be replaced through file owner transition"
    );

    let wrong_old_host = expected.clone();
    // Persisted Proxy ownership is mandatory for the expected row.
    let wrong_old_host = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        wrong_old_host.addon_file().clone(),
    );
    storage
        .upsert_installed_addon(&wrong_old_host)
        .expect("change owner host classification");
    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &wrong_old_host,
                    replacement: &replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "old host kind must be Proxy"
    );

    let wrong_new_host = InstalledAddon::new(
        game_id,
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Shared-Test/new-addon.addon64").expect("new payload"),
    );
    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &wrong_old_host,
                    replacement: &wrong_new_host,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "new host kind must be SharedVulkanLayer"
    );

    let wrong_id_replacement = InstalledAddon::new(
        renderpilot_domain::GameId::new("steam:wrong-replacement-game").expect("wrong id"),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Shared-Test/new-addon.addon64").expect("new payload"),
    )
    .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &expected,
                    replacement: &wrong_id_replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "replacement game id must match reservation owner"
    );

    // Topology and aggregate reservation guards are checked for both owner
    // kinds before the row can be replaced.
    let topology_storage = SqliteStorage::in_memory().expect("topology storage");
    let topology_game = game("steam:shared-replace-topology");
    let topology_id = topology_game.id().clone();
    topology_storage.upsert_game(&topology_game).expect("game");
    let (topology_begin, topology_expected, topology_replacement) =
        game_shared_replacement(&topology_storage, &topology_game, "replace-topology");
    store_active_optiscaler_topology(&topology_storage, &topology_id);
    assert!(
        topology_storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &topology_begin.id,
                scope: topology_begin.scope,
                game_id: topology_begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &topology_expected,
                    replacement: &topology_replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "active peer topology blocks the expected owner replacement"
    );

    let reservation_storage = SqliteStorage::in_memory().expect("reservation storage");
    let reservation_game = game("steam:shared-replace-reservation");
    let reservation_id = reservation_game.id().clone();
    reservation_storage
        .upsert_game(&reservation_game)
        .expect("game");
    let (reservation_begin, reservation_expected, reservation_replacement) =
        game_shared_replacement(
            &reservation_storage,
            &reservation_game,
            "replace-reservation",
        );
    reservation_storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO peer_aggregate_reservations
                (game_id, operation_id, aggregate_kind, pending_binding, state,
                 expected_revision, created_at, updated_at)
             VALUES (?1, 'fixture-peer-owner', 'shared_peer', 'shared', 'preparing', 0, 1, 1)",
            [reservation_id.as_str()],
        )
        .expect("simulate conflicting peer reservation");
    assert!(
        reservation_storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &reservation_begin.id,
                scope: reservation_begin.scope,
                game_id: reservation_begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &reservation_expected,
                    replacement: &reservation_replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err(),
        "peer aggregate reservation blocks the replacement"
    );
}

#[test]
fn game_shared_expected_replacement_failure_rolls_back_owner_engine_shared_artifact_and_marker() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let installation = game("steam:shared-replace-rollback");
    let game_id = installation.id().clone();
    storage.upsert_game(&installation).expect("game");
    let begin = begin_shared(
        "replace-rollback",
        SharedVulkanMutationScope::GameShared,
        Some(game_id.clone()),
    );
    let expected = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new("D:/External/old-luma.addon").expect("old external payload"),
    )
    .with_host_kind(InstalledAddonHostKind::Proxy);
    storage
        .upsert_installed_addon(&expected)
        .expect("old owner");
    let journal = stable_engine_journal();
    seed_engine_journal(&storage, &game_id, &journal);
    let expected = storage
        .get_installed_addon(&game_id)
        .expect("old owner read")
        .expect("old owner");
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
        .connection
        .lock()
        .expect("connection")
        .execute_batch(
            "CREATE TRIGGER abort_replace_shared_artifact
             BEFORE INSERT ON shared_artifacts
             BEGIN SELECT RAISE(ABORT, 'fixture rejects shared artifact'); END;",
        )
        .expect("failure trigger");
    let replacement = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Shared-Test/renodx.addon64").expect("new payload"),
    )
    .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);

    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: begin.game_id.as_ref(),
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &expected,
                    replacement: &replacement,
                },
                shared_artifact: SharedArtifactMutation::Upsert(&shared_record()),
            })
            .is_err(),
        "failure after owner replacement rolls back full composition"
    );
    assert_eq!(
        storage
            .get_installed_addon(&game_id)
            .expect("old owner restored"),
        Some(expected)
    );
    assert!(
        storage
            .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
            .expect("artifact rollback")
            .is_none()
    );
    assert_eq!(
        storage
            .engine_config_journal_owner(&game_id)
            .expect("Engine owner read")
            .expect("independent owner retained")
            .journal,
        journal
    );
    assert_eq!(
        storage
            .get_pending_shared_vulkan_mutation(&begin.id)
            .expect("pending row read")
            .expect("prepared row unchanged")
            .state,
        PendingSharedVulkanMutationState::Prepared
    );
}

#[test]
fn shared_only_scope_rejects_expected_game_owner_replacement() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let installation = game("steam:shared-only-replace");
    let game_id = installation.id().clone();
    let expected = InstalledAddon::new(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new("D:/External/luma.addon").expect("old payload"),
    )
    .with_host_kind(InstalledAddonHostKind::Proxy);
    let replacement = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Shared-Test/renodx.addon64").expect("new payload"),
    )
    .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
    let begin = begin_shared(
        "shared-only-replace",
        SharedVulkanMutationScope::SharedOnly,
        None,
    );
    storage
        .try_begin_shared_vulkan_mutation(&begin)
        .expect("reserve shared-only resource");
    storage
        .finish_preparing_shared_vulkan_mutation(&begin.id, begin.scope, None, "{}")
        .expect("prepare shared-only row");

    assert!(
        storage
            .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
                id: &begin.id,
                scope: begin.scope,
                game_id: None,
                addon: InstalledAddonMutation::ReplaceExpected {
                    expected: &expected,
                    replacement: &replacement,
                },
                shared_artifact: SharedArtifactMutation::Keep,
            })
            .is_err()
    );
    assert!(
        storage
            .get_installed_addon(&game_id)
            .expect("no standalone row written")
            .is_none()
    );
    assert_eq!(
        storage
            .get_pending_shared_vulkan_mutation(&begin.id)
            .expect("pending row")
            .expect("row")
            .state,
        PendingSharedVulkanMutationState::Prepared
    );
}

#[test]
fn game_shared_peer_commit_reports_peer_topology_conflict() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let installation = game("steam:shared-optiscaler-fence");
    let game_id = installation.id().clone();
    storage.upsert_game(&installation).expect("game");
    store_active_optiscaler_topology(&storage, &game_id);
    let begin = begin_shared(
        "shared-optiscaler-fence",
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
    let addon = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Shared-Test/test.addon64").expect("path"),
    );

    let error = storage
        .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
            id: &begin.id,
            scope: begin.scope,
            game_id: begin.game_id.as_ref(),
            addon: InstalledAddonMutation::Upsert(&addon),
            shared_artifact: SharedArtifactMutation::Keep,
        })
        .expect_err("game-shared peer projection must be fenced");

    assert_eq!(
        error.kind(),
        &renderpilot_application::AppErrorKind::PeerTopologyConflict {
            peer_kind: AddonKind::RenoDx,
        }
    );
    assert!(storage.get_installed_addon(&game_id).unwrap().is_none());
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
                addon: InstalledAddonMutation::Keep,
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
            addon: InstalledAddonMutation::Upsert(&addon),
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
            addon: InstalledAddonMutation::Keep,
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
