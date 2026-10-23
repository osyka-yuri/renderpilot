use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::*;
use crate::nvapi::ops::{
    ExternalSettingClaimDrs, ObservedClaimDrs, set_test_external_setting_claim_drs,
};
use crate::{NvapiClaimBlockedReason, ServiceError};
use renderpilot_storage_sqlite::NvapiTargetSettingClaimRow;

struct MockExternalClaimDrs {
    observations: Mutex<HashMap<(String, u32), ObservedClaimDrs>>,
    default_observation: Mutex<Option<ObservedClaimDrs>>,
    restore_calls: AtomicUsize,
    restore_error: Mutex<Option<String>>,
}

impl MockExternalClaimDrs {
    fn new() -> Self {
        Self {
            observations: Mutex::new(HashMap::new()),
            default_observation: Mutex::new(None),
            restore_calls: AtomicUsize::new(0),
            restore_error: Mutex::new(None),
        }
    }

    fn with_default(observation: ObservedClaimDrs) -> Self {
        let mock = Self::new();
        *mock.default_observation.lock().unwrap() = Some(observation);
        mock
    }

    fn set_observation(&self, target_id: &str, setting_id: u32, observation: ObservedClaimDrs) {
        self.observations
            .lock()
            .unwrap()
            .insert((target_id.to_owned(), setting_id), observation);
    }

    fn set_restore_error(&self, err: &str) {
        *self.restore_error.lock().unwrap() = Some(err.to_owned());
    }
}

impl ExternalSettingClaimDrs for MockExternalClaimDrs {
    fn observe_claim(
        &self,
        _context: &crate::Context,
        claim: &NvapiTargetSettingClaimRow,
    ) -> Result<ObservedClaimDrs, ServiceError> {
        let obs = self.observations.lock().unwrap();
        if let Some(res) = obs.get(&(claim.target_id.clone(), claim.setting_id)) {
            return Ok(res.clone());
        }
        if let Some(ref def) = *self.default_observation.lock().unwrap() {
            return Ok(def.clone());
        }
        Ok(ObservedClaimDrs::Verified {
            profile_name: claim.target_id.clone(),
            is_predefined: true,
            witness_json: format!(r#"{{"app_name":"{}"}}"#, claim.executable_path),
            live: (claim.expected_present, claim.expected_value),
        })
    }

    fn execute_restore(
        &self,
        context: &crate::Context,
        game_id: &str,
        claim: &NvapiTargetSettingClaimRow,
    ) -> Result<(), ServiceError> {
        self.restore_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(ref err) = *self.restore_error.lock().unwrap() {
            return Err(ServiceError::command_failed(err.clone()));
        }
        context.storage().release_nvapi_setting_claim(
            game_id,
            &claim.target_id,
            claim.setting_id,
        )?;
        Ok(())
    }
}

fn seed_game_with_custom_claim(
    context: &crate::Context,
    install: &std::path::Path,
    game_id_str: &str,
    target_name: &str,
    setting_id: u32,
    original: NvapiSettingState,
    expected: NvapiSettingState,
) -> (GameInstallation, std::path::PathBuf) {
    let game = GameInstallation::new(
        GameIdentity::new(
            GameId::new(game_id_str).expect("game id"),
            "Game Title",
            Launcher::Manual,
        )
        .expect("game identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(install.to_string_lossy()).expect("install path"),
    )
    .with_root_authority(RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("upsert game");

    let managed_file = install.join("managed_mod.dll");
    std::fs::write(&managed_file, b"mod-data").expect("write file");
    context
        .storage()
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::Luma,
            path_ref(&managed_file),
        ))
        .expect("seed addon");

    let exe_path = format!("{}/Game.exe", install.to_string_lossy().replace('\\', "/"));
    let identity_json = format!(r#"{{"name":"{target_name}","is_predefined":true}}"#);
    let witness_json = format!(r#"{{"app_name":"{exe_path}"}}"#);

    context
        .storage()
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: &format!("op-prep-{game_id_str}"),
            game_id: game.id().as_str(),
            profile_name: target_name,
            target_kind: "external",
            profile_is_predefined: true,
            profile_identity_json: &identity_json,
            executable_path: &exe_path,
            application_witness_json: &witness_json,
            setting_id,
            original,
            before: original,
            after: expected,
        })
        .expect("prep claim");
    context
        .storage()
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: &format!("op-prep-{game_id_str}"),
            scope: NvapiSettingOperationScope::Game {
                game_id: game.id().as_str(),
            },
            target_id: target_name,
            setting_id,
            expected,
            profile_identity_json: &identity_json,
            composition_json: None,
        })
        .expect("finish claim");

    (game, managed_file)
}

