use super::*;

fn prepare_external_setting_claim(
    storage: &SqliteStorage,
    game_id: &str,
    app_path: &str,
    application_count: u32,
    before: (bool, Option<u32>),
    after: (bool, Option<u32>),
    op_id: &str,
) {
    prepare_external_setting_claim_with_witness(
        storage,
        ExternalSettingClaimFixture {
            game_id,
            app_path,
            witness_app_name: app_path,
            profile_is_predefined: false,
            application_count,
            before,
            after,
            op_id,
        },
    );
}

#[derive(Clone, Copy)]
struct ExternalSettingClaimFixture<'a> {
    game_id: &'a str,
    app_path: &'a str,
    witness_app_name: &'a str,
    profile_is_predefined: bool,
    application_count: u32,
    before: (bool, Option<u32>),
    after: (bool, Option<u32>),
    op_id: &'a str,
}

fn prepare_external_setting_claim_with_witness(
    storage: &SqliteStorage,
    fixture: ExternalSettingClaimFixture<'_>,
) {
    let ExternalSettingClaimFixture {
        game_id,
        app_path,
        witness_app_name,
        profile_is_predefined,
        application_count,
        before,
        after,
        op_id,
    } = fixture;
    let witness = serde_json::to_string(&app(witness_app_name)).expect("witness json");
    let identity_json = profile_identity_json_with_status(
        application_count,
        u32::from(before.0),
        profile_is_predefined,
    );
    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id,
            game_id,
            profile_name: PROFILE,
            target_kind: "external",
            profile_is_predefined,
            profile_identity_json: &identity_json,
            executable_path: app_path,
            application_witness_json: &witness,
            setting_id: SETTING_ID,
            original: NvapiSettingState {
                present: false,
                value: None,
            },
            before: NvapiSettingState {
                present: before.0,
                value: before.1,
            },
            after: NvapiSettingState {
                present: after.0,
                value: after.1,
            },
        })
        .expect("persist external setting receipt");
}

fn finish_external_setting_claim(
    storage: &SqliteStorage,
    game_id: &str,
    application_count: u32,
    state: (bool, Option<u32>),
    op_id: &str,
) {
    let identity_json = profile_identity_json(application_count, u32::from(state.0));
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id,
            scope: NvapiSettingOperationScope::Game { game_id },
            target_id: PROFILE,
            setting_id: SETTING_ID,
            expected: NvapiSettingState {
                present: state.0,
                value: state.1,
            },
            profile_identity_json: &identity_json,
            composition_json: None,
        })
        .expect("finalize external setting receipt");
}

fn shared_setting_observation(path_a: &str, value: u32) -> Observation {
    let mut applications = vec![app("C:/Games/shared/Other.exe"), app(path_a)];
    applications.sort_by(|left, right| left.app_name.cmp(&right.app_name));
    let profile = observed_profile(
        PROFILE,
        applications,
        Some((SETTING_ID, true, Some(value))),
        false,
    );
    let mut observation = observation_for_path(path_a, profile);
    observation.named_profile = Lookup::Found(
        match &observation.path_profiles[&normalized_path_key(path_a)] {
            Lookup::Found(profile) => profile.clone(),
            _ => unreachable!(),
        },
    );
    observation
}

#[test]
fn restart_setting_rejects_a_changed_lookup_witness_even_when_value_matches() {
    let (storage, games) = storage_with_games(&["setting-witness-mismatch"]);
    let game_id = &games[0];
    let path = "C:/Games/setting-witness-mismatch/Game.exe";
    let op_id = "setting-witness-mismatch";
    prepare_external_setting_claim(
        &storage,
        game_id,
        path,
        1,
        (true, Some(3)),
        (true, Some(4)),
        op_id,
    );
    let mut profile = observed_profile(
        PROFILE,
        vec![app(path)],
        Some((SETTING_ID, true, Some(4))),
        false,
    );
    profile.matched_application = Some(app("C:/Games/other/Other.exe"));
    let observation = Observation {
        named_profile: Lookup::Found(profile.clone()),
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Found(profile))]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("reject unrelated setting witness");

    let rows = storage
        .list_pending_nvapi_operations()
        .expect("read pending operations");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].phase, "conflict");
}

