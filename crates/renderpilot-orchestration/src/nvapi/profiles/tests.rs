use super::{
    ProfileTransition, classify_profile_transition, owned_profile_receipt_matches,
    selected_executable_allows_owned_delete,
};
use renderpilot_application::GameRepository;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};
use renderpilot_nvapi::{ApplicationIdentity, ProfileIdentity};
use renderpilot_storage_sqlite::{NvapiOwnedProfileRow, SqliteStorage};

fn storage_with_profile_intent(kind: &str) -> SqliteStorage {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new("manual:profile-transition-test").expect("game id");
    let identity = GameIdentity::new(game_id.clone(), "Profile Transition", Launcher::Manual)
        .expect("game identity");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/ProfileTransition").expect("install path"),
    );
    storage.upsert_game(&game).expect("persist game");
    storage
        .begin_nvapi_operation(
            "profile-transition-op",
            Some(game_id.as_str()),
            Some("RenderPilot profile"),
            kind,
            "{}",
            "{}",
        )
        .expect("persist profile intent");
    storage
}

fn assert_uncertain_observation_keeps_intent(kind: &str) {
    let storage = storage_with_profile_intent(kind);
    assert_eq!(
        classify_profile_transition(None, Some(false)),
        ProfileTransition::Pending,
        "an unavailable DRS observation must not choose a durable transition"
    );
    let pending = storage
        .list_pending_nvapi_operations()
        .expect("read pending intent");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].phase, "intent");
}

#[test]
fn uncertain_create_identity_read_keeps_journal_at_intent() {
    assert_uncertain_observation_keeps_intent("create_profile");
}

#[test]
fn uncertain_delete_enum_read_keeps_journal_at_intent() {
    assert_uncertain_observation_keeps_intent("delete_profile");
}

#[test]
fn uncertain_move_destination_lookup_keeps_journal_at_intent() {
    assert_uncertain_observation_keeps_intent("move_profile");
}

#[test]
fn profile_transition_classifier_requires_one_exact_state() {
    assert_eq!(
        classify_profile_transition(Some(true), Some(false)),
        ProfileTransition::Before
    );
    assert_eq!(
        classify_profile_transition(Some(false), Some(true)),
        ProfileTransition::After
    );
    assert_eq!(
        classify_profile_transition(Some(false), Some(false)),
        ProfileTransition::Conflict
    );
    assert_eq!(
        classify_profile_transition(Some(true), Some(true)),
        ProfileTransition::Pending
    );
}

fn owned_receipt(path: &str, witness_json: &str) -> NvapiOwnedProfileRow {
    NvapiOwnedProfileRow {
        game_id: "manual:game".to_owned(),
        profile_name: "RenderPilot - Game [receipt]".to_owned(),
        binding_path: path.to_owned(),
        application_witness_json: witness_json.to_owned(),
        composition_json: "{\"applications\":[],\"settings\":[]}".to_owned(),
        state: "active".to_owned(),
        updated_at: 0,
    }
}

fn app_witness(path: &str) -> ApplicationIdentity {
    ApplicationIdentity {
        app_name: path.to_owned(),
        user_friendly_name: "Game".to_owned(),
        launcher: String::new(),
        file_in_folder: String::new(),
        flags: 0,
        command_line: String::new(),
        is_predefined: false,
    }
}

fn identity() -> ProfileIdentity {
    ProfileIdentity {
        name: "RenderPilot - Game [receipt]".to_owned(),
        is_predefined: false,
        application_count: 1,
        setting_count: 0,
    }
}

#[test]
fn exact_receipt_allows_deletion_when_bound_executable_is_missing() {
    let path = "Z:/removed-install/Game.exe";
    let witness = app_witness(path);
    let witness_json = serde_json::to_string(&witness).expect("witness JSON");
    let owner = owned_receipt(path, &witness_json);
    let composition = owner.composition_json.clone();

    assert!(owned_profile_receipt_matches(
        &owner,
        &identity(),
        &witness,
        &witness_json,
        &composition,
    ));
}

#[test]
fn changed_binding_or_profile_composition_blocks_deletion() {
    let path = "Z:/removed-install/Game.exe";
    let witness = app_witness(path);
    let witness_json = serde_json::to_string(&witness).expect("witness JSON");
    let owner = owned_receipt(path, &witness_json);

    let drifted_witness = app_witness("Z:/other-install/Game.exe");
    let drifted_witness_json = serde_json::to_string(&drifted_witness).expect("witness JSON");
    assert!(!owned_profile_receipt_matches(
        &owner,
        &identity(),
        &drifted_witness,
        &drifted_witness_json,
        &owner.composition_json,
    ));
    assert!(!owned_profile_receipt_matches(
        &owner,
        &identity(),
        &witness,
        &witness_json,
        "changed composition",
    ));
}

