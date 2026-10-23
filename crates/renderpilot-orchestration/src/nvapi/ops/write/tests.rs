use std::assert_matches;

use super::{
    ReconciledWrite, application_witness_matches, classify_reconciled_write,
    reconciled_owned_composition_matches, restore_original_op, setting_application_witness_json,
    stable_profile_identity_matches,
};
use crate::nvapi::ops::WriteOp;
use crate::nvapi::ops::claim_release::{
    ClaimRestoreCompositionObservation, ClaimRestoreExpectation, ClaimRestoreObservation,
    validate_claim_restore_observation,
};
use renderpilot_storage_sqlite::NvapiSettingState;

#[test]
fn restoring_absent_original_deletes_the_profile_override() {
    assert_matches!(
        restore_original_op(false, None).expect("absent original"),
        WriteOp::Delete
    );
}

#[test]
fn restoring_explicit_original_sets_its_recorded_value() {
    assert_matches!(
        restore_original_op(true, Some(17)).expect("explicit original"),
        WriteOp::Set(17)
    );
}

#[test]
fn explicit_original_without_a_value_fails_closed() {
    assert!(restore_original_op(true, None).is_err());
}

#[test]
fn setting_witness_serializes_full_basename_and_path_application_identity() {
    let application = renderpilot_nvapi::ApplicationIdentity {
        app_name: "game.exe".to_owned(),
        user_friendly_name: "Game".to_owned(),
        launcher: String::new(),
        file_in_folder: String::new(),
        flags: 0,
        command_line: String::new(),
        is_predefined: false,
    };

    let witness = setting_application_witness_json(Some(&application))
        .expect("encode witness")
        .expect("witness is present");
    assert_eq!(
        witness,
        serde_json::to_string(&application).expect("full identity")
    );
    assert!(application_witness_matches(Some(&witness), Some(&witness)));
    let mut changed = application.clone();
    changed.user_friendly_name = "Another profile application".to_owned();
    let changed_witness = setting_application_witness_json(Some(&changed))
        .expect("encode changed witness")
        .expect("changed witness is present");
    assert!(!application_witness_matches(
        Some(&witness),
        Some(&changed_witness)
    ));
    let mut full_path = application.clone();
    full_path.app_name = r"C:\Games\One\game.exe".to_owned();
    let full_path_witness = setting_application_witness_json(Some(&full_path))
        .expect("encode full-path witness")
        .expect("full-path witness is present");
    assert_eq!(
        serde_json::from_str::<renderpilot_nvapi::ApplicationIdentity>(&full_path_witness)
            .expect("escaped full path remains a complete witness"),
        full_path
    );
    assert!(
        setting_application_witness_json(None)
            .expect("missing witness is not a serialization error")
            .is_none()
    );
    let mut empty = application;
    empty.app_name.clear();
    assert!(
        setting_application_witness_json(Some(&empty))
            .expect("empty name is not a serialization error")
            .is_none()
    );
}

#[test]
fn changed_application_witness_does_not_reconcile_a_game_write() {
    let recorded = r#"{"app_name":"C:/Games/One/game.exe"}"#;
    let rebound = r#"{"app_name":"C:/Games/Two/game.exe"}"#;

    let changed_binding = !application_witness_matches(Some(recorded), Some(rebound));
    assert!(changed_binding);
    assert!(!application_witness_matches(None, Some(recorded)));
    assert_eq!(
        classify_reconciled_write(
            true,
            !changed_binding,
            (true, Some(7)),
            (false, None),
            (true, Some(7)),
        ),
        ReconciledWrite::Conflict,
        "the same profile name and expected value cannot hide a changed executable binding",
    );
}

#[test]
fn stable_identity_predicate_blocks_before_and_after_classification_on_status_drift() {
    let name = "NVIDIA Shared Profile";
    assert!(stable_profile_identity_matches(
        name,
        Some(false),
        name,
        false
    ));
    assert!(stable_profile_identity_matches(
        name,
        Some(true),
        name,
        true
    ));
    for observed in [false, true] {
        let identity_matches =
            stable_profile_identity_matches(name, Some(!observed), name, observed);
        assert!(!identity_matches);
        for state in [(false, None), (true, Some(7))] {
            assert_eq!(
                classify_reconciled_write(
                    identity_matches,
                    true,
                    state,
                    (false, None),
                    (true, Some(7)),
                ),
                ReconciledWrite::Conflict,
            );
        }
    }
    assert!(!stable_profile_identity_matches(name, None, name, false));
    assert!(!stable_profile_identity_matches(
        name,
        Some(false),
        "Different profile",
        false
    ));
}