#[test]
fn restart_authorized_owned_write_merges_only_requested_setting_and_preserves_external_drift() {
    let (storage, games) = storage_with_games(&["authorized-owned-merge"]);
    let game_id = &games[0];
    let path = "C:/Games/authorized-owned-merge/Game.exe";
    create_owned_fixture_with_settings(
        &storage,
        game_id,
        path,
        vec![
            setting_identity(SETTING_ID, 1),
            setting_identity(SETTING_ID + 1, 2),
        ],
    );

    let mut before_profile = observed_profile(
        PROFILE,
        vec![app(path)],
        Some((SETTING_ID, true, Some(10))),
        false,
    );
    before_profile
        .settings
        .push(setting_identity(SETTING_ID + 1, 200));
    before_profile.identity.setting_count = 2;
    prepare_authorized_owned_setting(
        &storage,
        AuthorizedOwnedSettingFixture {
            game_id,
            path,
            before_profile: &before_profile,
            setting_id: SETTING_ID,
            before: (true, Some(10)),
            after: (true, Some(11)),
            op_id: "authorized-owned-merge",
        },
    );

    let mut after_profile = before_profile.clone();
    after_profile.explicit_setting = Some((SETTING_ID, true, Some(11)));
    after_profile.settings[0] = setting_identity(SETTING_ID, 11);
    recover_with(&storage, &observation_for_path(path, after_profile))
        .expect("recover authorized owned setting");

    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending operations")
            .is_empty()
    );
    let owner = storage
        .get_nvapi_owned_profile(game_id)
        .expect("query owner")
        .expect("owned receipt");
    let saved: serde_json::Value =
        serde_json::from_str(&owner.composition_json).expect("owner composition JSON");
    assert_eq!(saved["settings"].as_array().unwrap().len(), 2);
    assert_eq!(
        saved["settings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|setting| setting["id"] == SETTING_ID)
            .unwrap()["current_value"],
        "11"
    );
    assert_eq!(
        saved["settings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|setting| setting["id"] == SETTING_ID + 1)
            .unwrap()["current_value"],
        "2",
        "unrelated driver changes are not adopted into the owned-profile receipt"
    );
    let claim = storage
        .get_nvapi_target_setting_claim(game_id, PROFILE, SETTING_ID)
        .expect("query claim")
        .expect("new claim");
    assert_eq!(claim.original_value, Some(10));
    assert_eq!(claim.expected_value, Some(11));
}

#[test]
fn authorized_owned_write_rejects_application_composition_or_witness_drift() {
    let (storage, games) = storage_with_games(&["authorized-owned-binding"]);
    let game_id = &games[0];
    let path = "C:/Games/authorized-owned-binding/Game.exe";
    create_owned_fixture(&storage, game_id, path);

    let mut changed_apps = observed_profile(
        PROFILE,
        vec![app(path), app("C:/Games/Other.exe")],
        None,
        false,
    );
    changed_apps.identity.application_count = 2;
    let changed_apps_json = composition_json(&changed_apps).expect("changed composition");
    let identity_json =
        serde_json::to_string(&changed_apps.identity).expect("changed identity JSON");
    let witness = serde_json::to_string(&app(path)).expect("witness JSON");
    let error = storage
        .prepare_nvapi_authorized_game_setting_operation(
            NvapiGameSettingPreparation {
                op_id: "authorized-owned-app-drift",
                game_id,
                profile_name: PROFILE,
                target_kind: "renderpilot",
                profile_is_predefined: false,
                profile_identity_json: &identity_json,
                executable_path: path,
                application_witness_json: &witness,
                setting_id: SETTING_ID,
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
                    value: Some(7),
                },
            },
            Some(&changed_apps_json),
        )
        .expect_err("owned application composition drift is not authorized");
    assert!(
        error
            .to_string()
            .contains("application composition changed")
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending operations")
            .is_empty()
    );

    let valid_before = observed_profile(PROFILE, vec![app(path)], None, false);
    let valid_composition = composition_json(&valid_before).expect("valid composition");
    let rebound_witness =
        serde_json::to_string(&app("C:/Games/Other.exe")).expect("rebound witness JSON");
    let error = storage
        .prepare_nvapi_authorized_game_setting_operation(
            NvapiGameSettingPreparation {
                op_id: "authorized-owned-witness-drift",
                game_id,
                profile_name: PROFILE,
                target_kind: "renderpilot",
                profile_is_predefined: false,
                profile_identity_json: &profile_identity_json(1, 0),
                executable_path: path,
                application_witness_json: &rebound_witness,
                setting_id: SETTING_ID,
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
                    value: Some(7),
                },
            },
            Some(&valid_composition),
        )
        .expect_err("owned executable witness remains exact");
    assert!(error.to_string().contains("witness changed"));
}

