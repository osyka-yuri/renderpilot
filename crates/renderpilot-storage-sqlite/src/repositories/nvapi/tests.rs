use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use renderpilot_application::GameRepository;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};

use rusqlite::OptionalExtension;

use super::{
    NvapiAuthorizedGlobalSettingSetPreparation, NvapiGameSettingPreparation,
    NvapiGlobalSettingPreparation, NvapiSettingOperationCompletion, NvapiSettingOperationScope,
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

const GLOBAL_PROFILE: &str = "Base Profile";
const TEST_SETTING_ID: u32 = 0x10B3_292C;

fn initial_global_identity_json(app_count: u32, setting_count: u32) -> String {
    serde_json::json!({
        "name": GLOBAL_PROFILE,
        "is_predefined": true,
        "application_count": app_count,
        "setting_count": setting_count,
    })
    .to_string()
}

fn get_target_setting_claim_row(
    storage: &SqliteStorage,
    target_id: &str,
    setting_id: u32,
) -> Option<(bool, Option<u32>, bool, Option<u32>)> {
    storage
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT original_present, original_value, expected_present, expected_value
                       FROM nvapi_target_setting_claims
                      WHERE target_id = ?1 AND setting_id = ?2",
                    rusqlite::params![target_id, setting_id],
                    |row| {
                        Ok((
                            row.get::<_, i32>(0)? != 0,
                            row.get::<_, Option<u32>>(1)?,
                            row.get::<_, i32>(2)? != 0,
                            row.get::<_, Option<u32>>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(crate::error::storage_error)
        })
        .expect("query target setting claim")
}

fn get_drs_target_row(
    storage: &SqliteStorage,
    profile_name: &str,
) -> Option<(String, bool, String)> {
    storage
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT target_id, profile_is_predefined, identity_json
                       FROM nvapi_drs_targets
                      WHERE profile_name = ?1",
                    [profile_name],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i32>(1)? != 0,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(crate::error::storage_error)
        })
        .expect("query drs target")
}

fn seed_initial_global_claim(
    storage: &SqliteStorage,
    identity_json: &str,
    setting_id: u32,
    original: NvapiSettingState,
    expected: NvapiSettingState,
) {
    storage
        .prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "op-seed-global",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: identity_json,
            setting_id,
            original,
            before: original,
            after: expected,
        })
        .expect("seed prepare global setting");

    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-seed-global",
            scope: NvapiSettingOperationScope::Global,
            target_id: GLOBAL_PROFILE,
            setting_id,
            expected,
            profile_identity_json: identity_json,
            composition_json: None,
        })
        .expect("seed finish global setting");
}

#[test]
fn characterize_global_setting_reapply_blocked_by_mutable_profile_identity_drift() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let initial_identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(
        &storage,
        &initial_identity,
        TEST_SETTING_ID,
        original,
        expected,
    );

    // Verify initial persistent state
    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(GLOBAL_PROFILE, TEST_SETTING_ID)
            .unwrap(),
        Some((false, None))
    );
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(1)))
    );

    // Next observation: counts changed from 50/120 to 55/125, but setting matches expected
    let drifted_identity = initial_global_identity_json(55, 125);
    let before = expected; // live setting matches expected
    let after = NvapiSettingState {
        present: true,
        value: Some(2),
    };

    let result = storage.prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
        op_id: "op-reapply-identity-drift",
        profile_name: GLOBAL_PROFILE,
        profile_is_predefined: true,
        profile_identity_json: &drifted_identity,
        setting_id: TEST_SETTING_ID,
        original,
        before,
        after,
    });

    // 1. Preparation rejects the operation due to identity JSON mismatch
    let err = result.expect_err("identity drift must block global setting preparation");
    assert!(
        err.to_string()
            .contains("NVIDIA global profile identity changed since its last verified state"),
        "error was: {err}"
    );

    // 2. The original claim remains unchanged
    // 3. The expected value remains unchanged
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(1))),
        "target setting claim must remain intact"
    );
    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(GLOBAL_PROFILE, TEST_SETTING_ID)
            .unwrap(),
        Some((false, None))
    );

    // 4. No new pending operation is created
    assert!(
        storage.list_pending_nvapi_operations().unwrap().is_empty(),
        "no pending operations should be journaled"
    );

    // 5. The stored baseline and target identity are preserved
    let target_row = get_drs_target_row(&storage, GLOBAL_PROFILE).expect("target exists");
    assert_eq!(
        target_row.2, initial_identity,
        "stored identity JSON must not be overwritten"
    );
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(GLOBAL_PROFILE, TEST_SETTING_ID)
            .unwrap(),
        0,
        "global setting must not create game references"
    );
}

#[test]
fn characterize_global_setting_reapply_blocked_by_driver_setting_reset() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(&storage, &identity, TEST_SETTING_ID, original, expected);

    // Next observation simulates a driver reset:
    // Identity is identical, but the setting was wiped by the driver (present = false, value = None)
    let before = NvapiSettingState {
        present: false,
        value: None,
    };
    let after = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    let result = storage.prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
        op_id: "op-reapply-setting-reset",
        profile_name: GLOBAL_PROFILE,
        profile_is_predefined: true,
        profile_identity_json: &identity,
        setting_id: TEST_SETTING_ID,
        original,
        before,
        after,
    });

    // 1. The operation is rejected by the expected-vs-live CAS check
    let err = result.expect_err("driver setting reset must trigger CAS conflict rejection");
    assert!(
        err.to_string().contains(
            "global NVIDIA setting changed outside RenderPilot; resolve the conflict before writing"
        ),
        "error was: {err}"
    );

    // 2. The original baseline remains unchanged
    // 3. The previous expected state remains unchanged
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(1))),
        "stored claim must retain original baseline and previous expected state"
    );
    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(GLOBAL_PROFILE, TEST_SETTING_ID)
            .unwrap(),
        Some((false, None))
    );

    // 4. No pending journal entry is inserted
    assert!(
        storage.list_pending_nvapi_operations().unwrap().is_empty(),
        "no pending operations should be journaled"
    );

    // 5. No game references are created for the global target
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(GLOBAL_PROFILE, TEST_SETTING_ID)
            .unwrap(),
        0,
        "global setting must not create game references"
    );
}

