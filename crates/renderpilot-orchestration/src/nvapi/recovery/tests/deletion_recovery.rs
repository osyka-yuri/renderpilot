use super::*;
use crate::nvapi::profiles::{
    DeleteLookup, OwnedProfileDeleteDrs, OwnedProfileObservation, delete_owned_profile_with_drs,
};

fn add_fixture_setting_claim(storage: &SqliteStorage, game_id: &str, path: &str) {
    let op_id = format!("claim-{game_id}");
    let witness = serde_json::to_string(&app(path)).expect("witness json");
    let identity_json = profile_identity_json(1, 0);
    storage
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: &op_id,
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
                value: Some(1),
            },
        })
        .expect("prepare claim");
    storage
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: &op_id,
            scope: NvapiSettingOperationScope::Game { game_id },
            target_id: PROFILE,
            setting_id: SETTING_ID,
            expected: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            profile_identity_json: &identity_json,
            composition_json: None,
        })
        .expect("finish claim");
}

#[test]
fn restart_recovery_confirmed_absent_both_missing_finalizes_deletion() {
    let (storage, games) = storage_with_games(&["del-absent-both-missing"]);
    let game_id = &games[0];
    let path = "C:/Games/del-absent-both-missing/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let owned = observed_profile(PROFILE, vec![app(path)], None, false);
    let before = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "profile_absent": true,
    });
    let op_id = "del-absent-both-missing-op";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist delete intent");

    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Missing)]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("finalize confirmed absent deletion");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_none(),
        "owned profile must be deleted"
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty(),
        "pending operation must be removed"
    );
    assert!(
        storage
            .find_game(&GameId::new(game_id).expect("valid game id"))
            .expect("find game")
            .is_some(),
        "game record must remain in catalog"
    );
}

#[test]
fn restart_recovery_contradictory_missing_by_name_but_matching_by_path_is_blocked() {
    let (storage, games) = storage_with_games(&["del-contradictory-same-name"]);
    let game_id = &games[0];
    let path = "C:/Games/del-contradictory-same-name/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let owned = observed_profile(PROFILE, vec![app(path)], None, false);
    let before = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "profile_absent": true,
    });
    let op_id = "del-contradictory-op";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist delete intent");

    // Contradictory driver observation: missing when queried by name, but present under the same name when queried by executable path.
    let matching_by_path = observed_profile(PROFILE, vec![app(path)], None, false);
    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(
            normalized_path_key(path),
            Lookup::Found(matching_by_path),
        )]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("recovery completes without panic");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_some(),
        "owned profile receipt must be preserved on contradictory observation"
    );
    let pending = storage
        .list_pending_nvapi_operations()
        .expect("pending rows");
    assert_eq!(pending.len(), 1, "pending row must be preserved");
    assert_eq!(
        pending[0].phase, "conflict",
        "contradictory observation must be marked as conflict"
    );
}

#[test]
fn restart_recovery_missing_by_name_and_failed_by_path_leaves_pending() {
    let (storage, games) = storage_with_games(&["del-failed-path"]);
    let game_id = &games[0];
    let path = "C:/Games/del-failed-path/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let owned = observed_profile(PROFILE, vec![app(path)], None, false);
    let before = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "profile_absent": true,
    });
    let op_id = "del-failed-path-op";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist delete intent");

    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(
            normalized_path_key(path),
            Lookup::Failed("EnumApplications failed".to_string()),
        )]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("recovery completes");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_some(),
        "owned profile must be preserved when path lookup failed"
    );
    assert_pending_intent(&storage, op_id);
}

#[test]
fn restart_recovery_missing_by_name_and_ambiguous_by_path_leaves_pending() {
    let (storage, games) = storage_with_games(&["del-ambiguous-path"]);
    let game_id = &games[0];
    let path = "C:/Games/del-ambiguous-path/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let owned = observed_profile(PROFILE, vec![app(path)], None, false);
    let before = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "profile_absent": true,
    });
    let op_id = "del-ambiguous-path-op";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist delete intent");

    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Ambiguous)]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("recovery completes");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_some(),
        "owned profile must be preserved when path lookup is ambiguous"
    );
    assert_pending_intent(&storage, op_id);
}