#[test]
fn restart_authorized_owned_write_conflicts_on_third_state_or_concurrent_unrelated_change() {
    for (suffix, actual_target, actual_other) in
        [("third-state", 99, 20), ("unrelated-change", 11, 21)]
    {
        let (storage, games) = storage_with_games(&[suffix]);
        let game_id = &games[0];
        let path = format!("C:/Games/{suffix}/Game.exe");
        create_owned_fixture_with_settings(
            &storage,
            game_id,
            &path,
            vec![
                setting_identity(SETTING_ID, 1),
                setting_identity(SETTING_ID + 1, 2),
            ],
        );
        let mut before_profile = observed_profile(
            PROFILE,
            vec![app(&path)],
            Some((SETTING_ID, true, Some(10))),
            false,
        );
        before_profile
            .settings
            .push(setting_identity(SETTING_ID + 1, 20));
        before_profile.identity.setting_count = 2;
        let op_id = format!("authorized-owned-{suffix}");
        prepare_authorized_owned_setting(
            &storage,
            AuthorizedOwnedSettingFixture {
                game_id,
                path: &path,
                before_profile: &before_profile,
                setting_id: SETTING_ID,
                before: (true, Some(10)),
                after: (true, Some(11)),
                op_id: &op_id,
            },
        );

        let mut actual = before_profile;
        actual.explicit_setting = Some((SETTING_ID, true, Some(actual_target)));
        actual.settings[0] = setting_identity(SETTING_ID, actual_target);
        actual.settings[1] = setting_identity(SETTING_ID + 1, actual_other);
        recover_with(&storage, &observation_for_path(&path, actual))
            .expect("recover unresolved owned write");

        let pending = storage
            .list_pending_nvapi_operations()
            .expect("pending operations");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].op_id, op_id);
        assert_eq!(pending[0].phase, "conflict");
    }
}

#[test]
fn restart_ignored_owned_write_cancels_and_allows_another_setting_write() {
    let (storage, games) = storage_with_games(&["authorized-owned-ignored"]);
    let game_id = &games[0];
    let path = "C:/Games/authorized-owned-ignored/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    let before_profile = observed_profile(
        PROFILE,
        vec![app(path)],
        Some((SETTING_ID, false, None)),
        false,
    );
    prepare_authorized_owned_setting(
        &storage,
        AuthorizedOwnedSettingFixture {
            game_id,
            path,
            before_profile: &before_profile,
            setting_id: SETTING_ID,
            before: (false, None),
            after: (true, Some(7)),
            op_id: "authorized-owned-ignored",
        },
    );

    recover_with(
        &storage,
        &observation_for_path(path, before_profile.clone()),
    )
    .expect("ignored driver change cancels to the exact fresh before state");
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending operations")
            .is_empty()
    );
    assert!(
        storage
            .get_nvapi_target_setting_claim(game_id, PROFILE, SETTING_ID)
            .expect("query cancelled claim")
            .is_none(),
        "the unconfirmed new claim is rolled back"
    );

    prepare_authorized_owned_setting(
        &storage,
        AuthorizedOwnedSettingFixture {
            game_id,
            path,
            before_profile: &before_profile,
            setting_id: SETTING_ID + 1,
            before: (false, None),
            after: (true, Some(9)),
            op_id: "authorized-owned-next-setting",
        },
    );
    assert_eq!(
        storage
            .list_pending_nvapi_operations()
            .expect("next setting remains writable")
            .len(),
        1
    );
}

