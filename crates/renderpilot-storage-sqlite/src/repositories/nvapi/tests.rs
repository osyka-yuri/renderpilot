use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use renderpilot_application::GameRepository;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};

use super::{
    NvapiGameSettingPreparation, NvapiSettingOperationCompletion, NvapiSettingOperationScope,
    NvapiSettingState,
};
use crate::SqliteStorage;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_storage() -> (SqliteStorage, PathBuf, GameId) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("renderpilot-nvapi-test-{nanos}-{counter}.db"));
    // Ensure clean slate.
    let _ = std::fs::remove_file(&path);
    let storage = SqliteStorage::open(&path).expect("open SqliteStorage");
    let game_id = GameId::new("manual:test-game").expect("game id");
    let identity =
        GameIdentity::new(game_id.clone(), "Test Game", Launcher::Manual).expect("identity");
    let installation = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Test").expect("install path"),
    );
    storage
        .upsert_game(&installation)
        .expect("upsert seed game");
    (storage, path, game_id)
}

#[test]
fn executable_override_roundtrip() {
    let (storage, _path, game_id) = fresh_storage();

    assert!(
        storage
            .get_nvapi_executable_override(game_id.as_str())
            .unwrap()
            .is_none()
    );

    storage
        .upsert_nvapi_executable_override(game_id.as_str(), "C:/Games/Test/Game.exe", "Game.exe")
        .unwrap();

    let row = storage
        .get_nvapi_executable_override(game_id.as_str())
        .unwrap()
        .expect("row present");
    assert_eq!(row.selected_basename, "Game.exe");
    assert_eq!(row.selected_path, "C:/Games/Test/Game.exe");
    assert!(row.updated_at > 0);
}

#[test]
fn executable_override_upsert_replaces_existing() {
    let (storage, _path, game_id) = fresh_storage();

    storage
        .upsert_nvapi_executable_override(game_id.as_str(), "C:/Games/Test/Old.exe", "Old.exe")
        .unwrap();
    storage
        .upsert_nvapi_executable_override(game_id.as_str(), "C:/Games/Test/New.exe", "New.exe")
        .unwrap();

    let row = storage
        .get_nvapi_executable_override(game_id.as_str())
        .unwrap()
        .unwrap();
    assert_eq!(row.selected_basename, "New.exe");
}

#[test]
fn delete_executable_override_removes_row() {
    let (storage, _path, game_id) = fresh_storage();

    storage
        .upsert_nvapi_executable_override(game_id.as_str(), "C:/Games/Test/Game.exe", "Game.exe")
        .unwrap();
    storage
        .delete_nvapi_executable_override(game_id.as_str())
        .unwrap();

    assert!(
        storage
            .get_nvapi_executable_override(game_id.as_str())
            .unwrap()
            .is_none()
    );
}