fn seed_local_owned_profile(
    context: &crate::Context,
    install: &std::path::Path,
    game_id_str: &str,
    profile_name: &str,
    setting_id: u32,
) -> (GameInstallation, std::path::PathBuf) {
    let game = GameInstallation::new(
        GameIdentity::new(
            GameId::new(game_id_str).expect("game id"),
            "Game Title",
            Launcher::Manual,
        )
        .expect("game identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(install.to_string_lossy()).expect("install path"),
    )
    .with_root_authority(RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("upsert game");

    let binding = format!("{}/Game.exe", install.to_string_lossy().replace('\\', "/"));
    let identity_json = format!(r#"{{"name":"{profile_name}","is_predefined":false}}"#);
    let witness_json = format!(r#"{{"app_name":"{binding}"}}"#);

    context
        .storage()
        .begin_nvapi_operation(
            &format!("create-owner-{game_id_str}"),
            Some(game.id().as_str()),
            Some(profile_name),
            "create_profile",
            r#"{"profile_absent":true}"#,
            r#"{"profile_present":true}"#,
        )
        .expect("journal");
    context
        .storage()
        .complete_nvapi_profile_creation(NvapiProfileCreationCompletion {
            op_id: &format!("create-owner-{game_id_str}"),
            game_id: game.id().as_str(),
            binding_path: &binding,
            profile: NvapiVerifiedProfileReceipt {
                profile_name,
                profile_identity_json: &identity_json,
                application_witness_json: &witness_json,
                composition_json: r#"{"applications":[],"settings":[]}"#,
            },
        })
        .expect("owner receipt");

    context
        .storage()
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: &format!("op-claim-{game_id_str}"),
            game_id: game.id().as_str(),
            profile_name,
            target_kind: "renderpilot",
            profile_is_predefined: false,
            profile_identity_json: &identity_json,
            executable_path: &binding,
            application_witness_json: &witness_json,
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
        .expect("prepare owned claim");
    context
        .storage()
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: &format!("op-claim-{game_id_str}"),
            scope: NvapiSettingOperationScope::Game {
                game_id: game.id().as_str(),
            },
            target_id: profile_name,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            profile_identity_json: &identity_json,
            composition_json: None,
        })
        .expect("finish owned claim");

    let managed_file = install.join("managed.addon64");
    std::fs::write(&managed_file, b"managed addon content").expect("write addon file");
    context
        .storage()
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::Luma,
            path_ref(&managed_file),
        ))
        .expect("seed addon");

    (game, managed_file)
}

fn seed_second_shared_claim(
    context: &crate::Context,
    install: &std::path::Path,
    game_id_str: &str,
    target_name: &str,
    setting_id: u32,
) -> (GameInstallation, std::path::PathBuf) {
    let game = GameInstallation::new(
        GameIdentity::new(
            GameId::new(game_id_str).expect("game id"),
            "Game Title B",
            Launcher::Manual,
        )
        .expect("game identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(install.to_string_lossy()).expect("install path"),
    )
    .with_root_authority(RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("upsert game");

    let managed_file = install.join("managed_mod_b.dll");
    std::fs::write(&managed_file, b"mod-data-b").expect("write file");
    context
        .storage()
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::Luma,
            path_ref(&managed_file),
        ))
        .expect("seed addon");

    let exe_path = format!("{}/GameB.exe", install.to_string_lossy().replace('\\', "/"));
    let identity_json = format!(r#"{{"name":"{target_name}","is_predefined":true}}"#);
    let witness_json = format!(r#"{{"app_name":"{exe_path}"}}"#);

    context
        .storage()
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: &format!("op-prep-{game_id_str}"),
            game_id: game.id().as_str(),
            profile_name: target_name,
            target_kind: "external",
            profile_is_predefined: true,
            profile_identity_json: &identity_json,
            executable_path: &exe_path,
            application_witness_json: &witness_json,
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
                value: Some(1),
            },
        })
        .expect("prep claim");
    context
        .storage()
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: &format!("op-prep-{game_id_str}"),
            scope: NvapiSettingOperationScope::Game {
                game_id: game.id().as_str(),
            },
            target_id: target_name,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            profile_identity_json: &identity_json,
            composition_json: None,
        })
        .expect("finish claim");

    (game, managed_file)
}

