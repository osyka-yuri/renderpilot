use renderpilot_application::GameRepository;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    normalized_path_key,
};
use renderpilot_storage_sqlite::{
    NvapiGameSettingPreparation, NvapiGlobalSettingPreparation, NvapiProfileCreationCompletion,
    NvapiSettingOperationCompletion, NvapiSettingOperationScope, NvapiSettingState,
    NvapiVerifiedProfileReceipt,
};

use super::receipts::{
    composition_json, effective_setting_witness_json, exact_owned_lifecycle_witness_for_path,
    exact_owned_lifecycle_witness_json_for_path, profile_receipts,
};
use super::*;

const PROFILE: &str = "RenderPilot Recovery Fixture";
const SETTING_ID: u32 = 0x10B3_292C;
fn profile_identity_json(application_count: u32, setting_count: u32) -> String {
    profile_identity_json_with_status(application_count, setting_count, false)
}

fn profile_identity_json_with_status(
    application_count: u32,
    setting_count: u32,
    is_predefined: bool,
) -> String {
    serde_json::to_string(&ProfileIdentity {
        name: PROFILE.to_owned(),
        is_predefined,
        application_count,
        setting_count,
    })
    .expect("profile identity json")
}

fn storage_with_games(ids: &[&str]) -> (SqliteStorage, Vec<String>) {
    let storage = SqliteStorage::in_memory().expect("in-memory storage");
    let mut game_ids = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let game_id =
            GameId::new(format!("manual:nvapi-recovery-{id}")).expect("valid recovery game id");
        let identity = GameIdentity::new(
            game_id.clone(),
            format!("Recovery game {index}"),
            Launcher::Manual,
        )
        .expect("game identity");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(format!("C:/Games/{id}")).expect("install path"),
        );
        storage.upsert_game(&game).expect("seed game");
        game_ids.push(game_id.to_string());
    }
    (storage, game_ids)
}

fn app(path: &str) -> ApplicationIdentity {
    ApplicationIdentity {
        app_name: path.to_owned(),
        user_friendly_name: "Recovery Fixture".to_owned(),
        launcher: String::new(),
        file_in_folder: String::new(),
        flags: 0,
        command_line: String::new(),
        is_predefined: false,
    }
}

fn observed_profile(
    name: &str,
    applications: Vec<ApplicationIdentity>,
    explicit: Option<(u32, bool, Option<u32>)>,
    is_predefined: bool,
) -> ObservedProfile {
    let settings = explicit
        .filter(|(_, present, _)| *present)
        .map(|(id, _, value)| {
            vec![SettingIdentity {
                id,
                setting_type: 0,
                location: 0,
                is_current_predefined: false,
                is_predefined_valid: false,
                current_value: value.expect("present setting value").to_string(),
                predefined_value: None,
            }]
        })
        .unwrap_or_default();
    ObservedProfile {
        identity: ProfileIdentity {
            name: name.to_owned(),
            is_predefined,
            application_count: applications.len() as u32,
            setting_count: settings.len() as u32,
        },
        matched_application: applications.first().cloned(),
        applications,
        settings,
        explicit_setting: explicit,
    }
}

fn recover_with(storage: &SqliteStorage, observation: &Observation) -> AppResult<()> {
    let rows = storage.list_pending_nvapi_operations()?;
    recover_rows(storage, &rows, |_| Ok(observation.clone()))
}

fn assert_pending_intent(storage: &SqliteStorage, op_id: &str) {
    let rows = storage
        .list_pending_nvapi_operations()
        .expect("read pending operations");
    let row = rows
        .iter()
        .find(|row| row.op_id == op_id)
        .expect("uncertain operation remains journaled");
    assert_eq!(row.phase, "intent");
}

fn observation_for_path(path: &str, profile: ObservedProfile) -> Observation {
    Observation {
        named_profile: Lookup::Found(profile.clone()),
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Found(profile))]),
        base_profile: Lookup::Missing,
    }
}

fn begin_create(storage: &SqliteStorage, game_id: &str, path: &str, op_id: &str) {
    let after = serde_json::json!({ "application_path": path }).to_string();
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "create_profile",
            "{}",
            &after,
        )
        .expect("persist profile creation intent");
}

fn create_owned_fixture(storage: &SqliteStorage, game_id: &str, path: &str) {
    let profile = observed_profile(PROFILE, vec![app(path)], None, false);
    let witness = profile
        .matched_application
        .as_ref()
        .expect("profile application witness");
    let (identity_json, witness_json, composition) =
        profile_receipts(&profile, witness).expect("profile receipts");
    let op_id = format!("create-owned-{}", game_id.replace(':', "-"));
    begin_create(storage, game_id, path, &op_id);
    storage
        .complete_nvapi_profile_creation(NvapiProfileCreationCompletion {
            op_id: &op_id,
            game_id,
            binding_path: path,
            profile: NvapiVerifiedProfileReceipt {
                profile_name: PROFILE,
                profile_identity_json: &identity_json,
                application_witness_json: &witness_json,
                composition_json: &composition,
            },
        })
        .expect("persist owned-profile fixture");
}