#[test]
fn characterize_global_setting_reapply_combined_drift_identity_guard_rejects_first() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let initial_identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(
        &storage,
        &initial_identity,
        TEST_SETTING_ID,
        original,
        expected,
    );

    // Simulate combined drift:
    // 1. Profile counts changed (50/120 -> 55/125)
    // 2. Setting value reset by driver (present = false, value = None vs expected true/Some(1))
    let drifted_identity = initial_global_identity_json(55, 125);
    let before = NvapiSettingState {
        present: false,
        value: None,
    };
    let after = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    let result = storage.prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
        op_id: "op-reapply-combined-drift",
        profile_name: GLOBAL_PROFILE,
        profile_is_predefined: true,
        profile_identity_json: &drifted_identity,
        setting_id: TEST_SETTING_ID,
        original,
        before,
        after,
    });

    // Guard ordering check: Identity guard runs at lines 480-487 before setting CAS at lines 501-507.
    let err = result.expect_err("combined drift must be rejected");
    assert!(
        err.to_string()
            .contains("NVIDIA global profile identity changed since its last verified state"),
        "identity guard must reject first; actual error: {err}"
    );
    assert!(
        !err.to_string()
            .contains("global NVIDIA setting changed outside RenderPilot"),
        "CAS guard must not be reached when identity guard triggers"
    );

    // Verify database state remains completely intact
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(1))),
        "stored claim must be untouched"
    );
    assert!(
        storage.list_pending_nvapi_operations().unwrap().is_empty(),
        "no pending operations should be journaled"
    );
}

#[test]
fn characterize_global_setting_stable_target_identity_protection() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(&storage, &identity, TEST_SETTING_ID, original, expected);

    // Case 4a: Ownership status change (profile_is_predefined drifted from true to false)
    let result_status_drift =
        storage.prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "op-ownership-drift",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: false, // stored is true
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            original,
            before: expected,
            after: NvapiSettingState {
                present: true,
                value: Some(2),
            },
        });
    let err = result_status_drift.expect_err("predefined status drift must be rejected");
    assert!(
        err.to_string()
            .contains("NVIDIA global profile identity changed since its last verified state"),
        "error was: {err}"
    );

    // Stored target still has profile_is_predefined = true
    let target_row = get_drs_target_row(&storage, GLOBAL_PROFILE).expect("target exists");
    assert!(
        target_row.1,
        "profile_is_predefined must remain true in database"
    );

    // Case 4b: Different profile name targeting global operation
    // If an operation specifies a different profile name, the storage engine keys by profile_name
    let other_profile = "Other Global Profile";
    let other_identity = serde_json::json!({
        "name": other_profile,
        "is_predefined": true,
        "application_count": 10,
        "setting_count": 20,
    })
    .to_string();

    let result_other =
        storage.prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "op-other-profile",
            profile_name: other_profile,
            profile_is_predefined: true,
            profile_identity_json: &other_identity,
            setting_id: TEST_SETTING_ID,
            original,
            before: original,
            after: expected,
        });
    // This succeeds as a separate target, but must NOT overwrite or alias "Base Profile"
    assert!(result_other.is_ok());

    // "Base Profile" target and setting claim remain completely independent and unchanged
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(1))),
        "Base Profile claim must remain intact"
    );
    let base_target = get_drs_target_row(&storage, GLOBAL_PROFILE).expect("base target");
    assert_eq!(base_target.0, GLOBAL_PROFILE);
    assert_eq!(base_target.2, identity);

    let other_target = get_drs_target_row(&storage, other_profile).expect("other target");
    assert_eq!(other_target.0, other_profile);
}

#[test]
fn characterize_global_setting_distinguishes_explicit_zero_from_absent() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    // RenderPilot explicitly applied 0 (e.g., Anisotropic Filtering Off)
    let explicit_zero = NvapiSettingState {
        present: true,
        value: Some(0),
    };

    seed_initial_global_claim(
        &storage,
        &identity,
        TEST_SETTING_ID,
        original,
        explicit_zero,
    );

    // Verify stored state is (expected_present = true, expected_value = Some(0))
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(0))),
        "stored claim must record explicit zero override"
    );

    // Driver reset occurs: the setting is now absent/inherited: (present = false, value = None).
    // Note: effective driver value might be 0 by default, but explicit override is absent.
    let absent_setting = NvapiSettingState {
        present: false,
        value: None,
    };
    let new_value = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    let result = storage.prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
        op_id: "op-reapply-zero-vs-absent",
        profile_name: GLOBAL_PROFILE,
        profile_is_predefined: true,
        profile_identity_json: &identity,
        setting_id: TEST_SETTING_ID,
        original,
        before: absent_setting,
        after: new_value,
    });

    // Storage preparation detects (present = false, value = None) != (present = true, value = Some(0))
    let err = result.expect_err("absent setting must not match explicit zero override");
    assert!(
        err.to_string().contains(
            "global NVIDIA setting changed outside RenderPilot; resolve the conflict before writing"
        ),
        "error was: {err}"
    );

    // Stored claim retains explicit zero
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(0)))
    );
}