#[test]
fn restart_malformed_owned_snapshot_is_fenced_without_erasing_journal() {
    let (storage, games) = storage_with_games(&["authorized-owned-malformed"]);
    let game_id = &games[0];
    let path = "C:/Games/authorized-owned-malformed/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    let malformed_before = serde_json::json!({
        "setting_id": SETTING_ID,
        "executable_path": path,
        "reference_previous_path": null,
        "present": false,
        "value": null,
        "claim_was_new": false,
        "reference_was_new": true,
        "owned_before_composition": "not a composition"
    })
    .to_string();
    storage
        .begin_nvapi_operation(
            "authorized-owned-malformed",
            Some(game_id),
            Some(PROFILE),
            "setting",
            &malformed_before,
            &serde_json::json!({
                "setting_id": SETTING_ID,
                "present": true,
                "value": 7
            })
            .to_string(),
        )
        .expect("persist malformed receipt fixture");
    let actual = observed_profile(
        PROFILE,
        vec![app(path)],
        Some((SETTING_ID, true, Some(7))),
        false,
    );

    recover_with(&storage, &observation_for_path(path, actual))
        .expect("malformed snapshot is retained as a conflict");
    let pending = storage
        .list_pending_nvapi_operations()
        .expect("read retained journal");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].phase, "conflict");
    assert_eq!(pending[0].before_json, malformed_before);
}

#[test]
fn restart_setting_rejects_a_missing_recorded_witness() {
    let (storage, games) = storage_with_games(&["setting-missing-receipt", "witness-other-path"]);
    let path = "C:/Games/setting-missing-receipt/Game.exe";
    let other_path = "C:/Games/witness-other-path/Game.exe";
    prepare_external_setting_claim(
        &storage,
        &games[1],
        other_path,
        1,
        (false, None),
        (true, Some(3)),
        "witness-other-path",
    );
    finish_external_setting_claim(
        &storage,
        &games[1],
        1,
        (true, Some(3)),
        "witness-other-path",
    );
    assert_eq!(
        storage
            .get_nvapi_target_profile_is_predefined(PROFILE)
            .expect("target identity receipt"),
        Some(false)
    );
    assert!(
        storage
            .get_nvapi_application_witness(PROFILE, path)
            .expect("selected path witness")
            .is_none()
    );
    storage
        .begin_nvapi_operation(
            "setting-missing-receipt",
            Some(&games[0]),
            Some(PROFILE),
            "setting",
            &serde_json::json!({
                "setting_id": SETTING_ID,
                "executable_path": path,
                "present": false,
                "value": null
            })
            .to_string(),
            &serde_json::json!({"setting_id": SETTING_ID, "present": true, "value": 12})
                .to_string(),
        )
        .expect("pending setting without an application receipt");
    let profile = observed_profile(
        PROFILE,
        vec![app(path)],
        Some((SETTING_ID, true, Some(12))),
        false,
    );

    recover_with(&storage, &observation_for_path(path, profile))
        .expect("missing witness must be marked as a conflict");

    let pending = storage
        .list_pending_nvapi_operations()
        .expect("pending operations");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].phase, "conflict");
}

#[test]
fn restart_recovery_finalizes_saved_setting_with_basename_lookup_witness() {
    for profile_is_predefined in [false, true] {
        let (storage, games) = storage_with_games(&["setting-basename-witness"]);
        let game_id = &games[0];
        let path = "C:/Games/setting-basename-witness/Game.exe";
        prepare_external_setting_claim_with_witness(
            &storage,
            ExternalSettingClaimFixture {
                game_id,
                app_path: path,
                witness_app_name: "Game.exe",
                profile_is_predefined,
                application_count: 1,
                before: (false, None),
                after: (true, Some(12)),
                op_id: "setting-basename-after-save",
            },
        );
        let mut profile = observed_profile(
            PROFILE,
            vec![app(path)],
            Some((SETTING_ID, true, Some(12))),
            profile_is_predefined,
        );
        profile.matched_application = Some(app("Game.exe"));

        recover_with(&storage, &observation_for_path(path, profile))
            .expect("finalize saved setting from same full-path lookup");

        assert!(
            storage
                .list_pending_nvapi_operations()
                .expect("pending operations")
                .is_empty()
        );
        let claims = storage
            .list_nvapi_setting_claims_for_game(game_id)
            .expect("setting claim");
        assert_eq!(claims.len(), 1);
        assert_eq!(
            (claims[0].expected_present, claims[0].expected_value),
            (true, Some(12))
        );
    }
}