#[test]
fn claim_restore_accepts_the_same_basename_witness_for_the_selected_path() {
    let application = renderpilot_nvapi::ApplicationIdentity {
        app_name: "game.exe".to_owned(),
        user_friendly_name: "Shared game profile".to_owned(),
        launcher: String::new(),
        file_in_folder: String::new(),
        flags: 0,
        command_line: String::new(),
        is_predefined: false,
    };
    let witness = serde_json::to_string(&application).expect("witness JSON");
    for profile_is_predefined in [false, true] {
        let result = validate_claim_restore_observation(
            &ClaimRestoreExpectation {
                target_id: "NVIDIA Shared Profile",
                profile_is_predefined,
                application_witness_json: &witness,
                state: NvapiSettingState {
                    present: false,
                    value: None,
                },
                setting_id: 10,
            },
            &ClaimRestoreObservation {
                profile_name: "NVIDIA Shared Profile",
                is_predefined: profile_is_predefined,
                application_witness_json: Some(&witness),
                state: NvapiSettingState {
                    present: false,
                    value: None,
                },
                composition: ClaimRestoreCompositionObservation {
                    owner_receipt: None,
                    before_restore: None,
                    after_restore: None,
                },
            },
        );
        assert_eq!(result, Ok(()));
    }
}

#[test]
fn claim_restore_rejects_predefined_status_drift_with_the_same_basename_witness() {
    let application = renderpilot_nvapi::ApplicationIdentity {
        app_name: "game.exe".to_owned(),
        user_friendly_name: "Shared game profile".to_owned(),
        launcher: String::new(),
        file_in_folder: String::new(),
        flags: 0,
        command_line: String::new(),
        is_predefined: false,
    };
    let witness = serde_json::to_string(&application).expect("witness JSON");
    for recorded_status in [false, true] {
        let result = validate_claim_restore_observation(
            &ClaimRestoreExpectation {
                target_id: "NVIDIA Shared Profile",
                profile_is_predefined: recorded_status,
                application_witness_json: &witness,
                state: NvapiSettingState {
                    present: false,
                    value: None,
                },
                setting_id: 10,
            },
            &ClaimRestoreObservation {
                profile_name: "NVIDIA Shared Profile",
                is_predefined: !recorded_status,
                application_witness_json: Some(&witness),
                state: NvapiSettingState {
                    present: false,
                    value: None,
                },
                composition: ClaimRestoreCompositionObservation {
                    owner_receipt: None,
                    before_restore: None,
                    after_restore: None,
                },
            },
        );
        assert_eq!(result, Err("profile identity changed"));
    }
}

#[test]
fn claim_restore_requires_a_complete_application_witness() {
    let incomplete_witness = r#"{"app_name":"game.exe"}"#;
    let result = validate_claim_restore_observation(
        &ClaimRestoreExpectation {
            target_id: "NVIDIA Shared Profile",
            profile_is_predefined: true,
            application_witness_json: incomplete_witness,
            state: NvapiSettingState {
                present: false,
                value: None,
            },
            setting_id: 10,
        },
        &ClaimRestoreObservation {
            profile_name: "NVIDIA Shared Profile",
            is_predefined: true,
            application_witness_json: Some(incomplete_witness),
            state: NvapiSettingState {
                present: false,
                value: None,
            },
            composition: ClaimRestoreCompositionObservation {
                owner_receipt: None,
                before_restore: None,
                after_restore: None,
            },
        },
    );
    assert_eq!(
        result,
        Err("effective-profile application witness is invalid")
    );
}

#[test]
fn claim_restore_rejects_rebound_profile_even_when_value_matches() {
    let witness = r#"{"app_name":"C:/Games/One/game.exe"}"#;
    let result = validate_claim_restore_observation(
        &ClaimRestoreExpectation {
            target_id: "RenderPilot - One [id]",
            profile_is_predefined: false,
            application_witness_json: witness,
            state: NvapiSettingState {
                present: true,
                value: Some(7),
            },
            setting_id: 10,
        },
        &ClaimRestoreObservation {
            profile_name: "RenderPilot - Another [id]",
            is_predefined: false,
            application_witness_json: Some(witness),
            state: NvapiSettingState {
                present: true,
                value: Some(7),
            },
            composition: ClaimRestoreCompositionObservation {
                owner_receipt: None,
                before_restore: None,
                after_restore: None,
            },
        },
    );

    assert_eq!(result, Err("profile identity changed"));
}