#[test]
fn restart_recovery_cancels_create_that_never_reached_save() {
    let (storage, games) = storage_with_games(&["before-save"]);
    let path = "C:/Games/before-save/Game.exe";
    begin_create(&storage, &games[0], path, "create-before-save");
    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Missing)]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("cancel proven before-state");

    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );
    assert!(
        storage
            .get_nvapi_owned_profile(&games[0])
            .expect("owner receipt")
            .is_none()
    );
}

#[test]
fn restart_recovery_finalizes_created_profile_and_is_idempotent() {
    let (storage, games) = storage_with_games(&["after-save"]);
    let path = "C:/Games/after-save/Game.exe";
    begin_create(&storage, &games[0], path, "create-after-save");
    let profile = observed_profile(PROFILE, vec![app(path)], None, false);

    recover_with(&storage, &observation_for_path(path, profile.clone()))
        .expect("finalize saved create after restart");
    recover_with(&storage, &observation_for_path(path, profile))
        .expect("second restart finds no unresolved operation");

    let owner = storage
        .get_nvapi_owned_profile(&games[0])
        .expect("read owner")
        .expect("created ownership receipt");
    assert_eq!(owner.binding_path, path);
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn restart_create_requires_the_full_path_lookup_witness_to_match_the_binding() {
    let (storage, games) = storage_with_games(&["create-witness-mismatch"]);
    let path = "C:/Games/create-witness-mismatch/Game.exe";
    let op_id = "create-witness-mismatch";
    begin_create(&storage, &games[0], path, op_id);
    let mut profile = observed_profile(PROFILE, vec![app(path)], None, false);
    profile.matched_application = Some(app("C:/Games/other/Other.exe"));

    recover_with(&storage, &observation_for_path(path, profile))
        .expect("reject an unrelated full-path lookup witness");

    let rows = storage
        .list_pending_nvapi_operations()
        .expect("read pending operations");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].phase, "conflict");
    assert!(
        storage
            .get_nvapi_owned_profile(&games[0])
            .expect("read owner")
            .is_none()
    );
}