#[test]
fn canceling_shared_claim_rebind_preserves_original_reference_and_state() {
    let (storage, _path, first_game) = fresh_storage();
    let second_game = GameInstallation::new(
        GameIdentity::new(
            GameId::new("manual:shared-claim-b").expect("game id"),
            "Shared B",
            Launcher::Manual,
        )
        .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/SharedB").expect("root"),
    );
    storage.upsert_game(&second_game).expect("second game");
    let target = "NVIDIA Shared Profile";
    let profile_identity = r#"{"name":"NVIDIA Shared Profile"}"#;
    let setting_id = 0x10B3_292C;

    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-first",
            game_id: first_game.as_str(),
            profile_name: target,
            target_kind: "external",
            profile_is_predefined: false,
            profile_identity_json: profile_identity,
            executable_path: "C:/Games/Test/A.exe",
            application_witness_json: r#"{"app_name":"C:/Games/Test/A.exe"}"#,
            setting_id,
            original: NvapiSettingState {
                present: false,
                value: None,
            },
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            after: NvapiSettingState {
                present: true,
                value: Some(3),
            },
        })
        .expect("first claim intent");
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-first",
            scope: NvapiSettingOperationScope::Game {
                game_id: first_game.as_str(),
            },
            target_id: target,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(3),
            },
            profile_identity_json: profile_identity,
            composition_json: None,
        })
        .expect("first claim finalize");

    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-second",
            game_id: second_game.id().as_str(),
            profile_name: target,
            target_kind: "external",
            profile_is_predefined: false,
            profile_identity_json: profile_identity,
            executable_path: "C:/Games/SharedB/B.exe",
            application_witness_json: r#"{"app_name":"C:/Games/SharedB/B.exe"}"#,
            setting_id,
            original: NvapiSettingState {
                present: false,
                value: None,
            },
            before: NvapiSettingState {
                present: true,
                value: Some(3),
            },
            after: NvapiSettingState {
                present: true,
                value: Some(4),
            },
        })
        .expect("second game joins shared claim");
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-second",
            scope: NvapiSettingOperationScope::Game {
                game_id: second_game.id().as_str(),
            },
            target_id: target,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(4),
            },
            profile_identity_json: profile_identity,
            composition_json: None,
        })
        .expect("second claim finalize");

    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-retry",
            game_id: first_game.as_str(),
            profile_name: target,
            target_kind: "external",
            profile_is_predefined: false,
            profile_identity_json: profile_identity,
            executable_path: "C:/Games/Test/A-Selected.exe",
            application_witness_json: r#"{"app_name":"C:/Games/Test/A-Selected.exe"}"#,
            setting_id,
            original: NvapiSettingState {
                present: false,
                value: None,
            },
            before: NvapiSettingState {
                present: true,
                value: Some(4),
            },
            after: NvapiSettingState {
                present: true,
                value: Some(5),
            },
        })
        .expect("retry intent records a path rebind");
    storage
        .cancel_nvapi_setting_operation("op-retry")
        .expect("cancel pre-save attempt");

    let first = storage
        .list_nvapi_setting_claims_for_game(first_game.as_str())
        .expect("first game claim");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].executable_path, "C:/Games/Test/A.exe");
    assert!(!first[0].original_present);
    assert_eq!(first[0].original_value, None);
    assert_eq!(
        (first[0].expected_present, first[0].expected_value),
        (true, Some(4))
    );
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("ref count"),
        2
    );
}

#[test]
fn malformed_setting_receipt_preserves_pending_journal_and_claims() {
    let (storage, _path, game_id) = fresh_storage();
    let target = "NVIDIA Malformed Receipt Profile";
    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-malformed-receipt",
            game_id: game_id.as_str(),
            profile_name: target,
            target_kind: "external",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"NVIDIA Malformed Receipt Profile"}"#,
            executable_path: "C:/Games/Test/Game.exe",
            application_witness_json: r#"{"app_name":"C:/Games/Test/Game.exe"}"#,
            setting_id: 7,
            original: NvapiSettingState {
                present: false,
                value: None,
            },
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            after: NvapiSettingState {
                present: true,
                value: Some(3),
            },
        })
        .expect("prepare setting intent");
    storage
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE pending_drs_operations
                            SET before_json = '{\"setting_id\":7,\"present\":false}'
                          WHERE op_id = 'op-malformed-receipt'",
                    [],
                )
                .map_err(crate::error::storage_error)?;
            Ok(())
        })
        .expect("damage receipt fixture");

    assert!(
        storage
            .cancel_nvapi_setting_operation("op-malformed-receipt")
            .is_err()
    );

    let pending = storage
        .list_pending_nvapi_operations()
        .expect("pending operation remains")
        .into_iter()
        .find(|row| row.op_id == "op-malformed-receipt")
        .expect("malformed receipt stays journaled");
    assert_eq!(pending.before_json, r#"{"setting_id":7,"present":false}"#);
    assert_eq!(
        storage
            .list_nvapi_setting_claims_for_game(game_id.as_str())
            .expect("original prepared claim remains")
            .len(),
        1
    );
}