#[test]
fn claim_restore_allows_only_the_claimed_setting_to_change_in_owned_composition() {
    let app = renderpilot_nvapi::ApplicationIdentity {
        app_name: "C:/Games/One/game.exe".to_owned(),
        user_friendly_name: "Game".to_owned(),
        launcher: String::new(),
        file_in_folder: String::new(),
        flags: 0,
        command_line: String::new(),
        is_predefined: false,
    };
    let witness = serde_json::to_string(&app).expect("witness JSON");
    let before = serde_json::json!({
        "applications": [app],
        "settings": [{"id": 10, "value": 7}, {"id": 11, "value": 2}],
    });
    let after = serde_json::json!({
        "applications": [{
            "app_name": "C:/Games/One/game.exe",
            "user_friendly_name": "Game",
            "launcher": "",
            "file_in_folder": "",
            "flags": 0,
            "command_line": "",
            "is_predefined": false,
        }],
        "settings": [{"id": 11, "value": 2}],
    });
    let before_json = before.to_string();

    let result = validate_claim_restore_observation(
        &ClaimRestoreExpectation {
            target_id: "RenderPilot - One [id]",
            profile_is_predefined: false,
            application_witness_json: &witness,
            state: NvapiSettingState {
                present: false,
                value: None,
            },
            setting_id: 10,
        },
        &ClaimRestoreObservation {
            profile_name: "RenderPilot - One [id]",
            is_predefined: false,
            application_witness_json: Some(&witness),
            state: NvapiSettingState {
                present: false,
                value: None,
            },
            composition: ClaimRestoreCompositionObservation {
                owner_receipt: Some(&before_json),
                before_restore: Some(&before_json),
                after_restore: Some(&after),
            },
        },
    );
    assert_eq!(result, Ok(()));

    let drifted_after = serde_json::json!({
        "applications": after["applications"],
        "settings": [{"id": 11, "value": 3}],
    });
    let drift_result = validate_claim_restore_observation(
        &ClaimRestoreExpectation {
            target_id: "RenderPilot - One [id]",
            profile_is_predefined: false,
            application_witness_json: &witness,
            state: NvapiSettingState {
                present: false,
                value: None,
            },
            setting_id: 10,
        },
        &ClaimRestoreObservation {
            profile_name: "RenderPilot - One [id]",
            is_predefined: false,
            application_witness_json: Some(&witness),
            state: NvapiSettingState {
                present: false,
                value: None,
            },
            composition: ClaimRestoreCompositionObservation {
                owner_receipt: Some(&before_json),
                before_restore: Some(&before_json),
                after_restore: Some(&drifted_after),
            },
        },
    );
    assert_eq!(
        drift_result,
        Err("owned profile composition changed beyond the restored setting")
    );
}

fn sample_owned_composition() -> serde_json::Value {
    serde_json::json!({
        "applications": [{
            "app_name": "C:/Games/Game/game.exe",
            "user_friendly_name": "Game",
            "launcher": "",
            "file_in_folder": "",
            "flags": 0,
            "command_line": "",
            "is_predefined": false,
        }],
        "settings": [
            {"id": 10, "value": 7},
            {"id": 11, "value": 2},
        ],
    })
}

#[test]
fn reconciled_owned_composition_after_accepts_when_only_claimed_setting_changed() {
    let before = sample_owned_composition();
    let owner_json = before.to_string();
    let mut after = before.clone();
    after["settings"][0]["value"] = serde_json::json!(9);

    let result = reconciled_owned_composition_matches(
        ReconciledWrite::After,
        &owner_json,
        &before,
        &after,
        10,
    );
    assert!(result.expect("validation succeeds"));
}

#[test]
fn reconciled_owned_composition_after_rejects_unrelated_setting_change() {
    let before = sample_owned_composition();
    let owner_json = before.to_string();
    let mut after = before.clone();
    after["settings"][0]["value"] = serde_json::json!(9);
    after["settings"][1]["value"] = serde_json::json!(3);

    let result = reconciled_owned_composition_matches(
        ReconciledWrite::After,
        &owner_json,
        &before,
        &after,
        10,
    );
    assert!(!result.expect("validation succeeds"));
}