#[test]
fn restart_recovery_keeps_create_intent_when_identity_or_enum_observation_fails() {
    let (storage, games) = storage_with_games(&["create-uncertain"]);
    let path = "C:/Games/create-uncertain/Game.exe";
    let op_id = "create-uncertain-observation";
    begin_create(&storage, &games[0], path, op_id);
    let observation = Observation {
        named_profile: Lookup::Failed("GetProfileInfo identity read failed".to_owned()),
        path_profiles: HashMap::from([(
            normalized_path_key(path),
            Lookup::Failed("EnumApplications failed".to_owned()),
        )]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("uncertain create stays recoverable");

    assert_pending_intent(&storage, op_id);
    assert!(
        storage
            .get_nvapi_owned_profile(&games[0])
            .expect("owner")
            .is_none()
    );
}

#[test]
fn restart_recovery_deletes_owned_profile_when_nvidia_exposes_another_profile() {
    let (storage, games) = storage_with_games(&["delete-after-save"]);
    let path = "C:/Games/delete-after-save/Game.exe";
    create_owned_fixture(&storage, &games[0], path);
    let owned = observed_profile(PROFILE, vec![app(path)], None, false);
    let before = serde_json::json!({
        "binding_path": path,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    storage
        .begin_nvapi_operation(
            "delete-after-save",
            Some(&games[0]),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            "{}",
        )
        .expect("persist delete intent");
    let exposed = observed_profile("NVIDIA predefined profile", vec![app(path)], None, true);
    let observation = Observation {
        named_profile: Lookup::Missing,
        path_profiles: HashMap::from([(normalized_path_key(path), Lookup::Found(exposed))]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("finalize exact profile deletion");

    assert!(
        storage
            .get_nvapi_owned_profile(&games[0])
            .expect("read owner")
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
fn restart_recovery_keeps_delete_intent_when_enum_observation_fails() {
    let (storage, games) = storage_with_games(&["delete-uncertain"]);
    let game_id = &games[0];
    let path = "C:/Games/delete-uncertain/Game.exe";
    create_owned_fixture(&storage, game_id, path);
    let owned = observed_profile(PROFILE, vec![app(path)], None, false);
    let before = serde_json::json!({
        "binding_path": path,
        "composition": composition_json(&owned).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&owned, path)
            .expect("witness"),
    });
    let op_id = "delete-uncertain-observation";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "delete_profile",
            &before.to_string(),
            "{}",
        )
        .expect("persist delete intent");
    let observation = Observation {
        named_profile: Lookup::Failed("EnumSettings failed".to_owned()),
        path_profiles: HashMap::from([(
            normalized_path_key(path),
            Lookup::Failed("profile identity could not be confirmed".to_owned()),
        )]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("uncertain delete stays recoverable");

    assert_pending_intent(&storage, op_id);
    assert!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("owner")
            .is_some()
    );
}

#[test]
fn restart_recovery_finalizes_move_and_restores_selector_intent() {
    let (storage, games) = storage_with_games(&["move-after-save"]);
    let game_id = &games[0];
    let old_path = "C:/Games/move-after-save/Game.exe";
    let new_path = "C:/Games/move-after-save/Selected.exe";
    create_owned_fixture(&storage, game_id, old_path);
    let old_profile = observed_profile(PROFILE, vec![app(old_path)], None, false);
    let before = serde_json::json!({
        "old_path": old_path,
        "composition": composition_json(&old_profile).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&old_profile, old_path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "new_path": new_path,
        "new_basename": "Selected.exe",
        "select_automatically": false,
    });
    storage
        .begin_nvapi_operation(
            "move-after-save",
            Some(game_id),
            Some(PROFILE),
            "move_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist move intent");
    let moved = observed_profile(PROFILE, vec![app(new_path)], None, false);
    let observation = Observation {
        named_profile: Lookup::Found(moved.clone()),
        path_profiles: HashMap::from([
            (normalized_path_key(old_path), Lookup::Missing),
            (normalized_path_key(new_path), Lookup::Found(moved)),
        ]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("finalize saved move after restart");

    assert_eq!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .expect("owner receipt")
            .binding_path,
        new_path
    );
    assert_eq!(
        storage
            .get_nvapi_executable_override(game_id)
            .expect("read shared executable selection")
            .expect("selection restored from durable intent")
            .selected_path,
        new_path
    );
    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn restart_move_cancels_exact_before_state_with_witness_from_path_lookup() {
    let (storage, games) = storage_with_games(&["move-before-save-path-witness"]);
    let game_id = &games[0];
    let old_path = "C:/Games/move-before-save-path-witness/Game.exe";
    let new_path = "C:/Games/move-before-save-path-witness/Selected.exe";
    create_owned_fixture(&storage, game_id, old_path);
    let by_old_path = observed_profile(PROFILE, vec![app(old_path)], None, false);
    let before = serde_json::json!({
        "old_path": old_path,
        "composition": composition_json(&by_old_path).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&by_old_path, old_path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "new_path": new_path,
        "new_basename": "Selected.exe",
        "select_automatically": false,
    });
    let op_id = "move-before-save-path-witness";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "move_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist move intent");
    let mut named = by_old_path.clone();
    named.matched_application = None;
    let observation = Observation {
        named_profile: Lookup::Found(named),
        path_profiles: HashMap::from([
            (normalized_path_key(old_path), Lookup::Found(by_old_path)),
            (normalized_path_key(new_path), Lookup::Missing),
        ]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("cancel proven pre-save move");

    assert!(
        storage
            .list_pending_nvapi_operations()
            .expect("read pending operations")
            .is_empty()
    );
    assert_eq!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .expect("existing owner receipt")
            .binding_path,
        old_path
    );
}

#[test]
fn restart_move_requires_old_path_absence_and_retains_uncertain_observations() {
    enum OldPathState {
        Missing,
        Owned,
        Ambiguous,
        Failed,
    }

    for (id, old_state, expected_phase) in [
        ("missing", OldPathState::Missing, None),
        ("owned", OldPathState::Owned, Some("conflict")),
        ("ambiguous", OldPathState::Ambiguous, Some("intent")),
        ("failed", OldPathState::Failed, Some("intent")),
    ] {
        let (storage, games) = storage_with_games(&[id]);
        let game_id = &games[0];
        let old_path = format!("C:/Games/{id}/Game.exe");
        let new_path = format!("C:/Games/{id}/Selected.exe");
        create_owned_fixture(&storage, game_id, &old_path);
        let old_profile = observed_profile(PROFILE, vec![app(&old_path)], None, false);
        let before = serde_json::json!({
            "old_path": old_path,
            "composition": composition_json(&old_profile).expect("composition"),
            "application_witness": exact_owned_lifecycle_witness_json_for_path(
                &old_profile,
                &old_path,
            )
            .expect("witness"),
        });
        let after = serde_json::json!({
            "new_path": new_path,
            "new_basename": "Selected.exe",
            "select_automatically": false,
        });
        let op_id = format!("move-old-path-{id}");
        storage
            .begin_nvapi_operation(
                &op_id,
                Some(game_id),
                Some(PROFILE),
                "move_profile",
                &before.to_string(),
                &after.to_string(),
            )
            .expect("persist move intent");
        let moved = observed_profile(PROFILE, vec![app(&new_path)], None, false);
        let old_lookup = match old_state {
            OldPathState::Missing => Lookup::Missing,
            OldPathState::Owned => Lookup::Found(moved.clone()),
            OldPathState::Ambiguous => Lookup::Ambiguous,
            OldPathState::Failed => Lookup::Failed("old path lookup failed".to_owned()),
        };
        let observation = Observation {
            named_profile: Lookup::Found(moved.clone()),
            path_profiles: HashMap::from([
                (normalized_path_key(&old_path), old_lookup),
                (normalized_path_key(&new_path), Lookup::Found(moved)),
            ]),
            base_profile: Lookup::Missing,
        };

        recover_with(&storage, &observation).expect("reconcile move from fresh observation");

        let rows = storage
            .list_pending_nvapi_operations()
            .expect("read pending operations");
        match expected_phase {
            None => {
                assert!(rows.is_empty(), "missing old path proves the move");
                assert_eq!(
                    storage
                        .get_nvapi_owned_profile(game_id)
                        .expect("read owner")
                        .expect("moved owner")
                        .binding_path,
                    new_path
                );
            }
            Some(phase) => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].phase, phase);
                assert_eq!(
                    storage
                        .get_nvapi_owned_profile(game_id)
                        .expect("read owner")
                        .expect("owner remains on old path")
                        .binding_path,
                    old_path
                );
            }
        }
    }
}

#[test]
fn restart_move_requires_the_full_path_lookup_witness_to_match_the_destination() {
    let (storage, games) = storage_with_games(&["move-witness-mismatch"]);
    let game_id = &games[0];
    let old_path = "C:/Games/move-witness-mismatch/Game.exe";
    let new_path = "C:/Games/move-witness-mismatch/Selected.exe";
    create_owned_fixture(&storage, game_id, old_path);
    let old_profile = observed_profile(PROFILE, vec![app(old_path)], None, false);
    let before = serde_json::json!({
        "old_path": old_path,
        "composition": composition_json(&old_profile).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&old_profile, old_path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "new_path": new_path,
        "new_basename": "Selected.exe",
        "select_automatically": false,
    });
    let op_id = "move-witness-mismatch";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "move_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist move intent");
    let mut moved = observed_profile(PROFILE, vec![app(new_path)], None, false);
    moved.matched_application = Some(app("C:/Games/other/Other.exe"));
    let observation = Observation {
        named_profile: Lookup::Found(moved.clone()),
        path_profiles: HashMap::from([
            (normalized_path_key(old_path), Lookup::Missing),
            (normalized_path_key(new_path), Lookup::Found(moved)),
        ]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("reject unrelated destination witness");

    let rows = storage
        .list_pending_nvapi_operations()
        .expect("read pending operations");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].phase, "conflict");
    assert_eq!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("read owner")
            .expect("owner remains on old path")
            .binding_path,
        old_path
    );
}

#[test]
fn restart_recovery_keeps_move_intent_when_destination_lookup_is_ambiguous() {
    let (storage, games) = storage_with_games(&["move-uncertain"]);
    let game_id = &games[0];
    let old_path = "C:/Games/move-uncertain/Game.exe";
    let new_path = "C:/Games/move-uncertain/Selected.exe";
    create_owned_fixture(&storage, game_id, old_path);
    let old_profile = observed_profile(PROFILE, vec![app(old_path)], None, false);
    let before = serde_json::json!({
        "old_path": old_path,
        "composition": composition_json(&old_profile).expect("composition"),
        "application_witness": exact_owned_lifecycle_witness_json_for_path(&old_profile, old_path)
            .expect("witness"),
    });
    let after = serde_json::json!({
        "new_path": new_path,
        "new_basename": "Selected.exe",
        "select_automatically": false,
    });
    let op_id = "move-uncertain-destination";
    storage
        .begin_nvapi_operation(
            op_id,
            Some(game_id),
            Some(PROFILE),
            "move_profile",
            &before.to_string(),
            &after.to_string(),
        )
        .expect("persist move intent");
    let observation = Observation {
        named_profile: Lookup::Found(old_profile.clone()),
        path_profiles: HashMap::from([
            (normalized_path_key(old_path), Lookup::Found(old_profile)),
            (normalized_path_key(new_path), Lookup::Ambiguous),
        ]),
        base_profile: Lookup::Missing,
    };

    recover_with(&storage, &observation).expect("ambiguous destination stays recoverable");

    assert_pending_intent(&storage, op_id);
    assert_eq!(
        storage
            .get_nvapi_owned_profile(game_id)
            .expect("owner")
            .expect("existing owner receipt")
            .binding_path,
        old_path
    );
}

mod deletion_recovery;
mod setting_recovery;
