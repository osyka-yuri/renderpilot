use super::*;

#[test]
fn pending_intent_for_game_blocks_removal_preflight() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Pending Intent Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let (game, managed_file) = seed_game_with_external_claim(
        &context,
        &install,
        "manual:pending-intent-game",
        "NVIDIA Base Target",
        0x10B3_292C,
    );

    context
        .storage()
        .begin_nvapi_operation(
            "op-pending-intent",
            Some(game.id().as_str()),
            Some("NVIDIA Base Target"),
            "setting",
            r#"{"setting_id":280176940}"#,
            r#"{"setting_id":280176940}"#,
        )
        .expect("seed pending intent");

    let pending_before = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("read pending");
    assert_eq!(pending_before.len(), 1);
    assert_eq!(pending_before[0].phase, "intent");

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("pending intent must block catalog removal");

    assert!(
        error
            .to_string()
            .contains("durable DRS operation must be reconciled")
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("read game")
            .is_some()
    );
    assert!(managed_file.exists(), "managed files must not be mutated");
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs("NVIDIA Base Target", 0x10B3_292C)
            .expect("read refs"),
        1
    );
    let pending_after = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("read pending");
    assert_eq!(pending_after.len(), 1);
    assert_eq!(pending_after[0].op_id, "op-pending-intent");
    assert_eq!(pending_after[0].phase, "intent");
}

#[test]
fn pending_conflict_for_game_blocks_removal_preflight() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Pending Conflict Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let (game, managed_file) = seed_game_with_external_claim(
        &context,
        &install,
        "manual:pending-conflict-game",
        "NVIDIA Base Target",
        0x10B3_292C,
    );

    context
        .storage()
        .begin_nvapi_operation(
            "op-pending-conflict",
            Some(game.id().as_str()),
            Some("NVIDIA Base Target"),
            "setting",
            r#"{"setting_id":280176940}"#,
            r#"{"setting_id":280176940}"#,
        )
        .expect("seed pending op");
    context
        .storage()
        .mark_nvapi_operation_conflict("op-pending-conflict", r#"{"diverged":true}"#)
        .expect("mark conflict");

    let pending_before = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("read pending");
    assert_eq!(pending_before.len(), 1);
    assert_eq!(pending_before[0].phase, "conflict");

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("pending conflict must block catalog removal");

    assert!(
        error
            .to_string()
            .contains("durable DRS operation must be reconciled")
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("read game")
            .is_some()
    );
    assert!(managed_file.exists(), "managed files must not be mutated");
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs("NVIDIA Base Target", 0x10B3_292C)
            .expect("read refs"),
        1
    );
    let pending_after = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("read pending");
    assert_eq!(pending_after.len(), 1);
    assert_eq!(pending_after[0].op_id, "op-pending-conflict");
    assert_eq!(pending_after[0].phase, "conflict");
}

#[test]
fn pending_operation_on_same_target_from_other_game_blocks_removal_preflight() {
    let temp = tempfile::tempdir().expect("temp");
    let install_a = temp.path().join("Game A");
    let install_b = temp.path().join("Game B");
    std::fs::create_dir_all(&install_a).expect("install a");
    std::fs::create_dir_all(&install_b).expect("install b");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let (game_a, managed_file_a) = seed_game_with_external_claim(
        &context,
        &install_a,
        "manual:game-a",
        "NVIDIA Shared Target",
        0x10B3_292C,
    );

    let game_b = game(&install_b, RootAuthority::UserConfirmed);
    context
        .storage()
        .upsert_game(&game_b)
        .expect("upsert game b");

    context
        .storage()
        .begin_nvapi_operation(
            "op-game-b",
            Some(game_b.id().as_str()),
            Some("NVIDIA Shared Target"),
            "setting",
            r#"{"setting_id":280176940}"#,
            r#"{"setting_id":280176940}"#,
        )
        .expect("seed pending op for game b");

    let error = remove_game_from_catalog(&context, game_a.id())
        .expect_err("pending op on same target must block Game A removal");

    assert!(
        error
            .to_string()
            .contains("durable DRS operation must be reconciled")
    );
    assert!(
        context
            .storage()
            .find_game(game_a.id())
            .expect("read game a")
            .is_some()
    );
    assert!(
        managed_file_a.exists(),
        "Game A managed file must not be touched"
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs("NVIDIA Shared Target", 0x10B3_292C)
            .expect("read refs"),
        1
    );
    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("read pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-game-b");
}

#[test]
fn pending_global_operation_on_same_target_blocks_removal_preflight() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Global Target Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let (game, managed_file) = seed_game_with_external_claim(
        &context,
        &install,
        "manual:global-target-game",
        "Base Profile",
        0x10B3_292C,
    );

    context
        .storage()
        .begin_nvapi_operation(
            "op-global-pending",
            None,
            Some("Base Profile"),
            "setting",
            r#"{"setting_id":280176940}"#,
            r#"{"setting_id":280176940}"#,
        )
        .expect("seed global pending op");

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("global pending op on same target must block game removal");

    assert!(
        error
            .to_string()
            .contains("durable DRS operation must be reconciled")
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("read game")
            .is_some()
    );
    assert!(managed_file.exists(), "managed file must not be touched");
    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("read pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-global-pending");
}