#[test]
fn scenario_01_shared_claim_removal_of_game_a_retains_game_b_and_does_not_write_drs() {
    let temp = tempfile::tempdir().expect("temp");
    let install_a = temp.path().join("Game A");
    let install_b = temp.path().join("Game B");
    std::fs::create_dir_all(&install_a).expect("install_a");
    std::fs::create_dir_all(&install_b).expect("install_b");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Shared Target";
    let setting_id = 0x10B3_292C;

    let (game_a, _) =
        seed_game_with_external_claim(&context, &install_a, "game-a", target, setting_id);
    let (game_b, _) = seed_second_shared_claim(&context, &install_b, "game-b", target, setting_id);

    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        2
    );

    let mock = Arc::new(MockExternalClaimDrs::new());
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let result = remove_game_from_catalog(&context, game_a.id()).expect("remove game a");
    assert_eq!(result.game_id, game_a.id().as_str());

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        1
    );
    assert!(
        context
            .storage()
            .find_game(game_a.id())
            .expect("find")
            .is_none()
    );
    assert!(
        context
            .storage()
            .find_game(game_b.id())
            .expect("find")
            .is_some()
    );
}

#[test]
fn scenario_02_shared_claim_final_game_b_release_follows_last_reference_policy() {
    let temp = tempfile::tempdir().expect("temp");
    let install_a = temp.path().join("Game A");
    let install_b = temp.path().join("Game B");
    std::fs::create_dir_all(&install_a).expect("install_a");
    std::fs::create_dir_all(&install_b).expect("install_b");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Shared Target";
    let setting_id = 0x10B3_292C;

    let (game_a, _) =
        seed_game_with_external_claim(&context, &install_a, "game-a", target, setting_id);
    let (game_b, _) = seed_second_shared_claim(&context, &install_b, "game-b", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::new());
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    remove_game_from_catalog(&context, game_a.id()).expect("remove game a");
    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);

    let result = remove_game_from_catalog(&context, game_b.id()).expect("remove game b");
    assert_eq!(result.game_id, game_b.id().as_str());

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        0
    );
    assert!(
        context
            .storage()
            .find_game(game_b.id())
            .expect("find")
            .is_none()
    );
}

#[test]
fn scenario_03_final_claim_live_equals_expected_verified_original_restoration_succeeds() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::new());
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let result = remove_game_from_catalog(&context, game.id()).expect("remove");
    assert_eq!(result.game_id, game.id().as_str());

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        0
    );
}

#[test]
fn scenario_04_final_claim_live_already_equals_original_no_redundant_driver_write() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: format!(
                r#"{{"app_name":"{}/Game.exe"}}"#,
                install.to_string_lossy().replace('\\', "/")
            ),
            live: (false, None),
        },
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let result = remove_game_from_catalog(&context, game.id()).expect("remove");
    assert_eq!(result.game_id, game.id().as_str());

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        0
    );
}

#[test]
fn scenario_05_final_claim_original_explicit_zero_is_preserved_as_set_zero() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_custom_claim(
        &context,
        &install,
        "game-1",
        target,
        setting_id,
        NvapiSettingState {
            present: true,
            value: Some(0),
        },
        NvapiSettingState {
            present: true,
            value: Some(1),
        },
    );

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: format!(
                r#"{{"app_name":"{}/Game.exe"}}"#,
                install.to_string_lossy().replace('\\', "/")
            ),
            live: (true, Some(1)),
        },
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let result = remove_game_from_catalog(&context, game.id()).expect("remove");
    assert_eq!(result.game_id, game.id().as_str());

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        0
    );
}