#[test]
fn authorized_global_set_reapply_succeeds_after_driver_setting_reset() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(&storage, &identity, TEST_SETTING_ID, original, expected);

    // Live driver setting has reset: present = false, value = None.
    let before = NvapiSettingState {
        present: false,
        value: None,
    };

    let result = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-auth-reset",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before,
            value: 2,
        },
    );
    assert!(
        result.is_ok(),
        "authorized set preparation must succeed after driver reset: {result:?}"
    );

    // Before finalization:
    // 1. Original baseline remains intact
    // 2. Expected value remains unchanged (Some(1)) until verified completion
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(1))),
        "stored claim expected value must remain unchanged prior to finalization"
    );

    // 3. Durable pending operation is recorded with observed before-state and explicit after-state
    let pending = storage
        .list_pending_nvapi_operations()
        .expect("pending ops");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-auth-reset");
    assert_eq!(pending[0].phase, "intent");
    assert_eq!(pending[0].kind, "setting");

    let before_receipt: serde_json::Value =
        serde_json::from_str(&pending[0].before_json).expect("before json");
    assert_eq!(before_receipt["present"], false);
    assert_eq!(before_receipt["value"], serde_json::Value::Null);
    assert_eq!(before_receipt["claim_was_new"], false);

    let after_receipt: serde_json::Value =
        serde_json::from_str(&pending[0].after_json).expect("after json");
    assert_eq!(after_receipt["present"], true);
    assert_eq!(after_receipt["value"], 2);

    // 4. Finalize the operation via existing method
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-auth-reset",
            scope: NvapiSettingOperationScope::Global,
            target_id: GLOBAL_PROFILE,
            setting_id: TEST_SETTING_ID,
            expected: NvapiSettingState {
                present: true,
                value: Some(2),
            },
            profile_identity_json: &identity,
            composition_json: None,
        })
        .expect("finish setting operation");

    // After finalization:
    // - Expected value updated to Some(2)
    // - Original baseline preserved as (false, None)
    // - Pending operation cleared
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(2)))
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending ops")
            .is_empty()
    );
}

#[test]
fn authorized_global_set_reapply_succeeds_with_mutable_count_drift() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let initial_identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(
        &storage,
        &initial_identity,
        TEST_SETTING_ID,
        original,
        expected,
    );

    // Drift counts: (50, 120) -> (55, 125)
    let drifted_identity = initial_global_identity_json(55, 125);
    let before = expected;

    let result = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-auth-drift",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &drifted_identity,
            setting_id: TEST_SETTING_ID,
            before,
            value: 2,
        },
    );
    assert!(
        result.is_ok(),
        "authorized set preparation must permit count drift: {result:?}"
    );

    // Stored target identity in SQLite is NOT replaced prior to finalization
    let target_row_before = get_drs_target_row(&storage, GLOBAL_PROFILE).expect("target exists");
    assert_eq!(
        target_row_before.2, initial_identity,
        "persisted identity JSON must remain intact until finalization"
    );

    // Finalize with drifted identity
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-auth-drift",
            scope: NvapiSettingOperationScope::Global,
            target_id: GLOBAL_PROFILE,
            setting_id: TEST_SETTING_ID,
            expected: NvapiSettingState {
                present: true,
                value: Some(2),
            },
            profile_identity_json: &drifted_identity,
            composition_json: None,
        })
        .expect("finish setting operation");

    // After finalization: target identity updated
    let target_row_after = get_drs_target_row(&storage, GLOBAL_PROFILE).expect("target exists");
    assert_eq!(
        target_row_after.2, drifted_identity,
        "persisted identity JSON must be updated upon finalization"
    );
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(2)))
    );
}

#[test]
fn authorized_global_set_reapply_succeeds_with_combined_drift_and_setting_reset() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let initial_identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(
        &storage,
        &initial_identity,
        TEST_SETTING_ID,
        original,
        expected,
    );

    // Both counts drifted AND driver setting reset
    let drifted_identity = initial_global_identity_json(55, 125);
    let before = NvapiSettingState {
        present: false,
        value: None,
    };

    let result = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-auth-combined",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &drifted_identity,
            setting_id: TEST_SETTING_ID,
            before,
            value: 2,
        },
    );
    assert!(
        result.is_ok(),
        "authorized set preparation must permit combined drift and reset: {result:?}"
    );

    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-auth-combined",
            scope: NvapiSettingOperationScope::Global,
            target_id: GLOBAL_PROFILE,
            setting_id: TEST_SETTING_ID,
            expected: NvapiSettingState {
                present: true,
                value: Some(2),
            },
            profile_identity_json: &drifted_identity,
            composition_json: None,
        })
        .expect("finish setting operation");

    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(2)))
    );
}

#[test]
fn authorized_global_set_reapply_preserves_historical_original_baseline_across_lifecycle() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: true,
        value: Some(99),
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(&storage, &identity, TEST_SETTING_ID, original, expected);

    // Baseline before prep
    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(GLOBAL_PROFILE, TEST_SETTING_ID)
            .unwrap(),
        Some((true, Some(99)))
    );

    // Prep authorized set
    storage
        .prepare_nvapi_authorized_global_setting_set(NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-auth-lifecycle",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        })
        .expect("prep authorized set");

    // Baseline after prep
    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(GLOBAL_PROFILE, TEST_SETTING_ID)
            .unwrap(),
        Some((true, Some(99)))
    );

    // Cancel operation
    storage
        .cancel_nvapi_setting_operation("op-auth-lifecycle")
        .expect("cancel operation");

    // Baseline and claim after cancellation
    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(GLOBAL_PROFILE, TEST_SETTING_ID)
            .unwrap(),
        Some((true, Some(99)))
    );
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((true, Some(99), true, Some(1))),
        "claim and prior expected value must survive cancellation"
    );

    // Prep again and finalize
    storage
        .prepare_nvapi_authorized_global_setting_set(NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-auth-lifecycle-2",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 3,
        })
        .expect("prep authorized set 2");

    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-auth-lifecycle-2",
            scope: NvapiSettingOperationScope::Global,
            target_id: GLOBAL_PROFILE,
            setting_id: TEST_SETTING_ID,
            expected: NvapiSettingState {
                present: true,
                value: Some(3),
            },
            profile_identity_json: &identity,
            composition_json: None,
        })
        .expect("finish operation");

    // Baseline after finalization
    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(GLOBAL_PROFILE, TEST_SETTING_ID)
            .unwrap(),
        Some((true, Some(99)))
    );
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((true, Some(99), true, Some(3)))
    );
}

