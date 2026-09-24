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