#[test]
fn scenario_06_final_claim_original_absent_state_is_restored_through_delete() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_custom_claim(
        &context,
        &install,
        "game-1",
        target,
        setting_id,
        NvapiSettingState {
            present: false,
            value: None,
        },
        NvapiSettingState {
            present: true,
            value: Some(1),
        },
    );

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: format!(
                r#"{{"app_name":"{}/Game.exe"}}"#,
                install.to_string_lossy().replace('\\', "/")
            ),
            live: (true, Some(1)),
        },
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let result = remove_game_from_catalog(&context, game.id()).expect("remove");
    assert_eq!(result.game_id, game.id().as_str());

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        0
    );
}

#[test]
fn scenario_07_final_claim_confirmed_absent_target_permits_safe_release_without_drs_mutation() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::ConfirmedAbsent,
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let result = remove_game_from_catalog(&context, game.id()).expect("remove");
    assert_eq!(result.game_id, game.id().as_str());

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        0
    );
}

#[test]
fn scenario_08_diverged_setting_blocks_ordinary_removal_without_explicit_approval() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, managed_file) =
        seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: format!(
                r#"{{"app_name":"{}/Game.exe"}}"#,
                install.to_string_lossy().replace('\\', "/")
            ),
            live: (true, Some(999)),
        },
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let assessment = assess_game_removal(&context, game.id()).expect("assess");
    assert!(!assessment.can_remove_automatically);
    assert!(assessment.nvapi_assessment.requires_user_authorization);
    assert_eq!(assessment.nvapi_assessment.claims.len(), 1);
    assert_eq!(
        assessment.nvapi_assessment.claims[0].disposition,
        NvapiClaimDisposition::RequiresAbandonmentAuthorization
    );

    let error = remove_game_from_catalog(&context, game.id()).expect_err("must block");
    assert!(
        matches!(
            error,
            ServiceError::NvapiClaimReleaseBlocked {
                reason: NvapiClaimBlockedReason::RequiredAuthorizationMissing,
                ..
            }
        ),
        "expected RequiredAuthorizationMissing, got {error:?}"
    );
    assert!(
        error
            .to_string()
            .contains("explicit user confirmation is required to abandon this claim")
    );

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert!(managed_file.exists(), "managed file preserved");
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        1
    );
}

#[test]
fn scenario_09_diverged_setting_authorized_abandonment_does_not_mutate_drs() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: format!(
                r#"{{"app_name":"{}/Game.exe"}}"#,
                install.to_string_lossy().replace('\\', "/")
            ),
            live: (true, Some(999)),
        },
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let auth = NvapiClaimAbandonmentAuthorization {
        game_id: game.id().as_str().to_owned(),
        claims: vec![NvapiClaimAbandonmentToken {
            target_id: target.to_owned(),
            setting_id,
            expected_present: true,
            expected_value: Some(1),
            original_present: false,
            original_value: None,
            observed_live_present: Some(true),
            observed_live_value: Some(999),
        }],
    };

    let result = remove_game_from_catalog_with_options(
        &context,
        game.id(),
        &RemoveGameOptions {
            abandonment_authorization: Some(auth),
        },
    )
    .expect("remove with authorization");

    assert_eq!(result.game_id, game.id().as_str());

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        0
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_none()
    );
}