#[test]
fn restart_recovery_matching_original_profile_cancels_intent() {
    let (storage, games) = storage_with_games(&["del-matching-cancel"]);
    let game_id = &games[0];
    let path = "C:/Games/del-matching-cancel/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let mut owned = observed_profile(PROFILE, vec![app(path)], None, false);
    owned.matched_application = Some(app(path));
    let before = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "profile_absent": true,
    });
    let op_id = "del-matching-cancel-op";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist delete intent");

    let observation = Observation {
        named_profile: Lookup::Found(owned.clone()),
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Found(owned))]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("recovery cancels intent");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_some(),
        "owned profile must be preserved when original profile still exists"
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty(),
        "pending operation must be cancelled"
    );
}

#[test]
fn restart_recovery_altered_profile_marks_conflict() {
    let (storage, games) = storage_with_games(&["del-altered-conflict"]);
    let game_id = &games[0];
    let path = "C:/Games/del-altered-conflict/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let mut owned = observed_profile(PROFILE, vec![app(path)], None, false);
    owned.matched_application = Some(app(path));
    let before = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "profile_absent": true,
    });
    let op_id = "del-altered-conflict-op";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist delete intent");

    let altered = observed_profile(
        PROFILE,
        vec![app(path)],
        Some((0x10f9dc83, true, Some(1))),
        false,
    );
    let observation = Observation {
        named_profile: Lookup::Found(altered.clone()),
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Found(altered))]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("recovery handles altered profile");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_some(),
        "owned profile must be preserved on conflict"
    );
    let pending = storage
        .list_pending_nvapi_operations()
        .expect("pending rows");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].phase, "conflict");
}

#[test]
fn cross_layer_confirmed_absent_retained_intent_not_deleted_by_uncertain_recovery() {
    let (storage, games) = storage_with_games(&["cross-layer-safe"]);
    let game_id = &games[0];
    let path = "C:/Games/cross-layer-safe/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let owned = observed_profile(PROFILE, vec![app(path)], None, false);
    let before = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "profile_absent": true,
    });
    let op_id = "cross-layer-op";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist delete intent");

    // Transient lookup failure prevents proving absence; the pending intent is retained for subsequent recovery.
    let uncertain_obs = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(
            normalized_path_key(path),
            Lookup::Failed("EnumApplications driver timeout".to_string()),
        )]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &uncertain_obs).expect("recovery executes fail-closed");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_some(),
        "owned profile receipt MUST be preserved across recovery uncertainty"
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending ops")
            .iter()
            .any(|row| row.op_id == op_id && row.phase == "intent"),
        "pending intent MUST remain for future recovery"
    );
}

#[test]
fn restart_recovery_confirmed_absent_intent_rejects_unrelated_path_profile() {
    let (storage, games) = storage_with_games(&["del-confirmed-absent-unrelated"]);
    let game_id = &games[0];
    let path = "C:/Games/del-confirmed-absent-unrelated/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let owned = observed_profile(PROFILE, vec![app(path)], None, false);
    let before = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "profile_absent": true,
        "deletion_provenance": "confirmed_absent_v1",
    });
    let op_id = "del-confirmed-absent-unrelated-op";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist delete intent");

    let unrelated = observed_profile("Unrelated Base Profile", vec![app(path)], None, true);
    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Found(unrelated))]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("recovery completes without panic");

    // ConfirmedAbsent requires complete absence; discovering an unrelated profile bound to the path fails into conflict rather than finalizing.
    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_some(),
        "owned profile receipt must be preserved when ConfirmedAbsent encounters unrelated profile at path"
    );
    assert!(
        storage
            .get_nvapi_target_setting_claim(game_id, PROFILE, SETTING_ID)
            .expect("read claim")
            .is_some(),
        "setting claim must be preserved"
    );
    let pending = storage
        .list_pending_nvapi_operations()
        .expect("pending rows");
    assert_eq!(pending.len(), 1, "pending row must be preserved");
    assert_eq!(
        pending[0].phase, "conflict",
        "ConfirmedAbsent encountering path profile must be marked as conflict"
    );
}

struct SequenceDeleteDrs {
    observations: std::sync::Mutex<
        std::collections::VecDeque<Result<OwnedProfileObservation, crate::ServiceError>>,
    >,
}