#[test]
fn other_selected_executable_allows_only_verified_deletion_of_missing_binding() {
    let directory = tempfile::tempdir().expect("temporary game folder");
    let missing_binding = directory.path().join("removed-game.exe");
    let selected_executable = directory.path().join("launcher.exe");
    let existing_binding = directory.path().join("game.exe");
    std::fs::write(&selected_executable, b"leave this file untouched")
        .expect("create alternate executable");
    std::fs::write(&existing_binding, b"owned executable still exists")
        .expect("create existing owned executable");
    let missing_binding = missing_binding.to_string_lossy().into_owned();
    let selected_executable = selected_executable.to_string_lossy().into_owned();
    let existing_binding = existing_binding.to_string_lossy().into_owned();

    assert!(
        selected_executable_allows_owned_delete(Some(&selected_executable), &missing_binding,)
            .expect("missing owner executable is confirmed")
    );
    assert!(
        !selected_executable_allows_owned_delete(Some(&selected_executable), &existing_binding,)
            .expect("existing owner executable blocks bypass")
    );

    let witness = app_witness(&missing_binding);
    let witness_json = serde_json::to_string(&witness).expect("witness JSON");
    let owner = owned_receipt(&missing_binding, &witness_json);
    let drifted = app_witness("C:/unrelated/game.exe");
    let drifted_json = serde_json::to_string(&drifted).expect("drifted witness JSON");
    assert!(!owned_profile_receipt_matches(
        &owner,
        &identity(),
        &drifted,
        &drifted_json,
        &owner.composition_json,
    ));
    assert!(!std::path::Path::new(&missing_binding).exists());
    assert_eq!(
        std::fs::read(&selected_executable).expect("alternate executable remains unchanged"),
        b"leave this file untouched",
    );
}

use super::delete::{
    Lookup, ObservedProfileReceipt, OwnedProfileDeleteDrs, OwnedProfileObservation,
    delete_owned_profile_with_drs, evaluate_delete_preconditions,
};
use crate::Context;
use renderpilot_nvapi::setting::SettingContext;

struct SyntheticOwnedProfileDeleteDrs<'a> {
    observation: OwnedProfileObservation,
    observations: std::sync::Mutex<
        std::collections::VecDeque<Result<OwnedProfileObservation, crate::ServiceError>>,
    >,
    live_delete_attempted: std::sync::atomic::AtomicBool,
    mutation_attempted: std::sync::atomic::AtomicBool,
    observe_call_count: std::sync::atomic::AtomicUsize,
    on_observe: Option<Box<dyn Fn(usize) + Send + Sync + 'a>>,
}

impl<'a> SyntheticOwnedProfileDeleteDrs<'a> {
    fn new(observation: OwnedProfileObservation) -> Self {
        Self {
            observation: observation.clone(),
            observations: std::sync::Mutex::new(std::collections::VecDeque::from([Ok(
                observation,
            )])),
            live_delete_attempted: std::sync::atomic::AtomicBool::new(false),
            mutation_attempted: std::sync::atomic::AtomicBool::new(false),
            observe_call_count: std::sync::atomic::AtomicUsize::new(0),
            on_observe: None,
        }
    }

    fn with_sequence(sequence: Vec<Result<OwnedProfileObservation, crate::ServiceError>>) -> Self {
        let first = sequence
            .first()
            .and_then(|r| r.as_ref().ok())
            .cloned()
            .unwrap_or(OwnedProfileObservation {
                by_name: Lookup::Missing,
                by_path: Lookup::Missing,
            });
        Self {
            observation: first,
            observations: std::sync::Mutex::new(std::collections::VecDeque::from(sequence)),
            live_delete_attempted: std::sync::atomic::AtomicBool::new(false),
            mutation_attempted: std::sync::atomic::AtomicBool::new(false),
            observe_call_count: std::sync::atomic::AtomicUsize::new(0),
            on_observe: None,
        }
    }

    fn with_hook(
        observation: OwnedProfileObservation,
        on_observe: Box<dyn Fn(usize) + Send + Sync + 'a>,
    ) -> Self {
        Self {
            observation: observation.clone(),
            observations: std::sync::Mutex::new(std::collections::VecDeque::from([Ok(
                observation,
            )])),
            live_delete_attempted: std::sync::atomic::AtomicBool::new(false),
            mutation_attempted: std::sync::atomic::AtomicBool::new(false),
            observe_call_count: std::sync::atomic::AtomicUsize::new(0),
            on_observe: Some(on_observe),
        }
    }
}