#[test]
fn scenario_10_stale_approval_or_changed_claim_receipt_is_rejected() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: format!(
                r#"{{"app_name":"{}/Game.exe"}}"#,
                install.to_string_lossy().replace('\\', "/")
            ),
            live: (true, Some(888)),
        },
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let stale_auth = NvapiClaimAbandonmentAuthorization {
        game_id: game.id().as_str().to_owned(),
        claims: vec![NvapiClaimAbandonmentToken {
            target_id: target.to_owned(),
            setting_id,
            expected_present: true,
            expected_value: Some(1),
            original_present: false,
            original_value: None,
            observed_live_present: Some(true),
            observed_live_value: Some(777),
        }],
    };

    let error = remove_game_from_catalog_with_options(
        &context,
        game.id(),
        &RemoveGameOptions {
            abandonment_authorization: Some(stale_auth),
        },
    )
    .expect_err("stale authorization must reject");

    assert!(
        matches!(error, ServiceError::NvapiClaimAuthorizationStale { .. }),
        "expected NvapiClaimAuthorizationStale, got {error:?}"
    );
    assert!(error.to_string().contains("stale authorization"));
    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        1
    );
}

#[test]
fn scenario_11_witness_mismatch_blocks_restore_and_protects_foreign_profile() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::WitnessMismatch {
            detail: "executable is bound to a different profile: Other Profile".to_owned(),
            live: None,
        },
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let error = remove_game_from_catalog(&context, game.id()).expect_err("must block");
    assert!(
        matches!(
            error,
            ServiceError::NvapiClaimReleaseBlocked {
                reason: NvapiClaimBlockedReason::WitnessMismatch,
                ..
            }
        ),
        "expected WitnessMismatch, got {error:?}"
    );
    assert!(
        error
            .to_string()
            .contains("executable is bound to a different profile")
    );

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        1
    );
}

#[test]
fn scenario_12_ambiguous_failed_drs_observation_fails_closed_and_retains_claims() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::FailedOrAmbiguous("NVIDIA driver is unavailable".to_owned()),
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let error = remove_game_from_catalog(&context, game.id()).expect_err("fail closed");
    assert!(
        matches!(
            error,
            ServiceError::NvapiClaimReleaseBlocked {
                reason: NvapiClaimBlockedReason::UncertainObservation,
                ..
            }
        ),
        "expected UncertainObservation, got {error:?}"
    );
    assert!(error.to_string().contains("NVIDIA driver is unavailable"));

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        1
    );
}

#[test]
fn scenario_13_pending_intent_and_conflict_operations_block_release() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) = seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    context
        .storage()
        .begin_nvapi_operation(
            "op-pending",
            Some(game.id().as_str()),
            Some(target),
            "setting",
            r#"{"setting_id":280176940}"#,
            r#"{"setting_id":280176940}"#,
        )
        .expect("begin op");

    let mock = Arc::new(MockExternalClaimDrs::new());
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let error = remove_game_from_catalog(&context, game.id()).expect_err("blocked by pending");
    assert!(
        error
            .to_string()
            .contains("durable DRS operation must be reconciled")
    );

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some()
    );
}

#[test]
fn scenario_14_nvapi_failure_occurs_before_filesystem_cleanup_preserving_files_and_baselines() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, managed_file) =
        seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::new());
    mock.set_restore_error("simulated hardware communication failure");
    let _guard = set_test_external_setting_claim_drs(mock);

    let error = remove_game_from_catalog(&context, game.id()).expect_err("restore failed");
    assert!(
        error
            .to_string()
            .contains("simulated hardware communication failure")
    );

    assert!(
        managed_file.exists(),
        "managed addon file must be preserved on NVAPI error"
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        1
    );
}