#[test]
fn authorized_global_set_reapply_explicit_zero_remains_distinct_from_absent() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(&storage, &identity, TEST_SETTING_ID, original, expected);

    // Prepare Set(0)
    storage
        .prepare_nvapi_authorized_global_setting_set(NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-auth-zero",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 0,
        })
        .expect("prep authorized set 0");

    let pending = storage
        .list_pending_nvapi_operations()
        .expect("pending ops");
    let after_receipt: serde_json::Value =
        serde_json::from_str(&pending[0].after_json).expect("after json");
    assert_eq!(after_receipt["present"], true);
    assert_eq!(after_receipt["value"], 0);

    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-auth-zero",
            scope: NvapiSettingOperationScope::Global,
            target_id: GLOBAL_PROFILE,
            setting_id: TEST_SETTING_ID,
            expected: NvapiSettingState {
                present: true,
                value: Some(0),
            },
            profile_identity_json: &identity,
            composition_json: None,
        })
        .expect("finish set 0");

    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        Some((false, None, true, Some(0)))
    );
}

#[test]
fn strict_global_preparation_continues_to_reject_stale_cas_and_drift() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let initial_identity = initial_global_identity_json(50, 120);
    let original = NvapiSettingState {
        present: false,
        value: None,
    };
    let expected = NvapiSettingState {
        present: true,
        value: Some(1),
    };

    seed_initial_global_claim(
        &storage,
        &initial_identity,
        TEST_SETTING_ID,
        original,
        expected,
    );

    // Strict method continues to reject identity drift:
    let drifted_identity = initial_global_identity_json(55, 125);
    let strict_drift_res =
        storage.prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "op-strict-drift",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &drifted_identity,
            setting_id: TEST_SETTING_ID,
            original,
            before: expected,
            after: NvapiSettingState {
                present: true,
                value: Some(2),
            },
        });
    assert!(strict_drift_res.is_err());
    assert!(
        strict_drift_res
            .unwrap_err()
            .to_string()
            .contains("identity changed")
    );

    // Strict method continues to reject stale CAS after reset:
    let strict_cas_res =
        storage.prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "op-strict-cas",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &initial_identity,
            setting_id: TEST_SETTING_ID,
            original,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            after: NvapiSettingState {
                present: true,
                value: Some(2),
            },
        });
    assert!(strict_cas_res.is_err());
    assert!(
        strict_cas_res
            .unwrap_err()
            .to_string()
            .contains("resolve the conflict before writing")
    );
}

#[test]
fn authorized_global_set_reapply_rejects_changed_stable_identity_or_predefined_status() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);
    seed_initial_global_claim(
        &storage,
        &identity,
        TEST_SETTING_ID,
        NvapiSettingState {
            present: false,
            value: None,
        },
        NvapiSettingState {
            present: true,
            value: Some(1),
        },
    );

    // 1. Mismatched profile name in identity JSON
    let mismatched_json = serde_json::json!({
        "name": "Different Name",
        "is_predefined": true,
        "application_count": 50,
        "setting_count": 120,
    })
    .to_string();
    let err_json_name = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-bad-json-name",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &mismatched_json,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        },
    );
    assert!(
        err_json_name.is_err(),
        "must reject mismatched profile name in identity JSON"
    );

    // 2. Profile is_predefined = false in preparation
    let err_not_predef = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-not-predef",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: false,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        },
    );
    assert!(err_not_predef.is_err(), "global profile must be predefined");

    // 3. Profile identity JSON has is_predefined = false
    let not_predef_json = serde_json::json!({
        "name": GLOBAL_PROFILE,
        "is_predefined": false,
        "application_count": 50,
        "setting_count": 120,
    })
    .to_string();
    let err_json_predef = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-not-predef-json",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &not_predef_json,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        },
    );
    assert!(
        err_json_predef.is_err(),
        "must reject identity JSON with is_predefined = false"
    );
}

#[test]
fn authorized_global_set_reapply_rejects_malformed_or_inconsistent_identity_json() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);
    seed_initial_global_claim(
        &storage,
        &identity,
        TEST_SETTING_ID,
        NvapiSettingState {
            present: false,
            value: None,
        },
        NvapiSettingState {
            present: true,
            value: Some(1),
        },
    );

    // 1. Malformed JSON
    let res_malformed = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-malformed",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: "not valid json {",
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        },
    );
    assert!(res_malformed.is_err(), "malformed json must be rejected");

    // 2. Missing required count fields
    let missing_fields = serde_json::json!({
        "name": GLOBAL_PROFILE,
        "is_predefined": true
    })
    .to_string();
    let res_missing = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-missing",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &missing_fields,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        },
    );
    assert!(res_missing.is_err(), "missing fields must be rejected");

    // 3. Unknown extra field
    let extra_field = serde_json::json!({
        "name": GLOBAL_PROFILE,
        "is_predefined": true,
        "application_count": 50,
        "setting_count": 120,
        "unexpected_key": "injected"
    })
    .to_string();
    let res_extra = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-extra",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &extra_field,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        },
    );
    assert!(res_extra.is_err(), "extra unknown fields must be rejected");

    // 4. Semantically equivalent JSON with different key ordering and whitespace is ACCEPTED
    let reordered_json = "{\n  \"setting_count\": 120,\n  \"name\": \"Base Profile\",\n  \"is_predefined\": true,\n  \"application_count\": 50\n}";
    let res_reordered = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-reordered",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: reordered_json,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        },
    );
    assert!(
        res_reordered.is_ok(),
        "semantically equivalent JSON must be accepted regardless of key order: {res_reordered:?}"
    );
}