#[test]
fn restart_setting_rejects_profile_status_drift_before_or_after_save() {
    for (game_name, state) in [
        ("setting-status-before", (false, None)),
        ("setting-status-after", (true, Some(12))),
    ] {
        let (storage, games) = storage_with_games(&[game_name]);
        let path = format!("C:/Games/{game_name}/Game.exe");
        prepare_external_setting_claim_with_witness(
            &storage,
            ExternalSettingClaimFixture {
                game_id: &games[0],
                app_path: &path,
                witness_app_name: "Game.exe",
                profile_is_predefined: false,
                application_count: 1,
                before: (false, None),
                after: (true, Some(12)),
                op_id: game_name,
            },
        );
        let mut profile = observed_profile(
            PROFILE,
            vec![app(&path)],
            Some((SETTING_ID, state.0, state.1)),
            true,
        );
        profile.matched_application = Some(app("Game.exe"));

        recover_with(&storage, &observation_for_path(&path, profile))
            .expect("status drift must be marked as a conflict");

        let pending = storage
            .list_pending_nvapi_operations()
            .expect("pending operations");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].phase, "conflict");
    }
}

#[test]
fn setting_and_owned_lifecycle_witness_policies_remain_distinct() {
    let path = "C:/Games/basename/Game.exe";
    let mut profile = observed_profile(PROFILE, vec![app(path)], None, false);
    profile.matched_application = Some(app("Game.exe"));

    assert_eq!(
        effective_setting_witness_json(&profile),
        Some(serde_json::to_string(&app("Game.exe")).expect("basename witness"))
    );
    assert!(exact_owned_lifecycle_witness_for_path(&profile, path).is_none());

    profile.matched_application = Some(app(path));
    assert_eq!(
        exact_owned_lifecycle_witness_for_path(&profile, path),
        profile.matched_application.as_ref()
    );
    assert_eq!(
        effective_setting_witness_json(&profile),
        Some(serde_json::to_string(&app(path)).expect("exact-path witness"))
    );
}