#[test]
fn scenario_15_multiple_claims_with_conflicting_second_claim_blocks_all_cleanups() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_1 = 0x10B3_292C;
    let setting_2 = 0x10B3_292D;

    let (game, managed_file) =
        seed_game_with_external_claim(&context, &install, "game-1", target, setting_1);

    let exe_path = format!("{}/Game.exe", install.to_string_lossy().replace('\\', "/"));
    let identity_json = format!(r#"{{"name":"{target}","is_predefined":true}}"#);
    let witness_json = format!(r#"{{"app_name":"{exe_path}"}}"#);

    context
        .storage()
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-claim-2",
            game_id: game.id().as_str(),
            profile_name: target,
            target_kind: "external",
            profile_is_predefined: true,
            profile_identity_json: &identity_json,
            executable_path: &exe_path,
            application_witness_json: &witness_json,
            setting_id: setting_2,
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
        .expect("prep claim 2");

    context
        .storage()
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-claim-2",
            scope: NvapiSettingOperationScope::Game {
                game_id: game.id().as_str(),
            },
            target_id: target,
            setting_id: setting_2,
            expected: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            profile_identity_json: &identity_json,
            composition_json: None,
        })
        .expect("finish claim 2");

    let mock = Arc::new(MockExternalClaimDrs::new());
    mock.set_observation(
        target,
        setting_1,
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: witness_json.clone(),
            live: (true, Some(1)),
        },
    );
    mock.set_observation(
        target,
        setting_2,
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json,
            live: (true, Some(555)),
        },
    );
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let error = remove_game_from_catalog(&context, game.id()).expect_err("preflight blocks");
    assert!(
        error
            .to_string()
            .contains("explicit user confirmation is required")
    );

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert!(managed_file.exists());
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_1)
            .expect("refs"),
        1
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_2)
            .expect("refs"),
        1
    );
}

#[test]
fn scenario_16_game_removal_succeeds_and_leaves_unmanaged_files_untouched() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game");
    std::fs::create_dir_all(&install).expect("install");
    let unmanaged_file = install.join("UnmanagedData.bin");
    std::fs::write(&unmanaged_file, b"user-save-game-or-asset").expect("write unmanaged");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, managed_file) =
        seed_game_with_external_claim(&context, &install, "game-1", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::new());
    let _guard = set_test_external_setting_claim_drs(mock);

    let result = remove_game_from_catalog(&context, game.id()).expect("remove");
    assert_eq!(result.game_id, game.id().as_str());

    assert!(!managed_file.exists(), "managed file cleaned up");
    assert!(
        unmanaged_file.exists(),
        "unmanaged user file must remain untouched"
    );
    assert_eq!(
        std::fs::read(&unmanaged_file).expect("read"),
        b"user-save-game-or-asset"
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_none()
    );
}

#[test]
fn scenario_17_existing_owned_profile_deletion_contract_remains_unaffected() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Owned Profile Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let profile = "RenderPilot - Game [owner]";
    let (game, _) =
        seed_local_owned_profile(&context, &install, "owned-game", profile, 0x10B3_292C);

    let mock = Arc::new(MockExternalClaimDrs::new());
    let _guard = set_test_external_setting_claim_drs(mock);

    let error =
        remove_game_from_catalog(&context, game.id()).expect_err("owned profile must block");
    assert!(
        error
            .to_string()
            .contains("delete the owned NVIDIA profile")
    );

    let assessment = assess_game_removal(&context, game.id()).expect("assess");
    assert!(!assessment.can_remove_automatically);
    assert_eq!(
        assessment.nvapi_assessment.blocked_reason.as_deref(),
        Some(
            "delete the owned NVIDIA profile from the game details page before removing this game"
        )
    );
}

#[test]
fn scenario_18_authorization_for_game_a_cannot_release_game_b_claim() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Game B");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game_b, _) =
        seed_game_with_external_claim(&context, &install, "game-b", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: "{}".to_owned(),
            live: (true, Some(999)),
        },
    ));
    let _guard = set_test_external_setting_claim_drs(mock);

    let auth_for_game_a = NvapiClaimAbandonmentAuthorization {
        game_id: "game-a".to_owned(),
        claims: vec![NvapiClaimAbandonmentToken {
            target_id: target.to_owned(),
            setting_id,
            expected_present: true,
            expected_value: Some(1),
            original_present: false,
            original_value: None,
            observed_live_present: Some(true),
            observed_live_value: Some(999),
        }],
    };
    let options = RemoveGameOptions {
        abandonment_authorization: Some(auth_for_game_a),
    };
    let error = remove_game_from_catalog_with_options(&context, game_b.id(), &options)
        .expect_err("authorization for different game must fail");
    assert!(
        matches!(error, ServiceError::NvapiClaimAuthorizationStale { .. }),
        "expected NvapiClaimAuthorizationStale, got {error:?}"
    );
    assert!(
        error
            .to_string()
            .contains("authorization game id does not match"),
        "unexpected error: {error}"
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        1
    );
}