impl OwnedProfileDeleteDrs for SyntheticOwnedProfileDeleteDrs<'_> {
    fn observe(
        &self,
        _owner: &NvapiOwnedProfileRow,
    ) -> Result<OwnedProfileObservation, crate::ServiceError> {
        let count = self
            .observe_call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if let Some(hook) = &self.on_observe {
            hook(count);
        }
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
        context: &Context,
        game_id: &str,
        owner: &NvapiOwnedProfileRow,
        _observation: &OwnedProfileObservation,
    ) -> Result<(), crate::ServiceError> {
        self.live_delete_attempted
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let before = serde_json::json!({
            "binding_path": owner.binding_path,
            "profile_name": owner.profile_name,
            "composition": owner.composition_json,
            "application_witness": owner.application_witness_json,
        })
        .to_string();
        let after = serde_json::json!({
            "binding_path": owner.binding_path,
            "profile_name": owner.profile_name,
            "profile_absent": true
        })
        .to_string();
        let op_id = ulid::Ulid::generate().to_string();
        context.storage().begin_nvapi_operation(
            &op_id,
            Some(game_id),
            Some(&owner.profile_name),
            "delete_profile",
            &before,
            &after,
        )?;
        self.mutation_attempted
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

fn setup_game_with_owned_profile(
    game_id_str: &str,
    binding_path: &str,
) -> (Context, NvapiOwnedProfileRow) {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game_id = GameId::new(game_id_str).expect("game id");
    let identity =
        GameIdentity::new(game_id.clone(), "Game Title", Launcher::Manual).expect("game identity");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Test").expect("install path"),
    );
    storage.upsert_game(&game).expect("persist game");

    let witness = app_witness(binding_path);
    let witness_json = serde_json::to_string(&witness).expect("witness JSON");
    let profile_name = format!("RenderPilot - {game_id_str} [receipt]");
    let composition_json = "{\"applications\":[],\"settings\":[]}".to_owned();

    storage
        .begin_nvapi_operation(
            "create-op",
            Some(game_id.as_str()),
            Some(&profile_name),
            "create_profile",
            "{}",
            "{}",
        )
        .expect("begin create");

    let profile_identity = ProfileIdentity {
        name: profile_name.clone(),
        is_predefined: false,
        application_count: 1,
        setting_count: 0,
    };
    let profile_identity_json =
        serde_json::to_string(&profile_identity).expect("profile identity JSON");

    storage
        .complete_nvapi_profile_creation(
            renderpilot_storage_sqlite::NvapiProfileCreationCompletion {
                op_id: "create-op",
                game_id: game_id.as_str(),
                binding_path,
                profile: renderpilot_storage_sqlite::NvapiVerifiedProfileReceipt {
                    profile_name: &profile_name,
                    profile_identity_json: &profile_identity_json,
                    application_witness_json: &witness_json,
                    composition_json: &composition_json,
                },
            },
        )
        .expect("complete profile creation");

    let row = storage
        .get_nvapi_owned_profile(game_id_str)
        .expect("read owned profile")
        .expect("owned profile exists");

    (Context::from_storage(storage), row)
}

fn add_game_setting_claim(
    context: &Context,
    game_id: &str,
    owner: &NvapiOwnedProfileRow,
    setting_id: u32,
    setting_value: u32,
) {
    let profile_identity = ProfileIdentity {
        name: owner.profile_name.clone(),
        is_predefined: false,
        application_count: 1,
        setting_count: 0,
    };
    let profile_identity_json =
        serde_json::to_string(&profile_identity).expect("profile identity JSON");

    context
        .storage()
        .prepare_nvapi_setting_operation(renderpilot_storage_sqlite::NvapiGameSettingPreparation {
            op_id: "claim-init-op",
            game_id,
            profile_name: &owner.profile_name,
            target_kind: "renderpilot",
            profile_is_predefined: false,
            profile_identity_json: &profile_identity_json,
            executable_path: &owner.binding_path,
            application_witness_json: &owner.application_witness_json,
            setting_id,
            original: renderpilot_storage_sqlite::NvapiSettingState {
                present: false,
                value: None,
            },
            before: renderpilot_storage_sqlite::NvapiSettingState {
                present: false,
                value: None,
            },
            after: renderpilot_storage_sqlite::NvapiSettingState {
                present: true,
                value: Some(setting_value),
            },
        })
        .expect("prepare setting operation");

    context
        .storage()
        .finish_nvapi_setting_operation(
            renderpilot_storage_sqlite::NvapiSettingOperationCompletion {
                op_id: "claim-init-op",
                scope: renderpilot_storage_sqlite::NvapiSettingOperationScope::Game { game_id },
                target_id: &owner.profile_name,
                setting_id,
                expected: renderpilot_storage_sqlite::NvapiSettingState {
                    present: true,
                    value: Some(setting_value),
                },
                profile_identity_json: &profile_identity_json,
                composition_json: None,
            },
        )
        .expect("finish setting operation");
}

fn matching_receipt(owner: &NvapiOwnedProfileRow) -> ObservedProfileReceipt {
    let witness: ApplicationIdentity =
        serde_json::from_str(&owner.application_witness_json).expect("valid witness json");
    ObservedProfileReceipt {
        identity: ProfileIdentity {
            name: owner.profile_name.clone(),
            is_predefined: false,
            application_count: 1,
            setting_count: 0,
        },
        witness: Some(witness.clone()),
        witness_json: Some(owner.application_witness_json.clone()),
        applications: vec![witness],
        settings: vec![],
        composition: owner.composition_json.clone(),
    }
}