#[test]
fn restart_recovery_finalizes_saved_shared_external_write_and_preserves_both_claims() {
    let (storage, games) = storage_with_games(&["shared-a", "shared-b"]);
    let path_a = "C:/Games/shared/GameA.exe";
    let path_b = "C:/Games/shared/Other.exe";
    prepare_external_setting_claim(
        &storage,
        &games[1],
        path_b,
        2,
        (false, None),
        (true, Some(3)),
        "b-write",
    );
    finish_external_setting_claim(&storage, &games[1], 2, (true, Some(3)), "b-write");
    prepare_external_setting_claim(
        &storage,
        &games[0],
        path_a,
        2,
        (true, Some(3)),
        (true, Some(4)),
        "a-after-save",
    );

    recover_with(&storage, &shared_setting_observation(path_a, 4))
        .expect("finalize saved setting after restart");
    recover_with(&storage, &shared_setting_observation(path_a, 4))
        .expect("restart is idempotent after finalize");

    let a_claims = storage
        .list_nvapi_setting_claims_for_game(&games[0])
        .expect("A's claim");
    let b_claims = storage
        .list_nvapi_setting_claims_for_game(&games[1])
        .expect("B's claim");
    assert_eq!(a_claims.len(), 1);
    assert_eq!(b_claims.len(), 1);
    assert!(!a_claims[0].original_present);
    assert_eq!(
        (a_claims[0].expected_present, a_claims[0].expected_value),
        (true, Some(4))
    );
    assert!((!b_claims[0].original_present) && b_claims[0].executable_path == path_b);
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn restart_recovery_cancels_pre_save_shared_write_without_losing_other_game_claim() {
    let (storage, games) = storage_with_games(&["shared-cancel-a", "shared-cancel-b"]);
    let path_a = "C:/Games/shared-cancel/GameA.exe";
    let path_b = "C:/Games/shared-cancel/Other.exe";
    prepare_external_setting_claim(
        &storage,
        &games[1],
        path_b,
        2,
        (false, None),
        (true, Some(3)),
        "b-write-cancel",
    );
    finish_external_setting_claim(&storage, &games[1], 2, (true, Some(3)), "b-write-cancel");
    prepare_external_setting_claim(
        &storage,
        &games[0],
        path_a,
        2,
        (true, Some(3)),
        (true, Some(4)),
        "a-before-save",
    );

    recover_with(&storage, &shared_setting_observation(path_a, 3))
        .expect("cancel proven before-state after restart");

    assert!(
        storage
            .list_nvapi_setting_claims_for_game(&games[0])
            .expect("A claim list")
            .is_empty()
    );
    let b_claims = storage
        .list_nvapi_setting_claims_for_game(&games[1])
        .expect("B claim list");
    assert_eq!(b_claims.len(), 1);
    assert!(!b_claims[0].original_present);
    assert_eq!(
        (b_claims[0].expected_present, b_claims[0].expected_value),
        (true, Some(3))
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn restart_recovery_finalizes_global_write_from_fresh_base_profile_observation() {
    let (storage, _) = storage_with_games(&[]);
    let before = (false, None);
    let after = (true, Some(9));
    storage
        .prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "global-after-save",
            profile_name: PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &profile_identity_json_with_status(0, 0, true),
            setting_id: SETTING_ID,
            original: NvapiSettingState {
                present: before.0,
                value: before.1,
            },
            before: NvapiSettingState {
                present: before.0,
                value: before.1,
            },
            after: NvapiSettingState {
                present: after.0,
                value: after.1,
            },
        })
        .expect("persist global intent");
    let base = observed_profile(PROFILE, Vec::new(), Some((SETTING_ID, true, Some(9))), true);
    let observation = Observation {
        named_profile: Lookup::Found(base.clone()),
        path_profiles: HashMap::new(),
        base_profile: Lookup::Found(base),
    };

    recover_with(&storage, &observation).expect("finalize global write after restart");

    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(PROFILE, SETTING_ID)
            .expect("global original state"),
        Some((false, None))
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn restart_global_setting_rejects_predefined_status_drift() {
    for (op_id, state) in [
        ("global-status-before", (false, None)),
        ("global-status-after", (true, Some(9))),
    ] {
        let (storage, _) = storage_with_games(&[]);
        storage
            .prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
                op_id,
                profile_name: PROFILE,
                profile_is_predefined: false,
                profile_identity_json: &profile_identity_json(0, 0),
                setting_id: SETTING_ID,
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
                    value: Some(9),
                },
            })
            .expect("persist global intent");
        let base = observed_profile(
            PROFILE,
            Vec::new(),
            Some((SETTING_ID, state.0, state.1)),
            true,
        );
        let observation = Observation {
            named_profile: Lookup::Found(base.clone()),
            path_profiles: HashMap::new(),
            base_profile: Lookup::Found(base),
        };

        recover_with(&storage, &observation).expect("status drift must be marked as a conflict");

        let pending = storage
            .list_pending_nvapi_operations()
            .expect("pending operations");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].phase, "conflict");
    }
}