#[test]
fn scenario_19_authorization_for_one_setting_cannot_release_another_divergent_setting() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Multi Divergent");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_1 = 0x10B3_292C;
    let setting_2 = 0x10B3_292D;
    let (game, _) =
        seed_game_with_external_claim(&context, &install, "game-multi", target, setting_1);

    let identity_json = format!(r#"{{"name":"{target}","is_predefined":true}}"#);
    let exe_path = format!("{}/Game.exe", install.to_string_lossy().replace('\\', "/"));
    let witness_json = format!(r#"{{"app_name":"{exe_path}"}}"#);
    context
        .storage()
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-claim-2",
            game_id: game.id().as_str(),
            profile_name: target,
            target_kind: "external",
            profile_is_predefined: true,
            profile_identity_json: &identity_json,
            executable_path: &exe_path,
            application_witness_json: &witness_json,
            setting_id: setting_2,
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
        .expect("prep claim 2");

    context
        .storage()
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-claim-2",
            scope: NvapiSettingOperationScope::Game {
                game_id: game.id().as_str(),
            },
            target_id: target,
            setting_id: setting_2,
            expected: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            profile_identity_json: &identity_json,
            composition_json: None,
        })
        .expect("finish claim 2");

    let mock = Arc::new(MockExternalClaimDrs::new());
    mock.set_observation(
        target,
        setting_1,
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: "{}".to_owned(),
            live: (true, Some(888)),
        },
    );
    mock.set_observation(
        target,
        setting_2,
        ObservedClaimDrs::Verified {
            profile_name: target.to_owned(),
            is_predefined: true,
            witness_json: "{}".to_owned(),
            live: (true, Some(999)),
        },
    );
    let _guard = set_test_external_setting_claim_drs(mock);

    let auth_only_setting_1 = NvapiClaimAbandonmentAuthorization {
        game_id: game.id().as_str().to_owned(),
        claims: vec![NvapiClaimAbandonmentToken {
            target_id: target.to_owned(),
            setting_id: setting_1,
            expected_present: true,
            expected_value: Some(1),
            original_present: false,
            original_value: None,
            observed_live_present: Some(true),
            observed_live_value: Some(888),
        }],
    };
    let options = RemoveGameOptions {
        abandonment_authorization: Some(auth_only_setting_1),
    };
    let error = remove_game_from_catalog_with_options(&context, game.id(), &options)
        .expect_err("partial authorization must block");
    assert!(
        matches!(error, ServiceError::NvapiClaimAuthorizationStale { .. }),
        "expected NvapiClaimAuthorizationStale, got {error:?}"
    );
    assert!(
        error
            .to_string()
            .contains("authorization missing required token for setting claim"),
        "unexpected error: {error}"
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_1)
            .expect("refs"),
        1
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_2)
            .expect("refs"),
        1
    );
}

#[test]
fn scenario_20_witness_mismatch_with_populated_authorization_is_blocked() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Witness Mismatch Auth");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) =
        seed_game_with_external_claim(&context, &install, "game-wm", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::WitnessMismatch {
            detail: "executable is bound to a different profile: Rogue Profile".to_owned(),
            live: Some((true, Some(555))),
        },
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let auth = NvapiClaimAbandonmentAuthorization {
        game_id: game.id().as_str().to_owned(),
        claims: vec![NvapiClaimAbandonmentToken {
            target_id: target.to_owned(),
            setting_id,
            expected_present: true,
            expected_value: Some(1),
            original_present: false,
            original_value: None,
            observed_live_present: Some(true),
            observed_live_value: Some(555),
        }],
    };
    let options = RemoveGameOptions {
        abandonment_authorization: Some(auth),
    };

    let error = remove_game_from_catalog_with_options(&context, game.id(), &options)
        .expect_err("witness mismatch with authorization must still block");
    assert!(
        matches!(
            error,
            ServiceError::NvapiClaimReleaseBlocked {
                reason: NvapiClaimBlockedReason::WitnessMismatch,
                ..
            }
        ),
        "expected WitnessMismatch, got {error:?}"
    );
    assert!(
        error.to_string().contains("witness changed")
            || error.to_string().contains("cannot safely release"),
        "unexpected error: {error}"
    );

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some(),
        "game must not be removed"
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        1
    );
}