fn matching_observation(owner: &NvapiOwnedProfileRow) -> OwnedProfileObservation {
    let receipt = matching_receipt(owner);
    OwnedProfileObservation {
        by_name: Lookup::Found(ObservedProfileReceipt {
            identity: receipt.identity.clone(),
            witness: None,
            witness_json: None,
            applications: receipt.applications.clone(),
            settings: receipt.settings.clone(),
            composition: receipt.composition.clone(),
        }),
        by_path: Lookup::Found(receipt),
    }
}

fn test_setting_context(exe_path: Option<&str>) -> SettingContext {
    SettingContext {
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
fn characterization_owned_delete_absent_profile_returns_executable_not_found() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let receipt = matching_receipt(&owner);
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Found(receipt),
        by_path: Lookup::Missing,
    });
    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "unhooked executable must return Err: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("DRS executable not found") || err.contains("-166"),
        "error must match ExecutableNotFound: {err}"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn characterization_owned_delete_ambiguous_lookup_returns_executable_ambiguous() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Ambiguous,
    });
    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "ambiguous lookup must return Err: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("DRS executable lookup is ambiguous") || err.contains("-182"),
        "error must match ExecutableAmbiguous: {err}"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn characterization_owned_delete_read_error_returns_err_without_mutation() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Failed("driver communication timeout".to_owned()),
    });
    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "driver lookup failure must return Err: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("driver communication timeout"),
        "error must convey driver failure: {err}"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn characterization_owned_delete_matching_profile_invokes_live_delete() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(matching_observation(&owner));

    assert!(
        evaluate_delete_preconditions(
            &context,
            &owner.game_id,
            &setting_context,
            &owner,
            &drs.observation
        )
        .is_ok(),
        "preconditions must succeed for matching profile"
    );

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_ok(),
        "delete with matching profile must succeed: {result:?}"
    );
    assert!(
        drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(
        drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn characterization_owned_delete_composition_changed_blocks_deletion() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let mut observation = matching_observation(&owner);
    if let Lookup::Found(receipt) = &mut observation.by_path {
        receipt.composition = "{\"applications\":[],\"settings\":[{\"id\":123}]}".to_owned();
    }
    let drs = SyntheticOwnedProfileDeleteDrs::new(observation);
    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(result.is_err(), "changed composition must fail: {result:?}");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("RenderPilot-owned NVIDIA profile changed"),
        "must fail with conflict: {err}"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn characterization_owned_delete_witness_changed_blocks_deletion() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let mut observation = matching_observation(&owner);
    if let Lookup::Found(receipt) = &mut observation.by_path {
        receipt.witness = Some(ApplicationIdentity {
            app_name: "C:/Games/Test/other.exe".to_owned(),
            user_friendly_name: "Other".to_owned(),
            launcher: "Epic".to_owned(),
            file_in_folder: String::new(),
            flags: 0,
            command_line: String::new(),
            is_predefined: false,
        });
        receipt.witness_json = Some("{\"drifted\":true}".to_owned());
    }
    let drs = SyntheticOwnedProfileDeleteDrs::new(observation);
    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(result.is_err(), "changed witness must fail: {result:?}");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("RenderPilot-owned NVIDIA profile changed"),
        "must fail with conflict: {err}"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn characterization_owned_delete_active_executable_mismatch_blocks_deletion() {
    let directory = tempfile::tempdir().expect("temp dir");
    let binding_file = directory.path().join("game.exe");
    std::fs::write(&binding_file, b"executable content").expect("write exe");
    let binding_path = binding_file.to_string_lossy().replace('\\', "/");

    let (context, owner) = setup_game_with_owned_profile("manual:test-game", &binding_path);
    let other_exe = directory.path().join("launcher.exe");
    std::fs::write(&other_exe, b"launcher content").expect("write launcher");
    let other_path = other_exe.to_string_lossy().replace('\\', "/");

    let setting_context = test_setting_context(Some(&other_path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(matching_observation(&owner));
    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(result.is_err(), "active exe mismatch must fail: {result:?}");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("select that executable before deleting it"),
        "must require selecting bound executable: {err}"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn characterization_owned_delete_other_game_claim_blocks_deletion() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let game_id2 = GameId::new("manual:other-game").expect("game id 2");
    let identity2 =
        GameIdentity::new(game_id2, "Other Game", Launcher::Manual).expect("identity 2");
    let game2 = GameInstallation::new(
        identity2,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Other").expect("install path"),
    );
    context
        .storage()
        .upsert_game(&game2)
        .expect("persist game 2");

    let other_path = "C:/Games/Other/other.exe";
    let other_witness = app_witness(other_path);
    let other_witness_json = serde_json::to_string(&other_witness).expect("witness JSON");
    let profile_identity = ProfileIdentity {
        name: owner.profile_name.clone(),
        is_predefined: false,
        application_count: 1,
        setting_count: 0,
    };
    let profile_identity_json =
        serde_json::to_string(&profile_identity).expect("profile identity JSON");

    context
        .storage()
        .prepare_nvapi_setting_operation(renderpilot_storage_sqlite::NvapiGameSettingPreparation {
            op_id: "claim-op",
            game_id: "manual:other-game",
            profile_name: &owner.profile_name,
            target_kind: "renderpilot",
            profile_is_predefined: false,
            profile_identity_json: &profile_identity_json,
            executable_path: other_path,
            application_witness_json: &other_witness_json,
            setting_id: 0x10f9dc83,
            original: renderpilot_storage_sqlite::NvapiSettingState {
                present: false,
                value: None,
            },
            before: renderpilot_storage_sqlite::NvapiSettingState {
                present: false,
                value: None,
            },
            after: renderpilot_storage_sqlite::NvapiSettingState {
                present: true,
                value: Some(1),
            },
        })
        .expect("prepare setting operation");

    context
        .storage()
        .finish_nvapi_setting_operation(
            renderpilot_storage_sqlite::NvapiSettingOperationCompletion {
                op_id: "claim-op",
                scope: renderpilot_storage_sqlite::NvapiSettingOperationScope::Game {
                    game_id: "manual:other-game",
                },
                target_id: &owner.profile_name,
                setting_id: 0x10f9dc83,
                expected: renderpilot_storage_sqlite::NvapiSettingState {
                    present: true,
                    value: Some(1),
                },
                profile_identity_json: &profile_identity_json,
                composition_json: None,
            },
        )
        .expect("finish setting operation");

    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(matching_observation(&owner));
    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "other game claim must block deletion: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("another game still references a setting on this profile"),
        "must fail with claim block: {err}"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn characterization_owned_delete_overlapping_pending_operation_blocks_deletion() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    context
        .storage()
        .begin_nvapi_operation(
            "existing-pending-op",
            Some(&owner.game_id),
            Some(&owner.profile_name),
            "setting",
            "{}",
            "{}",
        )
        .expect("begin existing pending op");

    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(matching_observation(&owner));
    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "must fail due to overlapping pending operation: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("overlaps this profile or executable"),
        "error must indicate overlapping pending operation: {err}"
    );
    assert!(
        drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_confirmed_missing_profile_releases_ownership_without_driver_mutation() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    add_game_setting_claim(&context, &owner.game_id, &owner, 0x10f9dc83, 1);

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("read owner")
            .is_some()
    );
    assert!(
        !context
            .storage()
            .list_nvapi_setting_claims_for_game(&owner.game_id)
            .expect("read claims")
            .is_empty()
    );
    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("read pending")
            .is_empty()
    );

    let setting_context = test_setting_context(Some(path));
    let pending_intent_observed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let pending_intent_observed_hook = pending_intent_observed.clone();
    let drs = SyntheticOwnedProfileDeleteDrs::with_hook(
        OwnedProfileObservation {
            by_name: Lookup::Missing,
            by_path: Lookup::Missing,
        },
        Box::new(|count| {
            if count == 2 {
                let pending = context
                    .storage()
                    .list_pending_nvapi_operations()
                    .expect("read pending during second observation");
                if pending.len() == 1
                    && pending[0].phase == "intent"
                    && pending[0].kind == "delete_profile"
                {
                    pending_intent_observed_hook.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
        }),
    );

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);

    assert!(
        result.is_ok(),
        "confirmed missing profile deletion must succeed: {result:?}"
    );
    assert!(
        pending_intent_observed.load(std::sync::atomic::Ordering::SeqCst),
        "durable pending intent must have existed prior to completion"
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query owned")
            .is_none(),
        "owned profile receipt must be removed from storage"
    );
    assert!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(&owner.game_id)
            .expect("query claims")
            .is_empty(),
        "target setting claims for this game must be reconciled"
    );
    let game_id_parsed = GameId::new(&owner.game_id).expect("game id");
    assert!(
        context
            .storage()
            .find_game(&game_id_parsed)
            .expect("query game")
            .is_some(),
        "game record in catalog must remain intact"
    );
    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("query pending")
            .is_empty(),
        "no unresolved pending operation may remain after completion"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst),
        "synthetic adapter must report no live delete was attempted"
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst),
        "synthetic adapter must report no live mutation occurred"
    );
}