#[test]
fn restart_recovery_finalizes_when_global_setting_before_equals_after_and_preserves_new_claim() {
    let (storage, _) = storage_with_games(&[]);
    let before = (true, Some(9));
    let after = (true, Some(9));
    storage
        .prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "global-new-before-equals-after",
            profile_name: PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &profile_identity_json_with_status(0, 0, true),
            setting_id: SETTING_ID,
            original: NvapiSettingState {
                present: before.0,
                value: before.1,
            },
            before: NvapiSettingState {
                present: before.0,
                value: before.1,
            },
            after: NvapiSettingState {
                present: after.0,
                value: after.1,
            },
        })
        .expect("persist global intent");
    let base = observed_profile(PROFILE, Vec::new(), Some((SETTING_ID, true, Some(9))), true);
    let observation = Observation {
        named_profile: Lookup::Found(base.clone()),
        path_profiles: HashMap::new(),
        base_profile: Lookup::Found(base),
    };

    recover_with(&storage, &observation).expect("finalize global write when before == after");

    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(PROFILE, SETTING_ID)
            .expect("global original state"),
        Some((true, Some(9)))
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn restart_recovery_finalizes_when_existing_global_setting_before_equals_after() {
    let (storage, _) = storage_with_games(&[]);
    let initial_identity_json = profile_identity_json_with_status(0, 0, true);
    storage
        .prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "global-init",
            profile_name: PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &initial_identity_json,
            setting_id: SETTING_ID,
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
                value: Some(9),
            },
        })
        .expect("prepare initial global claim");
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "global-init",
            scope: NvapiSettingOperationScope::Global,
            target_id: PROFILE,
            setting_id: SETTING_ID,
            expected: NvapiSettingState {
                present: true,
                value: Some(9),
            },
            profile_identity_json: &initial_identity_json,
            composition_json: None,
        })
        .expect("finish initial global claim");

    storage
        .prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "global-subsequent",
            profile_name: PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &initial_identity_json,
            setting_id: SETTING_ID,
            original: NvapiSettingState {
                present: false,
                value: None,
            },
            before: NvapiSettingState {
                present: true,
                value: Some(9),
            },
            after: NvapiSettingState {
                present: true,
                value: Some(9),
            },
        })
        .expect("prepare subsequent global intent with before == after");

    let updated_identity_json = profile_identity_json_with_status(0, 1, true);
    let mut base = observed_profile(PROFILE, Vec::new(), Some((SETTING_ID, true, Some(9))), true);
    base.identity.setting_count = 1;
    let observation = Observation {
        named_profile: Lookup::Found(base.clone()),
        path_profiles: HashMap::new(),
        base_profile: Lookup::Found(base),
    };

    recover_with(&storage, &observation)
        .expect("finalize subsequent global write when before == after");

    assert_eq!(
        storage
            .get_nvapi_global_setting_claim(PROFILE, SETTING_ID)
            .expect("global claim"),
        Some((false, None))
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );

    let next_prep = storage.prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
        op_id: "global-followup",
        profile_name: PROFILE,
        profile_is_predefined: true,
        profile_identity_json: &updated_identity_json,
        setting_id: SETTING_ID,
        original: NvapiSettingState {
            present: false,
            value: None,
        },
        before: NvapiSettingState {
            present: true,
            value: Some(9),
        },
        after: NvapiSettingState {
            present: true,
            value: Some(9),
        },
    });
    assert!(
        next_prep.is_ok(),
        "finalization must record updated target identity in SQLite: {next_prep:?}"
    );
}

