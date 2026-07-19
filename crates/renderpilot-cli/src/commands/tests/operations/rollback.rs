use std::fs;

use renderpilot_orchestration::application::ComponentRepository;

use crate::hash::sha256_hex;

use super::super::args;
use super::helpers::{ORIGINAL_BYTES, setup_applied_scenario};
#[cfg(windows)]
use super::helpers::{REPLACEMENT_BYTES, open_exclusive_file_lock};

#[test]
fn rollback_restores_original_file_and_updates_catalog() {
    let scenario = setup_applied_scenario("rollback-success");

    let rollback_output = scenario
        .fixture
        .run(args(&[
            "rollback",
            "--game",
            scenario.game_id.as_str(),
            "--component",
            "component:game-a:dlss",
        ]))
        .expect("rollback should succeed");
    let rollback_json: serde_json::Value =
        serde_json::from_str(&rollback_output).expect("valid rollback json");
    let components = scenario
        .fixture
        .storage()
        .list_components_for_game(&scenario.game_id)
        .expect("components should load");

    assert_eq!(rollback_json["game_id"], scenario.game_id.as_str());
    assert_eq!(rollback_json["component_id"], "component:game-a:dlss");
    assert_eq!(
        fs::read(&scenario.source_path).expect("restored bytes should be readable"),
        ORIGINAL_BYTES
    );
    assert_eq!(sha256_hex(ORIGINAL_BYTES), scenario.original_sha256);
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].files().len(), 1);
    assert_eq!(
        components[0].files()[0]
            .sha256()
            .map(|sha256| sha256.as_str()),
        Some(scenario.original_sha256.as_str())
    );
}

#[test]
fn rollback_consumes_bak_on_first_restore_and_second_fails() {
    let scenario = setup_applied_scenario("rollback-consumed");

    scenario
        .fixture
        .run(args(&[
            "rollback",
            "--game",
            scenario.game_id.as_str(),
            "--component",
            "component:game-a:dlss",
        ]))
        .expect("first rollback should succeed");

    let second_error = scenario
        .fixture
        .run(args(&[
            "rollback",
            "--game",
            scenario.game_id.as_str(),
            "--component",
            "component:game-a:dlss",
        ]))
        .expect_err("second rollback should fail because the baseline is cleared");

    assert!(
        second_error.to_string().contains("no swap to roll back"),
        "expected no-baseline error, got: {}",
        second_error
    );
}

#[cfg(windows)]
#[test]
fn rollback_fails_when_target_file_is_locked() {
    let scenario = setup_applied_scenario("rollback-locked");
    let lock = open_exclusive_file_lock(&scenario.source_path);

    let error = scenario
        .fixture
        .run(args(&[
            "rollback",
            "--game",
            scenario.game_id.as_str(),
            "--component",
            "component:game-a:dlss",
        ]))
        .expect_err("rollback should fail while target is locked");
    drop(lock);

    assert!(
        error.to_string().contains("before restore")
            || error.to_string().contains("changed on disk")
            || error.to_string().contains("could not open")
            || error.to_string().contains("cannot read baseline"),
        "expected restore/lock failure error, got: {error}"
    );
    assert_eq!(
        fs::read(&scenario.source_path).expect("applied bytes should remain in place"),
        REPLACEMENT_BYTES
    );
}