#[test]
fn owned_delete_missing_profile_blocked_by_existing_pending_intent() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    add_game_setting_claim(&context, &owner.game_id, &owner, 0x10f9dc83, 1);

    context
        .storage()
        .begin_nvapi_operation(
            "pending-intent-op",
            Some(&owner.game_id),
            Some(&owner.profile_name),
            "setting",
            "{}",
            "{}",
        )
        .expect("begin pending intent");

    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Missing,
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "missing profile deletion must be blocked by pending intent: {result:?}"
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some(),
        "owned profile receipt must be preserved"
    );
    assert!(
        !context
            .storage()
            .list_nvapi_setting_claims_for_game(&owner.game_id)
            .expect("query")
            .is_empty(),
        "claims must be preserved"
    );
    let game_id_parsed = GameId::new(&owner.game_id).expect("game id");
    assert!(
        context
            .storage()
            .find_game(&game_id_parsed)
            .expect("query game")
            .is_some(),
        "game must be preserved"
    );
    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("query pending");
    assert_eq!(pending.len(), 1, "exactly one pending op must exist");
    assert_eq!(pending[0].op_id, "pending-intent-op");
    assert_eq!(
        pending[0].phase, "intent",
        "pending op phase must remain intent"
    );

    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_missing_profile_blocked_by_existing_pending_conflict() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    add_game_setting_claim(&context, &owner.game_id, &owner, 0x10f9dc83, 1);

    context
        .storage()
        .begin_nvapi_operation(
            "pending-conflict-op",
            Some(&owner.game_id),
            Some(&owner.profile_name),
            "setting",
            "{}",
            "{}",
        )
        .expect("begin pending conflict");
    context
        .storage()
        .mark_nvapi_operation_conflict("pending-conflict-op", "{\"observed\":\"conflict\"}")
        .expect("mark conflict");

    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Missing,
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "missing profile deletion must be blocked by pending conflict: {result:?}"
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some(),
        "owned profile receipt must be preserved"
    );
    assert!(
        !context
            .storage()
            .list_nvapi_setting_claims_for_game(&owner.game_id)
            .expect("query")
            .is_empty(),
        "claims must be preserved"
    );
    let game_id_parsed = GameId::new(&owner.game_id).expect("game id");
    assert!(
        context
            .storage()
            .find_game(&game_id_parsed)
            .expect("query game")
            .is_some(),
        "game must be preserved"
    );
    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("query pending");
    assert_eq!(pending.len(), 1, "exactly one pending op must exist");
    assert_eq!(pending[0].op_id, "pending-conflict-op");
    assert_eq!(
        pending[0].phase, "conflict",
        "pending op phase must remain conflict"
    );

    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_missing_profile_blocked_by_other_game_overlapping_pending_op() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    add_game_setting_claim(&context, &owner.game_id, &owner, 0x10f9dc83, 1);

    let game_id2 = GameId::new("manual:other-game").expect("game id 2");
    let identity2 =
        GameIdentity::new(game_id2, "Other Game", Launcher::Manual).expect("identity 2");
    let game2 = GameInstallation::new(
        identity2,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Other").expect("install path"),
    );
    context
        .storage()
        .upsert_game(&game2)
        .expect("persist game 2");

    context
        .storage()
        .begin_nvapi_operation(
            "other-game-pending-op",
            Some("manual:other-game"),
            Some(&owner.profile_name),
            "setting",
            "{}",
            "{}",
        )
        .expect("begin other game op");

    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Missing,
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "missing profile deletion must be blocked by other game's pending op: {result:?}"
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some()
    );
    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("query pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "other-game-pending-op");
    assert_eq!(pending[0].game_id.as_deref(), Some("manual:other-game"));

    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_missing_profile_blocked_by_other_game_claim_reference() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    add_game_setting_claim(&context, &owner.game_id, &owner, 0x10f9dc83, 1);

    let game_id2 = GameId::new("manual:other-game").expect("game id 2");
    let identity2 =
        GameIdentity::new(game_id2, "Other Game", Launcher::Manual).expect("identity 2");
    let game2 = GameInstallation::new(
        identity2,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Other").expect("install path"),
    );
    context
        .storage()
        .upsert_game(&game2)
        .expect("persist game 2");

    let other_path = "C:/Games/Other/other.exe";
    let other_witness = app_witness(other_path);
    let other_witness_json = serde_json::to_string(&other_witness).expect("witness JSON");
    let profile_identity = ProfileIdentity {
        name: owner.profile_name.clone(),
        is_predefined: false,
        application_count: 1,
        setting_count: 0,
    };
    let profile_identity_json =
        serde_json::to_string(&profile_identity).expect("profile identity JSON");

    context
        .storage()
        .prepare_nvapi_setting_operation(renderpilot_storage_sqlite::NvapiGameSettingPreparation {
            op_id: "other-claim-op",
            game_id: "manual:other-game",
            profile_name: &owner.profile_name,
            target_kind: "renderpilot",
            profile_is_predefined: false,
            profile_identity_json: &profile_identity_json,
            executable_path: other_path,
            application_witness_json: &other_witness_json,
            setting_id: 0x10f9dc84,
            original: renderpilot_storage_sqlite::NvapiSettingState {
                present: false,
                value: None,
            },
            before: renderpilot_storage_sqlite::NvapiSettingState {
                present: false,
                value: None,
            },
            after: renderpilot_storage_sqlite::NvapiSettingState {
                present: true,
                value: Some(1),
            },
        })
        .expect("prepare setting operation");

    context
        .storage()
        .finish_nvapi_setting_operation(
            renderpilot_storage_sqlite::NvapiSettingOperationCompletion {
                op_id: "other-claim-op",
                scope: renderpilot_storage_sqlite::NvapiSettingOperationScope::Game {
                    game_id: "manual:other-game",
                },
                target_id: &owner.profile_name,
                setting_id: 0x10f9dc84,
                expected: renderpilot_storage_sqlite::NvapiSettingState {
                    present: true,
                    value: Some(1),
                },
                profile_identity_json: &profile_identity_json,
                composition_json: None,
            },
        )
        .expect("finish setting operation");

    assert!(
        context
            .storage()
            .nvapi_target_has_other_game_claims(&owner.profile_name, &owner.game_id)
            .expect("check claims"),
        "target must have claims from other game"
    );

    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Missing,
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "deletion must be blocked by other-game claim reference: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("another game still references a setting on this profile"),
        "must fail with other-game claim block: {err}"
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some()
    );
    assert!(
        !context
            .storage()
            .list_nvapi_setting_claims_for_game(&owner.game_id)
            .expect("query")
            .is_empty()
    );
    assert!(
        !context
            .storage()
            .list_nvapi_setting_claims_for_game("manual:other-game")
            .expect("query")
            .is_empty()
    );
    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("query")
            .is_empty()
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_found_by_name_but_missing_by_path_is_blocked() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let receipt = matching_receipt(&owner);
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Found(receipt),
        by_path: Lookup::Missing,
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "profile existing by name must block deletion even if path is unhooked: {result:?}"
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some()
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_missing_by_name_but_found_different_by_path_is_blocked() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let mut different_receipt = matching_receipt(&owner);
    different_receipt.identity.name = "Other Profile".to_owned();
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Found(different_receipt),
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "reassigned executable must block deletion: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("RenderPilot-owned NVIDIA profile changed"),
        "must fail with conflict: {err}"
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some()
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_contradictory_missing_by_name_but_matching_by_path_is_blocked() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let receipt = matching_receipt(&owner);
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Found(receipt),
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "contradictory observation (missing by name but matching by path) must be blocked: {result:?}"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst),
        "contradictory observation must not attempt live deletion"
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some()
    );
}