#[test]
fn reconciled_owned_composition_after_rejects_application_composition_change() {
    let before = sample_owned_composition();
    let owner_json = before.to_string();
    let mut after = before.clone();
    after["settings"][0]["value"] = serde_json::json!(9);
    after["applications"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "app_name": "C:/Games/Game/launcher.exe",
            "user_friendly_name": "Launcher",
            "launcher": "",
            "file_in_folder": "",
            "flags": 0,
            "command_line": "",
            "is_predefined": false,
        }));

    let result = reconciled_owned_composition_matches(
        ReconciledWrite::After,
        &owner_json,
        &before,
        &after,
        10,
    );
    assert!(!result.expect("validation succeeds"));
}

#[test]
fn reconciled_owned_composition_before_accepts_exact_restored_composition() {
    let before = sample_owned_composition();
    let owner_json = before.to_string();

    let result = reconciled_owned_composition_matches(
        ReconciledWrite::Before,
        &owner_json,
        &before,
        &before,
        10,
    );
    assert!(result.expect("validation succeeds"));
}

#[test]
fn reconciled_owned_composition_before_rejects_any_composition_drift() {
    let before = sample_owned_composition();
    let owner_json = before.to_string();

    let mut drifted_setting = before.clone();
    drifted_setting["settings"][0]["value"] = serde_json::json!(9);
    let result_setting = reconciled_owned_composition_matches(
        ReconciledWrite::Before,
        &owner_json,
        &before,
        &drifted_setting,
        10,
    );
    assert!(!result_setting.expect("validation succeeds"));

    let mut drifted_app = before.clone();
    drifted_app["applications"] = serde_json::json!([]);
    let result_app = reconciled_owned_composition_matches(
        ReconciledWrite::Before,
        &owner_json,
        &before,
        &drifted_app,
        10,
    );
    assert!(!result_app.expect("validation succeeds"));
}

#[test]
fn reconciled_owned_composition_rejects_when_owner_receipt_differs_from_before_snapshot() {
    let before = sample_owned_composition();
    let mut drifted_owner = before.clone();
    drifted_owner["settings"][0]["value"] = serde_json::json!(1);
    let drifted_owner_json = drifted_owner.to_string();

    let mut actual_after = before.clone();
    actual_after["settings"][0]["value"] = serde_json::json!(9);

    let result_after = reconciled_owned_composition_matches(
        ReconciledWrite::After,
        &drifted_owner_json,
        &before,
        &actual_after,
        10,
    );
    assert!(!result_after.expect("validation succeeds"));

    let result_before = reconciled_owned_composition_matches(
        ReconciledWrite::Before,
        &drifted_owner_json,
        &before,
        &before,
        10,
    );
    assert!(!result_before.expect("validation succeeds"));
}

#[test]
fn reconciled_owned_composition_handles_malformed_receipt_and_missing_inputs() {
    let before = sample_owned_composition();
    let after = before.clone();

    // 1. If owner receipt is malformed or drifted from before, validation safely short-circuits
    // to Ok(false) without failing or attempting to parse malformed JSON, because the
    // owner_composition_json != before_json string comparison runs first.
    let malformed_owner = "not valid json {{{";
    let mismatch_result = reconciled_owned_composition_matches(
        ReconciledWrite::After,
        malformed_owner,
        &before,
        &after,
        10,
    );
    assert!(!mismatch_result.expect("short circuits on mismatch"));

    // 2. Direct parsing of malformed JSON in composition_matches_except_setting returns an Err(ServiceError).
    assert!(super::composition_matches_except_setting(malformed_owner, &before, 10).is_err());

    // 3. ReconciledWrite::Conflict outcome directly returns Ok(false).
    let conflict_result = reconciled_owned_composition_matches(
        ReconciledWrite::Conflict,
        &before.to_string(),
        &before,
        &after,
        10,
    );
    assert!(!conflict_result.expect("conflict outcome returns false"));

    // Missing inputs (e.g. global writes or unobserved composition) are guarded at the caller boundary:
    // `if let (Some(owner), Some(actual), Some(before)) = (owner.as_ref(), after_composition.as_ref(), before_composition)`
    // ensuring the helper only executes when all required components are present.
}