#[test]
fn authorized_global_set_reapply_rejects_game_associated_targets() {
    let (storage, _, game_id) = fresh_storage();
    let target_name = "GameTarget";
    let identity = serde_json::json!({
        "name": target_name,
        "is_predefined": true,
        "application_count": 1,
        "setting_count": 1,
    })
    .to_string();

    // Case A: Target has a game claim reference
    let game_claim_prep = NvapiGameSettingPreparation {
        op_id: "op-game-prep",
        game_id: game_id.as_str(),
        profile_name: target_name,
        target_kind: "external",
        profile_is_predefined: true,
        profile_identity_json: &identity,
        executable_path: "C:/Games/Test/game.exe",
        application_witness_json: r#"{"app_name":"game.exe"}"#,
        setting_id: TEST_SETTING_ID,
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
            value: Some(1),
        },
    };
    storage
        .prepare_nvapi_setting_operation(game_claim_prep)
        .expect("prep game claim");

    let res_game_associated = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-auth-game-target",
            profile_name: target_name,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: 0x9999,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 42,
        },
    );
    assert!(
        res_game_associated.is_err(),
        "target with game claim references must be rejected"
    );

    // Case B: Target has an application witness
    let witness_target = "WitnessTarget";
    let witness_identity = serde_json::json!({
        "name": witness_target,
        "is_predefined": true,
        "application_count": 1,
        "setting_count": 0,
    })
    .to_string();
    storage
        .with_connection(|conn| {
            conn.execute(
                "INSERT INTO nvapi_drs_targets (target_id, profile_name, profile_is_predefined, kind, identity_json)
             VALUES (?1, ?1, 1, 'external', ?2)",
                rusqlite::params![witness_target, witness_identity],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO nvapi_drs_target_app_witnesses (target_id, executable_path, application_json)
             VALUES (?1, 'C:/Games/witness.exe', '{\"app_name\":\"witness.exe\"}')",
                rusqlite::params![witness_target],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();

    let res_witness = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-auth-witness-target",
            profile_name: witness_target,
            profile_is_predefined: true,
            profile_identity_json: &witness_identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 42,
        },
    );
    assert!(
        res_witness.is_err(),
        "target with application witnesses must be rejected"
    );

    // Case C: Target is an owned profile (kind = 'renderpilot')
    let owned_target = "OwnedTarget";
    let owned_identity = serde_json::json!({
        "name": owned_target,
        "is_predefined": true,
        "application_count": 0,
        "setting_count": 0,
    })
    .to_string();
    storage
        .with_connection(|conn| {
            conn.execute(
                "INSERT INTO nvapi_drs_targets (target_id, profile_name, profile_is_predefined, kind, identity_json)
             VALUES (?1, ?1, 1, 'renderpilot', ?2)",
                rusqlite::params![owned_target, owned_identity],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO nvapi_owned_profiles (game_id, profile_name, binding_path, application_witness_json, composition_json)
             VALUES (?1, ?2, 'C:/Games/owned.exe', '{\"app_name\":\"owned.exe\"}', '{}')",
                rusqlite::params![game_id.as_str(), owned_target],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();

    let res_owned = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-auth-owned-target",
            profile_name: owned_target,
            profile_is_predefined: true,
            profile_identity_json: &owned_identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 42,
        },
    );
    assert!(
        res_owned.is_err(),
        "target owned by game profile must be rejected"
    );
}

#[test]
fn authorized_global_set_reapply_rejects_unresolved_pending_intent_or_conflict() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);

    // 1. Prepare an operation, leaving phase = 'intent'
    storage
        .prepare_nvapi_authorized_global_setting_set(NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-pending-1",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 1,
        })
        .expect("prep first operation");

    // Second prep on same target while intent is pending
    let res_intent = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-pending-2",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        },
    );
    assert!(
        res_intent.is_err(),
        "unresolved pending intent must block overlapping preparation"
    );

    // 2. Mark as conflict
    storage
        .mark_nvapi_operation_conflict("op-pending-1", "{\"observed\":\"conflict\"}")
        .expect("mark conflict");

    let res_conflict = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-pending-3",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &identity,
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 3,
        },
    );
    assert!(
        res_conflict.is_err(),
        "unresolved conflict must block overlapping preparation"
    );
}

#[test]
fn authorized_global_set_reapply_failure_rolls_back_without_partial_changes() {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let identity = initial_global_identity_json(50, 120);
    seed_initial_global_claim(
        &storage,
        &identity,
        TEST_SETTING_ID,
        NvapiSettingState {
            present: false,
            value: None,
        },
        NvapiSettingState {
            present: true,
            value: Some(1),
        },
    );

    let claim_before = get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID);
    let target_before = get_drs_target_row(&storage, GLOBAL_PROFILE);
    assert!(storage.list_pending_nvapi_operations().unwrap().is_empty());

    // Trigger failure with invalid JSON
    let res_fail = storage.prepare_nvapi_authorized_global_setting_set(
        NvapiAuthorizedGlobalSettingSetPreparation {
            op_id: "op-should-fail",
            profile_name: GLOBAL_PROFILE,
            profile_is_predefined: true,
            profile_identity_json: "invalid-json",
            setting_id: TEST_SETTING_ID,
            before: NvapiSettingState {
                present: false,
                value: None,
            },
            value: 2,
        },
    );
    assert!(res_fail.is_err());

    // Assert zero partial changes
    assert!(
        storage.list_pending_nvapi_operations().unwrap().is_empty(),
        "failed preparation must not leave pending operations"
    );
    assert_eq!(
        get_target_setting_claim_row(&storage, GLOBAL_PROFILE, TEST_SETTING_ID),
        claim_before,
        "failed preparation must not alter target setting claims"
    );
    assert_eq!(
        get_drs_target_row(&storage, GLOBAL_PROFILE),
        target_before,
        "failed preparation must not alter drs targets"
    );
}