#[test]
fn pending_operation_on_unrelated_target_does_not_block_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Clean Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("upsert game");

    let other_install = temp.path().join("Other Game");
    std::fs::create_dir_all(&other_install).expect("install other");
    let other_game = GameInstallation::new(
        GameIdentity::new(
            GameId::new("manual:other-unrelated-game").expect("game id"),
            "Other Game",
            Launcher::Manual,
        )
        .expect("game identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(other_install.to_string_lossy()).expect("install path"),
    )
    .with_root_authority(RootAuthority::UserConfirmed);
    context
        .storage()
        .upsert_game(&other_game)
        .expect("upsert other game");

    context
        .storage()
        .begin_nvapi_operation(
            "op-unrelated",
            Some(other_game.id().as_str()),
            Some("Unrelated NVIDIA Profile"),
            "setting",
            r#"{"setting_id":280176940}"#,
            r#"{"setting_id":280176940}"#,
        )
        .expect("seed unrelated pending op");

    let result = remove_game_from_catalog(&context, game.id())
        .expect("unrelated pending operation must not block removal");

    assert_eq!(result.game_id, game.id().as_str());
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("read game")
            .is_none()
    );
    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("read pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-unrelated");
}

#[test]
fn shared_setting_claim_release_removes_only_referencing_game_and_allows_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install_a = temp.path().join("Shared Game A");
    let install_b = temp.path().join("Shared Game B");
    std::fs::create_dir_all(&install_a).expect("install a");
    std::fs::create_dir_all(&install_b).expect("install b");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Shared Target";
    let setting_id = 0x10B3_292C;

    let (game_a, _) = seed_game_with_external_claim(
        &context,
        &install_a,
        "manual:shared-game-a",
        target,
        setting_id,
    );

    let game_b = GameInstallation::new(
        GameIdentity::new(
            GameId::new("manual:shared-game-b").expect("game id"),
            "Game B",
            Launcher::Manual,
        )
        .expect("game identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(install_b.to_string_lossy()).expect("install path"),
    )
    .with_root_authority(RootAuthority::UserConfirmed);
    context
        .storage()
        .upsert_game(&game_b)
        .expect("upsert game b");

    let exe_b = format!(
        "{}/GameB.exe",
        install_b.to_string_lossy().replace('\\', "/")
    );
    let identity_json = format!(r#"{{"name":"{target}","is_predefined":true}}"#);
    let witness_b = format!(r#"{{"app_name":"{exe_b}"}}"#);

    context
        .storage()
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-prep-b",
            game_id: game_b.id().as_str(),
            profile_name: target,
            target_kind: "external",
            profile_is_predefined: true,
            profile_identity_json: &identity_json,
            executable_path: &exe_b,
            application_witness_json: &witness_b,
            setting_id,
            original: NvapiSettingState {
                present: false,
                value: None,
            },
            before: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            after: NvapiSettingState {
                present: true,
                value: Some(2),
            },
        })
        .expect("prepare game b claim");
    context
        .storage()
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-prep-b",
            scope: NvapiSettingOperationScope::Game {
                game_id: game_b.id().as_str(),
            },
            target_id: target,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(2),
            },
            profile_identity_json: &identity_json,
            composition_json: None,
        })
        .expect("finish game b claim");

    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("count refs"),
        2
    );
    assert_eq!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game_a.id().as_str())
            .expect("claims a")
            .len(),
        1
    );
    assert_eq!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game_b.id().as_str())
            .expect("claims b")
            .len(),
        1
    );

    let result = remove_game_from_catalog(&context, game_a.id())
        .expect("removing Game A with shared claim must succeed without calling NVAPI");
    assert_eq!(result.game_id, game_a.id().as_str());

    assert!(
        context
            .storage()
            .find_game(game_a.id())
            .expect("find a")
            .is_none()
    );
    assert!(
        context
            .storage()
            .find_game(game_b.id())
            .expect("find b")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("count refs after"),
        1
    );
    assert!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game_a.id().as_str())
            .expect("claims a after")
            .is_empty()
    );

    let claims_b = context
        .storage()
        .list_nvapi_setting_claims_for_game(game_b.id().as_str())
        .expect("claims b after");
    assert_eq!(claims_b.len(), 1);

    assert!(!claims_b[0].original_present);
    assert_eq!(claims_b[0].original_value, None);
    assert!(claims_b[0].expected_present);
    assert_eq!(claims_b[0].expected_value, Some(2));

    assert_eq!(
        context
            .storage()
            .get_nvapi_target_profile_is_predefined(target)
            .expect("target predefined"),
        Some(true)
    );
    assert!(
        context
            .storage()
            .get_nvapi_application_witness(target, &exe_b)
            .expect("witness b")
            .is_some()
    );

    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("pending ops")
            .is_empty()
    );
}