#[test]
fn restart_recovery_finalizes_shared_claim_when_before_equals_after_and_preserves_new_reference() {
    let (storage, games) = storage_with_games(&["shared-a", "shared-b"]);
    let path_a = "C:/Games/shared/GameA.exe";
    let path_b = "C:/Games/shared/GameB.exe";
    prepare_external_setting_claim(
        &storage,
        &games[0],
        path_a,
        2,
        (false, None),
        (true, Some(3)),
        "a-init",
    );
    finish_external_setting_claim(&storage, &games[0], 2, (true, Some(3)), "a-init");

    prepare_external_setting_claim(
        &storage,
        &games[1],
        path_b,
        2,
        (true, Some(3)),
        (true, Some(3)),
        "b-reapply",
    );

    let applications = vec![app(path_a), app(path_b)];
    let mut profile = observed_profile(
        PROFILE,
        applications,
        Some((SETTING_ID, true, Some(3))),
        false,
    );
    profile.matched_application = Some(app(path_b));
    let mut observation = observation_for_path(path_b, profile);
    observation.named_profile = Lookup::Found(
        match &observation.path_profiles[&normalized_path_key(path_b)] {
            Lookup::Found(profile) => profile.clone(),
            _ => unreachable!(),
        },
    );

    recover_with(&storage, &observation).expect("finalize shared setting when before == after");

    let b_claims = storage
        .list_nvapi_setting_claims_for_game(&games[1])
        .expect("B claim list");
    assert_eq!(
        b_claims.len(),
        1,
        "Game B's new reference must be preserved on finalization"
    );
    assert!(!b_claims[0].original_present);
    assert_eq!(
        (b_claims[0].expected_present, b_claims[0].expected_value),
        (true, Some(3))
    );

    let a_claims = storage
        .list_nvapi_setting_claims_for_game(&games[0])
        .expect("A claim list");
    assert_eq!(
        a_claims.len(),
        1,
        "Game A's existing reference must be preserved"
    );
    assert!(!a_claims[0].original_present);
    assert_eq!(
        (a_claims[0].expected_present, a_claims[0].expected_value),
        (true, Some(3))
    );

    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(PROFILE, SETTING_ID)
            .expect("ref count"),
        2
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );

    recover_with(&storage, &observation).expect("recovery is idempotent on restart");
    assert_eq!(
        storage
            .list_nvapi_setting_claims_for_game(&games[1])
            .expect("B claim list")
            .len(),
        1
    );
    assert_eq!(
        storage
            .list_nvapi_setting_claims_for_game(&games[0])
            .expect("A claim list")
            .len(),
        1
    );
    assert_eq!(
        storage
            .count_nvapi_setting_claim_refs(PROFILE, SETTING_ID)
            .expect("ref count"),
        2
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn restart_recovery_cancels_pre_save_global_write_when_observed_matches_before() {
    let (storage, _) = storage_with_games(&[]);
    let before = (false, None);
    let after = (true, Some(9));
    storage
        .prepare_nvapi_global_setting_operation(NvapiGlobalSettingPreparation {
            op_id: "global-before-cancel",
            profile_name: PROFILE,
            profile_is_predefined: true,
            profile_identity_json: &profile_identity_json_with_status(0, 0, true),
            setting_id: SETTING_ID,
            original: NvapiSettingState {
                present: before.0,
                value: before.1,
            },
            before: NvapiSettingState {
                present: before.0,
                value: before.1,
            },
            after: NvapiSettingState {
                present: after.0,
                value: after.1,
            },
        })
        .expect("persist global intent");
    let base = observed_profile(PROFILE, Vec::new(), Some((SETTING_ID, false, None)), true);
    let observation = Observation {
        named_profile: Lookup::Found(base.clone()),
        path_profiles: HashMap::new(),
        base_profile: Lookup::Found(base),
    };

    recover_with(&storage, &observation).expect("cancel pre-save global write after restart");

    assert!(
        storage
            .get_nvapi_global_setting_claim(PROFILE, SETTING_ID)
            .expect("global claim")
            .is_none()
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn restart_setting_rejects_changed_witness_even_when_before_equals_after_and_matches_observation() {
    let (storage, games) = storage_with_games(&["setting-witness-mismatch-equality"]);
    let game_id = &games[0];
    let path = "C:/Games/setting-witness-mismatch-equality/Game.exe";
    let op_id = "setting-witness-mismatch-equality";
    prepare_external_setting_claim(
        &storage,
        game_id,
        path,
        1,
        (true, Some(4)),
        (true, Some(4)),
        op_id,
    );
    let mut profile = observed_profile(
        PROFILE,
        vec![app(path)],
        Some((SETTING_ID, true, Some(4))),
        false,
    );
    profile.matched_application = Some(app("C:/Games/other/Other.exe"));
    let observation = Observation {
        named_profile: Lookup::Found(profile.clone()),
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Found(profile))]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("reject unrelated setting witness");

    let rows = storage
        .list_pending_nvapi_operations()
        .expect("read pending operations");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].phase, "conflict");
}

#[test]
fn restart_setting_rejects_owned_composition_mismatch_when_before_equals_after() {
    let (storage, games) = storage_with_games(&["owned-setting-composition"]);
    let game_id = &games[0];
    let path = "C:/Games/owned-setting-composition/Game.exe";
    create_owned_fixture(&storage, game_id, path);

    let op_id = "owned-setting-op";
    let witness = serde_json::to_string(&app(path)).expect("witness json");
    let identity_json = profile_identity_json(1, 0);
    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id,
            game_id,
            profile_name: PROFILE,
            target_kind: "renderpilot",
            profile_is_predefined: false,
            profile_identity_json: &identity_json,
            executable_path: path,
            application_witness_json: &witness,
            setting_id: SETTING_ID,
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
                value: Some(4),
            },
        })
        .expect("persist owned setting receipt");

    let mut profile = observed_profile(
        PROFILE,
        vec![
            app(path),
            app("C:/Games/owned-setting-composition/Extra.exe"),
        ],
        Some((SETTING_ID, true, Some(4))),
        false,
    );
    profile.matched_application = Some(app(path));
    let observation = Observation {
        named_profile: Lookup::Found(profile.clone()),
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Found(profile))]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("reject composition change beyond setting");

    let rows = storage
        .list_pending_nvapi_operations()
        .expect("read pending operations");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].phase, "conflict");
}