#[test]
fn release_final_nvapi_setting_claim_removes_claim_and_target_records() {
    let (storage, _path, game_a) = fresh_storage();
    let game_b = GameInstallation::new(
        GameIdentity::new(
            GameId::new("manual:final-ref-game-b").expect("game id"),
            "Game B",
            Launcher::Manual,
        )
        .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/FinalRefB").expect("root"),
    );
    storage.upsert_game(&game_b).expect("upsert b");

    let target_a = "NVIDIA Target A";
    let target_b = "NVIDIA Target B";
    let setting_id = 0x10B3_292C;
    let identity_a = r#"{"name":"NVIDIA Target A"}"#;
    let identity_b = r#"{"name":"NVIDIA Target B"}"#;

    // Seed Game A claim on Target A (exclusive claim)
    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-prep-a",
            game_id: game_a.as_str(),
            profile_name: target_a,
            target_kind: "external",
            profile_is_predefined: true,
            profile_identity_json: identity_a,
            executable_path: "C:/Games/Test/A.exe",
            application_witness_json: r#"{"app_name":"C:/Games/Test/A.exe"}"#,
            setting_id,
            original: NvapiSettingState {
                present: true,
                value: Some(10),
            },
            before: NvapiSettingState {
                present: true,
                value: Some(10),
            },
            after: NvapiSettingState {
                present: true,
                value: Some(20),
            },
        })
        .expect("prep claim a");
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-prep-a",
            scope: NvapiSettingOperationScope::Game {
                game_id: game_a.as_str(),
            },
            target_id: target_a,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(20),
            },
            profile_identity_json: identity_a,
            composition_json: None,
        })
        .expect("finish claim a");

    // Seed Game B claim on Target B (unrelated claim to verify non-interference)
    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-prep-b",
            game_id: game_b.id().as_str(),
            profile_name: target_b,
            target_kind: "external",
            profile_is_predefined: true,
            profile_identity_json: identity_b,
            executable_path: "C:/Games/FinalRefB/B.exe",
            application_witness_json: r#"{"app_name":"C:/Games/FinalRefB/B.exe"}"#,
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
                value: Some(30),
            },
        })
        .expect("prep claim b");
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-prep-b",
            scope: NvapiSettingOperationScope::Game {
                game_id: game_b.id().as_str(),
            },
            target_id: target_b,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(30),
            },
            profile_identity_json: identity_b,
            composition_json: None,
        })
        .expect("finish claim b");

    // Verify initial state for Game A
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(target_a, setting_id)
            .expect("count refs a"),
        1
    );
    assert_eq!(
        get_target_setting_claim_row(&storage, target_a, setting_id),
        Some((true, Some(10), true, Some(20)))
    );
    assert!(get_drs_target_row(&storage, target_a).is_some());
    assert!(
        storage
            .get_nvapi_application_witness(target_a, "C:/Games/Test/A.exe")
            .expect("witness a")
            .is_some()
    );

    // Call release_nvapi_setting_claim on the final reference for Game A
    storage
        .release_nvapi_setting_claim(game_a.as_str(), target_a, setting_id)
        .expect("release final reference");

    // 1. Deletes Game A's reference
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(target_a, setting_id)
            .expect("count refs a after"),
        0
    );

    // 2. Deletes the underlying setting claim because no references remain
    // 4. Permanently removes original_* and expected_* with the claim row deletion
    assert_eq!(
        get_target_setting_claim_row(&storage, target_a, setting_id),
        None,
        "underlying claim row and historical baselines must be permanently deleted"
    );

    // 3. Removes associated target metadata because target was external with no other claims/owned profiles
    assert_eq!(
        get_drs_target_row(&storage, target_a),
        None,
        "external target row must be deleted when no claims or owned profiles remain"
    );
    assert_eq!(
        storage
            .get_nvapi_application_witness(target_a, "C:/Games/Test/A.exe")
            .expect("witness a after"),
        None,
        "target app witness must be deleted"
    );

    // 5. Does not modify unrelated game B, its claim, or target B
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(target_b, setting_id)
            .expect("count refs b"),
        1
    );
    assert_eq!(
        get_target_setting_claim_row(&storage, target_b, setting_id),
        Some((false, None, true, Some(30))),
        "unrelated game B claim must remain unchanged"
    );
    assert!(
        get_drs_target_row(&storage, target_b).is_some(),
        "unrelated target B must remain"
    );
    assert!(
        storage
            .get_nvapi_application_witness(target_b, "C:/Games/FinalRefB/B.exe")
            .expect("witness b")
            .is_some(),
        "unrelated witness B must remain"
    );
}

#[derive(Clone, Copy)]
struct OwnedTargetFixtureParams<'a> {
    game_id: &'a str,
    target_id: &'a str,
    kind: Option<&'a str>,
    profile_is_predefined: i32,
    owned_state: Option<&'a str>,
    with_claim: bool,
    op_id: &'a str,
}

fn setup_owned_target_fixture(storage: &SqliteStorage, params: OwnedTargetFixtureParams<'_>) {
    let OwnedTargetFixtureParams {
        game_id,
        target_id,
        kind,
        profile_is_predefined,
        owned_state,
        with_claim,
        op_id,
    } = params;
    let binding_path = "C:/Games/Test/Game.exe";
    let witness_json = r#"{"app_name":"C:/Games/Test/Game.exe"}"#;
    let identity_json = r#"{"name":"test","is_predefined":false}"#;
    let composition_json = r#"{"settings":[]}"#;

    storage
        .with_connection(|connection| {
            if let Some(target_kind) = kind {
                connection
                    .execute(
                        "INSERT INTO nvapi_drs_targets
                            (target_id, profile_name, profile_is_predefined, kind, identity_json)
                         VALUES (?1, ?1, ?2, ?3, ?4)",
                        rusqlite::params![target_id, profile_is_predefined, target_kind, identity_json],
                    )
                    .map_err(crate::error::storage_error)?;
                connection
                    .execute(
                        "INSERT INTO nvapi_drs_target_app_witnesses
                            (target_id, executable_path, application_json)
                         VALUES (?1, ?2, ?3)",
                        rusqlite::params![target_id, binding_path, witness_json],
                    )
                    .map_err(crate::error::storage_error)?;

                if with_claim {
                    let setting_id = 0x10f9dc83;
                    connection
                        .execute(
                            "INSERT INTO nvapi_target_setting_claims
                                (target_id, setting_id, original_present, original_value, expected_present, expected_value)
                             VALUES (?1, ?2, 0, NULL, 1, 1)",
                            rusqlite::params![target_id, setting_id],
                        )
                        .map_err(crate::error::storage_error)?;
                    connection
                        .execute(
                            "INSERT INTO nvapi_game_claim_refs
                                (game_id, target_id, setting_id, executable_path)
                             VALUES (?1, ?2, ?3, ?4)",
                            rusqlite::params![game_id, target_id, setting_id, binding_path],
                        )
                        .map_err(crate::error::storage_error)?;
                }
            }

            if let Some(state) = owned_state {
                connection
                    .execute(
                        "INSERT INTO nvapi_owned_profiles
                            (game_id, profile_name, binding_path, application_witness_json, composition_json, state)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![game_id, target_id, binding_path, witness_json, composition_json, state],
                    )
                    .map_err(crate::error::storage_error)?;
            }
            Ok(())
        })
        .expect("setup fixture");

    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(target_id),
            "delete_profile",
            r#"{"binding_path":"C:/Games/Test/Game.exe"}"#,
            r#"{"profile_absent":true}"#,
        )
        .expect("begin pending delete_profile operation");
}