#[test]
fn owned_delete_failed_by_name_and_missing_by_path_is_blocked() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Failed("driver communication failure".to_owned()),
        by_path: Lookup::Missing,
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "unproven name absence must block deletion: {result:?}"
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some()
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_missing_by_name_and_failed_by_path_is_blocked() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Failed("driver communication failure".to_owned()),
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "driver lookup failure on path must block deletion: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("driver communication failure"),
        "error must convey driver failure: {err}"
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some()
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_missing_by_name_and_ambiguous_by_path_is_blocked() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    let setting_context = test_setting_context(Some(path));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Ambiguous,
    });

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "ambiguous executable lookup must block deletion: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("DRS executable lookup is ambiguous") || err.contains("-182"),
        "error must convey ambiguous lookup: {err}"
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some()
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_missing_profile_aborted_when_second_observation_drifts_to_present() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    add_game_setting_claim(&context, &owner.game_id, &owner, 0x10f9dc83, 1);

    let setting_context = test_setting_context(Some(path));
    let missing_obs = OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Missing,
    };
    let matching_obs = matching_observation(&owner);
    let drs =
        SyntheticOwnedProfileDeleteDrs::with_sequence(vec![Ok(missing_obs), Ok(matching_obs)]);

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "reappearing profile must abort missing-profile deletion: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("NVIDIA profile reappeared during deletion"),
        "error must indicate reappearance cancellation: {err}"
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some(),
        "owned profile receipt must be preserved"
    );
    assert!(
        !context
            .storage()
            .list_nvapi_setting_claims_for_game(&owner.game_id)
            .expect("query")
            .is_empty(),
        "claims must be preserved"
    );
    let game_id_parsed = GameId::new(&owner.game_id).expect("game id");
    assert!(
        context
            .storage()
            .find_game(&game_id_parsed)
            .expect("query game")
            .is_some(),
        "game must be preserved"
    );
    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("query pending")
            .is_empty(),
        "pending operation must be cancelled cleanly without conflict"
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_missing_profile_leaves_pending_when_second_observation_fails() {
    let path = "C:/Games/Test/game.exe";
    let (context, owner) = setup_game_with_owned_profile("manual:test-game", path);
    add_game_setting_claim(&context, &owner.game_id, &owner, 0x10f9dc83, 1);

    let setting_context = test_setting_context(Some(path));
    let missing_obs = OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Missing,
    };
    let drs = SyntheticOwnedProfileDeleteDrs::with_sequence(vec![
        Ok(missing_obs),
        Err(crate::ServiceError::command_failed(
            "driver disconnected during verification",
        )),
    ]);

    let result = delete_owned_profile_with_drs(&context, &owner.game_id, &setting_context, &drs);
    assert!(
        result.is_err(),
        "unavailable second observation must return Err: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("driver disconnected during verification"),
        "error must reflect failure: {err}"
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner.game_id)
            .expect("query")
            .is_some(),
        "owned profile receipt must be preserved"
    );
    assert!(
        !context
            .storage()
            .list_nvapi_setting_claims_for_game(&owner.game_id)
            .expect("query")
            .is_empty(),
        "claims must be preserved"
    );
    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("query pending");
    assert_eq!(
        pending.len(),
        1,
        "pending operation must remain for recovery"
    );
    assert_eq!(pending[0].kind, "delete_profile");
    assert_eq!(pending[0].phase, "intent");

    let after_val: serde_json::Value =
        serde_json::from_str(&pending[0].after_json).expect("valid after json");
    assert_eq!(
        after_val["deletion_provenance"], "confirmed_absent_v1",
        "ConfirmedAbsent receipt must record confirmed_absent_v1 provenance"
    );
    assert_eq!(after_val["profile_name"], owner.profile_name);
    assert_eq!(after_val["binding_path"], owner.binding_path);
    assert_eq!(after_val["profile_absent"], true);

    let before_val: serde_json::Value =
        serde_json::from_str(&pending[0].before_json).expect("valid before json");
    assert_eq!(before_val["profile_name"], owner.profile_name);
    assert_eq!(before_val["binding_path"], owner.binding_path);
    assert_eq!(before_val["composition"], owner.composition_json);
    assert_eq!(
        before_val["application_witness"],
        owner.application_witness_json
    );

    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn owned_delete_missing_profile_receipt_integrity_preserves_unrelated_target_claims() {
    let path1 = "C:/Games/Game1/game.exe";
    let (context, owner1) = setup_game_with_owned_profile("manual:game1", path1);
    add_game_setting_claim(&context, &owner1.game_id, &owner1, 0x10f9dc83, 1);

    let game_id2 = GameId::new("manual:game2").expect("game id 2");
    let identity2 = GameIdentity::new(game_id2, "Game 2", Launcher::Manual).expect("identity 2");
    let game2 = GameInstallation::new(
        identity2,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Game2").expect("install path"),
    );
    context
        .storage()
        .upsert_game(&game2)
        .expect("persist game 2");

    let path2 = "C:/Games/Game2/game.exe";
    let witness2 = app_witness(path2);
    let witness_json2 = serde_json::to_string(&witness2).expect("witness JSON");
    let profile_name2 = "Unrelated Target Profile";
    let profile_identity2 = ProfileIdentity {
        name: profile_name2.to_owned(),
        is_predefined: false,
        application_count: 1,
        setting_count: 0,
    };
    let profile_identity_json2 =
        serde_json::to_string(&profile_identity2).expect("profile identity JSON");

    context
        .storage()
        .prepare_nvapi_setting_operation(renderpilot_storage_sqlite::NvapiGameSettingPreparation {
            op_id: "game2-claim-op",
            game_id: "manual:game2",
            profile_name: profile_name2,
            target_kind: "external",
            profile_is_predefined: false,
            profile_identity_json: &profile_identity_json2,
            executable_path: path2,
            application_witness_json: &witness_json2,
            setting_id: 0x10f9dc84,
            original: renderpilot_storage_sqlite::NvapiSettingState {
                present: false,
                value: None,
            },
            before: renderpilot_storage_sqlite::NvapiSettingState {
                present: false,
                value: None,
            },
            after: renderpilot_storage_sqlite::NvapiSettingState {
                present: true,
                value: Some(1),
            },
        })
        .expect("prepare setting operation");

    context
        .storage()
        .finish_nvapi_setting_operation(
            renderpilot_storage_sqlite::NvapiSettingOperationCompletion {
                op_id: "game2-claim-op",
                scope: renderpilot_storage_sqlite::NvapiSettingOperationScope::Game {
                    game_id: "manual:game2",
                },
                target_id: profile_name2,
                setting_id: 0x10f9dc84,
                expected: renderpilot_storage_sqlite::NvapiSettingState {
                    present: true,
                    value: Some(1),
                },
                profile_identity_json: &profile_identity_json2,
                composition_json: None,
            },
        )
        .expect("finish setting operation");

    let setting_context = test_setting_context(Some(path1));
    let drs = SyntheticOwnedProfileDeleteDrs::new(OwnedProfileObservation {
        by_name: Lookup::Missing,
        by_path: Lookup::Missing,
    });

    let result = delete_owned_profile_with_drs(&context, &owner1.game_id, &setting_context, &drs);
    assert!(
        result.is_ok(),
        "missing profile deletion must succeed for game 1: {result:?}"
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(&owner1.game_id)
            .expect("query")
            .is_none()
    );
    assert!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(&owner1.game_id)
            .expect("query")
            .is_empty()
    );
    let game_id1_parsed = GameId::new(&owner1.game_id).expect("game id 1");
    assert!(
        context
            .storage()
            .find_game(&game_id1_parsed)
            .expect("query")
            .is_some()
    );

    let game_id2_parsed = GameId::new("manual:game2").expect("game id 2");
    assert!(
        context
            .storage()
            .find_game(&game_id2_parsed)
            .expect("query")
            .is_some()
    );
    let game2_claims = context
        .storage()
        .list_nvapi_setting_claims_for_game("manual:game2")
        .expect("query game 2 claims");
    assert_eq!(game2_claims.len(), 1);
    assert_eq!(game2_claims[0].target_id, profile_name2);

    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("query pending")
            .is_empty()
    );
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}