#[test]
fn scenario_21_rebound_executable_and_missing_witness_fail_closed_in_assessment_and_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Witness Assessment");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) =
        seed_game_with_external_claim(&context, &install, "game-assess-wm", target, setting_id);

    // Subcase 1: Rebound executable
    let mock_rebound = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::WitnessMismatch {
            detail: "executable is bound to a different profile: Other".to_owned(),
            live: None,
        },
    ));
    {
        let _guard = set_test_external_setting_claim_drs(mock_rebound);
        let assessment = assess_game_removal(&context, game.id()).expect("assess");
        assert!(!assessment.can_remove_automatically);
        assert!(!assessment.nvapi_assessment.requires_user_authorization);
        assert!(
            assessment
                .nvapi_assessment
                .blocked_reason
                .as_deref()
                .unwrap_or("")
                .contains("executable witness changed"),
            "blocked reason should explain witness changed: {:?}",
            assessment.nvapi_assessment.blocked_reason
        );
        assert_eq!(
            assessment.nvapi_assessment.claims[0].disposition,
            NvapiClaimDisposition::Blocked
        );
    }

    // Subcase 2: Missing application witness
    let mock_missing = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::WitnessMismatch {
            detail: "target profile exists but application witness is not associated with it"
                .to_owned(),
            live: None,
        },
    ));
    {
        let _guard = set_test_external_setting_claim_drs(mock_missing);
        let assessment = assess_game_removal(&context, game.id()).expect("assess");
        assert!(!assessment.can_remove_automatically);
        assert!(!assessment.nvapi_assessment.requires_user_authorization);
        assert!(
            assessment
                .nvapi_assessment
                .blocked_reason
                .as_deref()
                .unwrap_or("")
                .contains("executable witness changed"),
            "blocked reason should explain witness changed: {:?}",
            assessment.nvapi_assessment.blocked_reason
        );
        assert_eq!(
            assessment.nvapi_assessment.claims[0].disposition,
            NvapiClaimDisposition::Blocked
        );
    }
}

#[test]
fn scenario_22_failed_or_ambiguous_observation_with_authorization_fails_closed() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Ambiguous Auth");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target = "NVIDIA Target";
    let setting_id = 0x10B3_292C;
    let (game, _) =
        seed_game_with_external_claim(&context, &install, "game-ambig", target, setting_id);

    let mock = Arc::new(MockExternalClaimDrs::with_default(
        ObservedClaimDrs::FailedOrAmbiguous("driver query timed out".to_owned()),
    ));
    let _guard = set_test_external_setting_claim_drs(mock.clone());

    let auth = NvapiClaimAbandonmentAuthorization {
        game_id: game.id().as_str().to_owned(),
        claims: vec![NvapiClaimAbandonmentToken {
            target_id: target.to_owned(),
            setting_id,
            expected_present: true,
            expected_value: Some(1),
            original_present: false,
            original_value: None,
            observed_live_present: Some(true),
            observed_live_value: Some(1),
        }],
    };
    let options = RemoveGameOptions {
        abandonment_authorization: Some(auth),
    };

    let error = remove_game_from_catalog_with_options(&context, game.id(), &options)
        .expect_err("failed or ambiguous observation must fail closed even with auth");
    assert!(
        matches!(
            error,
            ServiceError::NvapiClaimReleaseBlocked {
                reason: NvapiClaimBlockedReason::UncertainObservation,
                ..
            }
        ),
        "expected UncertainObservation, got {error:?}"
    );
    assert!(
        error
            .to_string()
            .contains("could not verify NVIDIA claim target")
            || error.to_string().contains("timed out"),
        "unexpected error: {error}"
    );

    assert_eq!(mock.restore_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target, setting_id)
            .expect("refs"),
        1
    );
}