#[test]
fn complete_nvapi_profile_deletion_succeeds_for_valid_renderpilot_target() {
    let (storage, _path, game_id) = fresh_storage();
    let target_id = "RP-Target-Valid";
    let op_id = "op-del-valid";
    setup_owned_target_fixture(
        &storage,
        OwnedTargetFixtureParams {
            game_id: game_id.as_str(),
            target_id,
            kind: Some("renderpilot"),
            profile_is_predefined: 0,
            owned_state: Some("active"),
            with_claim: true,
            op_id,
        },
    );

    let result = storage.complete_nvapi_profile_deletion(op_id, game_id.as_str(), target_id);
    assert!(
        result.is_ok(),
        "valid renderpilot profile deletion must succeed: {result:?}"
    );

    assert!(
        storage
            .get_nvapi_owned_profile(game_id.as_str())
            .expect("query")
            .is_none()
    );
    assert!(get_drs_target_row(&storage, target_id).is_none());
    assert!(get_target_setting_claim_row(&storage, target_id, 0x10f9dc83).is_none());
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(target_id, 0x10f9dc83)
            .expect("refs"),
        0
    );
    assert!(
        storage
            .get_nvapi_application_witness(target_id, "C:/Games/Test/Game.exe")
            .expect("witness")
            .is_none()
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty()
    );
}

#[test]
fn complete_nvapi_profile_deletion_rejects_external_target() {
    let (storage, _path, game_id) = fresh_storage();
    let target_id = "RP-Target-External";
    let op_id = "op-del-external";
    setup_owned_target_fixture(
        &storage,
        OwnedTargetFixtureParams {
            game_id: game_id.as_str(),
            target_id,
            kind: Some("external"),
            profile_is_predefined: 0,
            owned_state: Some("active"),
            with_claim: true,
            op_id,
        },
    );

    let result = storage.complete_nvapi_profile_deletion(op_id, game_id.as_str(), target_id);
    assert!(result.is_err(), "must reject external target");

    // All records must be preserved
    assert!(
        storage
            .get_nvapi_owned_profile(game_id.as_str())
            .expect("query")
            .is_some(),
        "owned receipt must remain"
    );
    assert!(
        get_drs_target_row(&storage, target_id).is_some(),
        "target row must remain"
    );
    assert!(
        get_target_setting_claim_row(&storage, target_id, 0x10f9dc83).is_some(),
        "setting claim must remain"
    );
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(target_id, 0x10f9dc83)
            .expect("refs"),
        1,
        "claim ref must remain"
    );
    assert!(
        storage
            .get_nvapi_application_witness(target_id, "C:/Games/Test/Game.exe")
            .expect("witness")
            .is_some(),
        "witness must remain"
    );
    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1, "pending operation must be preserved");
}

#[test]
fn complete_nvapi_profile_deletion_rejects_missing_target() {
    let (storage, _path, game_id) = fresh_storage();
    let target_id = "RP-Target-Missing";
    let op_id = "op-del-missing";
    setup_owned_target_fixture(
        &storage,
        OwnedTargetFixtureParams {
            game_id: game_id.as_str(),
            target_id,
            kind: None,
            profile_is_predefined: 0,
            owned_state: Some("active"),
            with_claim: false,
            op_id,
        },
    );

    let result = storage.complete_nvapi_profile_deletion(op_id, game_id.as_str(), target_id);
    assert!(result.is_err(), "must reject missing target metadata");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id.as_str())
            .expect("query")
            .is_some(),
        "owned receipt must remain"
    );
    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1, "pending operation must be preserved");
}

#[test]
fn complete_nvapi_profile_deletion_rejects_predefined_target() {
    let (storage, _path, game_id) = fresh_storage();
    let target_id = "RP-Target-Predefined";
    let op_id = "op-del-predefined";
    setup_owned_target_fixture(
        &storage,
        OwnedTargetFixtureParams {
            game_id: game_id.as_str(),
            target_id,
            kind: Some("renderpilot"),
            profile_is_predefined: 1,
            owned_state: Some("active"),
            with_claim: true,
            op_id,
        },
    );

    let result = storage.complete_nvapi_profile_deletion(op_id, game_id.as_str(), target_id);
    assert!(result.is_err(), "must reject predefined target");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id.as_str())
            .expect("query")
            .is_some(),
        "owned receipt must remain"
    );
    assert!(
        get_drs_target_row(&storage, target_id).is_some(),
        "target row must remain"
    );
    assert!(
        get_target_setting_claim_row(&storage, target_id, 0x10f9dc83).is_some(),
        "setting claim must remain"
    );
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(target_id, 0x10f9dc83)
            .expect("refs"),
        1,
        "claim ref must remain"
    );
    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1, "pending operation must be preserved");
}