impl SequenceDeleteDrs {
    fn new(seq: Vec<Result<OwnedProfileObservation, crate::ServiceError>>) -> Self {
        Self {
            observations: std::sync::Mutex::new(std::collections::VecDeque::from(seq)),
        }
    }
}

impl OwnedProfileDeleteDrs for SequenceDeleteDrs {
    fn observe(
        &self,
        _owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
    ) -> Result<OwnedProfileObservation, crate::ServiceError> {
        let mut queue = self.observations.lock().unwrap();
        if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else if let Some(last) = queue.front() {
            last.clone()
        } else {
            Err(crate::ServiceError::command_failed(
                "no observations configured",
            ))
        }
    }

    fn execute_live_delete(
        &self,
        _context: &crate::Context,
        _game_id: &str,
        _owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
        _observation: &OwnedProfileObservation,
    ) -> Result<(), crate::ServiceError> {
        panic!("unexpected live delete in sequence adapter");
    }
}

fn test_setting_context(exe_path: Option<&str>) -> renderpilot_nvapi::setting::SettingContext {
    renderpilot_nvapi::setting::SettingContext {
        game_install_dir: std::path::PathBuf::from("C:/Games/Test"),
        dlls: std::collections::HashMap::new(),
        catalog_readiness: renderpilot_nvapi::setting::CatalogReadiness::NotApplicable,
        effective_exe: exe_path.and_then(|p| {
            std::path::Path::new(p)
                .file_name()
                .and_then(|f| f.to_str())
                .map(str::to_owned)
        }),
        effective_exe_path: exe_path.map(str::to_owned),
    }
}

#[test]
fn production_confirmed_absent_receipt_recovery_safety_pipeline() {
    let (storage, games) = storage_with_games(&["prod-receipt-safety"]);
    let game_id = &games[0];
    let path = "C:/Games/prod-receipt-safety/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let context = crate::Context::from_storage(storage);
    let setting_context = test_setting_context(Some(path));

    let drs = SequenceDeleteDrs::new(vec![
        Ok(OwnedProfileObservation {
            by_name: DeleteLookup::Missing,
            by_path: DeleteLookup::Missing,
        }),
        Err(crate::ServiceError::command_failed(
            "driver communication timeout",
        )),
    ]);

    let res = delete_owned_profile_with_drs(&context, game_id, &setting_context, &drs);
    assert!(
        res.is_err(),
        "delete operation must fail on second observation"
    );

    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("query pending");
    assert_eq!(
        pending.len(),
        1,
        "exactly 1 pending intent must be persisted"
    );
    assert_eq!(pending[0].kind, "delete_profile");
    assert_eq!(pending[0].phase, "intent");

    let after_val: serde_json::Value =
        serde_json::from_str(&pending[0].after_json).expect("valid after json");
    assert_eq!(
        after_val["deletion_provenance"], "confirmed_absent_v1",
        "production receipt must contain the versioned confirmed_absent_v1 discriminator"
    );
    let before_val: serde_json::Value =
        serde_json::from_str(&pending[0].before_json).expect("valid before json");
    assert_eq!(before_val["profile_name"], PROFILE);

    let unrelated = observed_profile("Unrelated Base Profile", vec![app(path)], None, true);
    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Found(unrelated))]),
        base_profile: Lookup::Missing,
    };

    recover_with(context.storage(), &observation).expect("recovery completes without panic");

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_some(),
        "owned profile receipt must be preserved when production ConfirmedAbsent receipt sees path profile"
    );
    assert!(
        context
            .storage()
            .get_nvapi_target_setting_claim(game_id, PROFILE, SETTING_ID)
            .expect("read claim")
            .is_some(),
        "setting claim must be preserved"
    );
    let pending_after = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("pending rows");
    assert_eq!(pending_after.len(), 1, "pending row must be preserved");
    assert_eq!(
        pending_after[0].phase, "conflict",
        "ConfirmedAbsent receipt encountering path profile must be marked conflict"
    );
}