#[test]
fn complete_nvapi_profile_deletion_rejects_missing_owned_receipt() {
    let (storage, _path, game_id) = fresh_storage();
    let target_id = "RP-Target-NoOwner";
    let op_id = "op-del-no-owner";
    setup_owned_target_fixture(
        &storage,
        OwnedTargetFixtureParams {
            game_id: game_id.as_str(),
            target_id,
            kind: Some("renderpilot"),
            profile_is_predefined: 0,
            owned_state: None,
            with_claim: true,
            op_id,
        },
    );
    let result = storage.complete_nvapi_profile_deletion(op_id, game_id.as_str(), target_id);
    assert!(result.is_err(), "must reject missing owned profile receipt");
    assert!(
        get_drs_target_row(&storage, target_id).is_some(),
        "target must remain"
    );
    assert!(
        get_target_setting_claim_row(&storage, target_id, 0x10f9dc83).is_some(),
        "setting claim must remain"
    );
    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1, "pending operation must be preserved");
}

#[test]
fn complete_nvapi_profile_deletion_rejects_non_active_owned_receipt() {
    let (storage, _path, game_id) = fresh_storage();
    let target_id = "RP-Target-Pending";
    let op_id = "op-del-pending";
    setup_owned_target_fixture(
        &storage,
        OwnedTargetFixtureParams {
            game_id: game_id.as_str(),
            target_id,
            kind: Some("renderpilot"),
            profile_is_predefined: 0,
            owned_state: Some("pending"),
            with_claim: true,
            op_id,
        },
    );
    let result = storage.complete_nvapi_profile_deletion(op_id, game_id.as_str(), target_id);
    assert!(
        result.is_err(),
        "must reject non-active owned profile receipt"
    );
    let owner = storage
        .get_nvapi_owned_profile(game_id.as_str())
        .expect("query")
        .expect("owner exists");
    assert_eq!(
        owner.state, "pending",
        "pending state must remain unchanged"
    );
    assert!(
        get_drs_target_row(&storage, target_id).is_some(),
        "target must remain"
    );
    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1, "pending operation must be preserved");
}

#[test]
fn complete_nvapi_profile_deletion_rejects_other_game_claim() {
    let (storage, _path, game_id) = fresh_storage();
    let target_id = "RP-Target-ForeignClaim";
    let op_id = "op-del-foreign-claim";
    setup_owned_target_fixture(
        &storage,
        OwnedTargetFixtureParams {
            game_id: game_id.as_str(),
            target_id,
            kind: Some("renderpilot"),
            profile_is_predefined: 0,
            owned_state: Some("active"),
            with_claim: true,
            op_id,
        },
    );

    // Add another game that references this target
    let other_id = GameId::new("manual:other-game").expect("valid game id");
    let other_game = GameInstallation::new(
        GameIdentity::new(other_id.clone(), "Other Game", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Other").expect("install path"),
    );
    storage.upsert_game(&other_game).expect("upsert other game");

    storage
        .with_connection(|conn| {
            conn.execute(
                "INSERT INTO nvapi_game_claim_refs (game_id, target_id, setting_id, executable_path) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![other_id.as_str(), target_id, 0x10f9dc83, "C:/Games/Other/Game.exe"],
            )
            .map_err(crate::error::storage_error)?;
            Ok(())
        })
        .expect("add other game claim");

    let result = storage.complete_nvapi_profile_deletion(op_id, game_id.as_str(), target_id);
    assert!(
        result.is_err(),
        "must reject deletion when other game claims target"
    );

    // All records preserved
    assert!(
        storage
            .get_nvapi_owned_profile(game_id.as_str())
            .expect("query")
            .is_some()
    );
    assert!(get_drs_target_row(&storage, target_id).is_some());
    assert!(get_target_setting_claim_row(&storage, target_id, 0x10f9dc83).is_some());
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(target_id, 0x10f9dc83)
            .expect("refs"),
        2
    );
    let pending = storage.list_pending_nvapi_operations().expect("pending");
    assert_eq!(pending.len(), 1);
}

#[test]
fn release_nvapi_setting_claim_enforces_pending_fence_and_existence() {
    let (storage, _path, game_id) = fresh_storage();
    let target_id = "ReleaseFenceProfile";
    let setting_id = 0x10B3_292C;

    // 1. Releasing a nonexistent claim fails
    let err = storage
        .release_nvapi_setting_claim(game_id.as_str(), target_id, setting_id)
        .expect_err("must fail on nonexistent claim");
    assert!(err.to_string().contains("reference not found"));

    // Prepare a claim
    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-release-fence-init",
            game_id: game_id.as_str(),
            profile_name: target_id,
            target_kind: "external",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"ReleaseFenceProfile","is_predefined":false}"#,
            executable_path: "C:/Games/Test/Game.exe",
            application_witness_json: r#"{"app_name":"C:/Games/Test/Game.exe"}"#,
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
                value: Some(1),
            },
        })
        .expect("prep claim");
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-release-fence-init",
            scope: NvapiSettingOperationScope::Game {
                game_id: game_id.as_str(),
            },
            target_id,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            profile_identity_json: r#"{"name":"ReleaseFenceProfile","is_predefined":false}"#,
            composition_json: None,
        })
        .expect("finish claim");

    // 2. Begin a pending operation on the target
    storage
        .begin_nvapi_operation(
            "op-blocking-intent",
            Some(game_id.as_str()),
            Some(target_id),
            "setting",
            r#"{"setting_id":280176940}"#,
            r#"{"setting_id":280176940}"#,
        )
        .expect("begin intent");

    // Attempting release while pending op exists on target must fail
    let fence_err = storage
        .release_nvapi_setting_claim(game_id.as_str(), target_id, setting_id)
        .expect_err("must block on pending target operation");
    assert!(fence_err.to_string().contains("pending DRS operations"));

    // Complete the pending operation so fence clears
    storage
        .with_connection(|conn| {
            conn.execute(
                "DELETE FROM pending_drs_operations WHERE op_id = 'op-blocking-intent'",
                [],
            )
            .map_err(crate::error::storage_error)?;
            Ok(())
        })
        .expect("clear pending op");

    // Now release succeeds
    storage
        .release_nvapi_setting_claim(game_id.as_str(), target_id, setting_id)
        .expect("release must succeed once fence is clear");

    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(target_id, setting_id)
            .expect("count refs"),
        0
    );
}