#[test]
fn production_confirmed_absent_receipt_finalizes_on_confirmed_absence() {
    let (storage, games) = storage_with_games(&["prod-receipt-finalizes"]);
    let game_id = &games[0];
    let path = "C:/Games/prod-receipt-finalizes/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let context = crate::Context::from_storage(storage);
    let setting_context = test_setting_context(Some(path));

    let drs = SequenceDeleteDrs::new(vec![
        Ok(OwnedProfileObservation {
            by_name: DeleteLookup::Missing,
            by_path: DeleteLookup::Missing,
        }),
        Err(crate::ServiceError::command_failed("driver disconnected")),
    ]);

    let res = delete_owned_profile_with_drs(&context, game_id, &setting_context, &drs);
    assert!(res.is_err());

    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Missing)]),
        base_profile: Lookup::Missing,
    };

    recover_with(context.storage(), &observation).expect("recovery finalizes ConfirmedAbsent");

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_none(),
        "owned profile must be deleted when absence is confirmed"
    );
    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty(),
        "pending operation must be removed"
    );
}

#[test]
fn unknown_deletion_provenance_fails_closed() {
    let (storage, games) = storage_with_games(&["del-unknown-provenance"]);
    let game_id = &games[0];
    let path = "C:/Games/del-unknown-provenance/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let owned = observed_profile(PROFILE, vec![app(path)], None, false);
    let before = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "binding_path": path,
        "profile_name": PROFILE,
        "profile_absent": true,
        "deletion_provenance": "unknown_future_v99",
    });
    let op_id = "del-unknown-provenance-op";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist delete intent");

    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Missing)]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("recovery completes");

    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .is_some(),
        "ownership must be preserved when provenance is unknown"
    );
    let pending = storage
        .list_pending_nvapi_operations()
        .expect("pending rows");
    assert_eq!(pending.len(), 1, "pending row must be preserved");
    assert_eq!(
        pending[0].phase, "conflict",
        "unknown provenance must be marked as conflict"
    );
}

#[test]
fn interrupted_confirmed_absent_release_blocks_removal_until_recovery_finalization() {
    let (storage, games) = storage_with_games(&["interrupted-absent-removal"]);
    let game_id = &games[0];
    let path = "C:/Games/interrupted-absent-removal/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    add_fixture_setting_claim(&storage, game_id, path);

    let context = crate::Context::from_storage(storage);
    let setting_context = test_setting_context(Some(path));

    let drs = SequenceDeleteDrs::new(vec![
        Ok(OwnedProfileObservation {
            by_name: DeleteLookup::Missing,
            by_path: DeleteLookup::Missing,
        }),
        Err(crate::ServiceError::command_failed("driver disconnected")),
    ]);
    let delete_res = delete_owned_profile_with_drs(&context, game_id, &setting_context, &drs);
    assert!(delete_res.is_err());

    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("query pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, "delete_profile");
    assert_eq!(pending[0].phase, "intent");
    let after_val: serde_json::Value =
        serde_json::from_str(&pending[0].after_json).expect("valid json");
    assert_eq!(after_val["deletion_provenance"], "confirmed_absent_v1");

    let game_id_parsed = renderpilot_domain::GameId::new(game_id).expect("valid game id");
    let remove_err = crate::catalog::remove_game_from_catalog(&context, &game_id_parsed)
        .expect_err("removal must be blocked by pending intent");
    assert!(
        remove_err
            .to_string()
            .contains("a durable DRS operation must be reconciled before removing this game",),
        "error must indicate pending DRS reconciliation: {remove_err}"
    );

    assert!(
        context
            .storage()
            .find_game(&game_id_parsed)
            .expect("find")
            .is_some()
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game_id)
            .expect("owner")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game_id)
            .expect("claims")
            .len(),
        1
    );

    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Missing)]),
        base_profile: Lookup::Missing,
    };
    recover_with(context.storage(), &observation).expect("recovery completes");

    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty()
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game_id)
            .expect("owner")
            .is_none()
    );
    assert!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game_id)
            .expect("claims")
            .is_empty()
    );
    assert!(
        context
            .storage()
            .find_game(&game_id_parsed)
            .expect("find")
            .is_some(),
        "game still in catalog"
    );

    let remove_res = crate::catalog::remove_game_from_catalog(&context, &game_id_parsed);
    assert!(
        remove_res.is_ok(),
        "catalog removal must succeed after recovery: {remove_res:?}"
    );
    assert!(
        context
            .storage()
            .find_game(&game_id_parsed)
            .expect("find")
            .is_none()
    );
}
