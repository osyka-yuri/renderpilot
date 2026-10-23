use super::*;
use crate::nvapi::profiles::{
    DeleteLookup, ObservedProfileReceipt, OwnedProfileDeleteDrs, OwnedProfileObservation,
    delete_owned_profile_with_drs,
};
use renderpilot_nvapi::setting::SettingContext;

struct TestDeleteDrs {
    observations:
        std::sync::Mutex<std::collections::VecDeque<Result<OwnedProfileObservation, ServiceError>>>,
    live_delete_attempted: std::sync::atomic::AtomicBool,
    mutation_attempted: std::sync::atomic::AtomicBool,
}

impl TestDeleteDrs {
    fn new(sequence: Vec<Result<OwnedProfileObservation, ServiceError>>) -> Self {
        Self {
            observations: std::sync::Mutex::new(std::collections::VecDeque::from(sequence)),
            live_delete_attempted: std::sync::atomic::AtomicBool::new(false),
            mutation_attempted: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn confirmed_absent() -> Self {
        Self::new(vec![
            Ok(OwnedProfileObservation {
                by_name: DeleteLookup::Missing,
                by_path: DeleteLookup::Missing,
            }),
            Ok(OwnedProfileObservation {
                by_name: DeleteLookup::Missing,
                by_path: DeleteLookup::Missing,
            }),
        ])
    }
}

impl OwnedProfileDeleteDrs for TestDeleteDrs {
    fn observe(
        &self,
        _owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
    ) -> Result<OwnedProfileObservation, ServiceError> {
        let mut queue = self.observations.lock().unwrap();
        if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else if let Some(last) = queue.front() {
            last.clone()
        } else {
            Err(ServiceError::command_failed("no observations configured"))
        }
    }

    fn execute_live_delete(
        &self,
        _context: &crate::Context,
        _game_id: &str,
        _owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
        _observation: &OwnedProfileObservation,
    ) -> Result<(), ServiceError> {
        self.live_delete_attempted
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.mutation_attempted
            .store(true, std::sync::atomic::Ordering::SeqCst);
        panic!("unexpected live delete in GPU-free test");
    }
}

fn test_setting_context(install: &std::path::Path, exe_path: Option<&str>) -> SettingContext {
    SettingContext {
        game_install_dir: install.to_path_buf(),
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

fn observed_receipt(name: &str, binding: &str, is_predefined: bool) -> ObservedProfileReceipt {
    let app = renderpilot_nvapi::ApplicationIdentity {
        app_name: binding.to_owned(),
        user_friendly_name: "Test Fixture".to_owned(),
        launcher: String::new(),
        file_in_folder: String::new(),
        flags: 0,
        command_line: String::new(),
        is_predefined,
    };
    ObservedProfileReceipt {
        identity: renderpilot_nvapi::ProfileIdentity {
            name: name.to_owned(),
            is_predefined,
            application_count: 1,
            setting_count: 0,
        },
        witness: Some(app.clone()),
        witness_json: Some(format!(r#"{{"app_name":"{binding}"}}"#)),
        applications: vec![app],
        settings: Vec::new(),
        composition: r#"{"applications":[],"settings":[]}"#.to_owned(),
    }
}

fn seed_game_with_owned_profile(
    context: &crate::Context,
    install: &std::path::Path,
    game_id_str: &str,
    profile_name: &str,
    setting_id: u32,
) -> (GameInstallation, String) {
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

    (game, binding)
}

#[test]
fn e2e_confirmed_absent_release_then_catalog_removal_succeeds() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("E2E Game");
    std::fs::create_dir_all(&install).expect("install dir");
    let sentinel = install.join("unmanaged_user_save.dat");
    std::fs::write(&sentinel, b"user-important-save-data").expect("sentinel");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let profile_name = "RenderPilot - E2E Game [owner]";
    let setting_id = 0x10B3_292C;

    let (game, binding) = seed_game_with_owned_profile(
        &context,
        &install,
        "manual:e2e-game",
        profile_name,
        setting_id,
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .get_nvapi_target_profile_is_predefined(profile_name)
            .expect("predefined"),
        Some(false)
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(profile_name, setting_id)
            .expect("claim refs"),
        1
    );
    assert_eq!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game.id().as_str())
            .expect("claims")
            .len(),
        1
    );
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
            .find_game(game.id())
            .expect("find")
            .is_some()
    );
    assert!(sentinel.exists());

    let drs = TestDeleteDrs::confirmed_absent();
    let setting_context = test_setting_context(&install, Some(&binding));
    let release_res =
        delete_owned_profile_with_drs(&context, game.id().as_str(), &setting_context, &drs);
    assert!(
        release_res.is_ok(),
        "ConfirmedAbsent release must succeed: {release_res:?}"
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_none(),
        "owned profile receipt must be released"
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(profile_name, setting_id)
            .expect("claim refs"),
        0,
        "claim reference count must drop to 0"
    );
    assert!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game.id().as_str())
            .expect("claims")
            .is_empty(),
        "claims for game must be empty"
    );
    assert!(
        context
            .storage()
            .get_nvapi_application_witness(profile_name, &binding)
            .expect("witness")
            .is_none(),
        "witness must be released"
    );
    assert!(
        context
            .storage()
            .get_nvapi_target_profile_is_predefined(profile_name)
            .expect("predefined")
            .is_none(),
        "target row must be deleted"
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some(),
        "game must remain registered immediately after release"
    );
    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("pending")
            .is_empty(),
        "no pending operations should remain"
    );
    assert!(sentinel.exists(), "unmanaged sentinel must survive release");
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst),
        "no live delete must be attempted"
    );
    assert!(
        !drs.mutation_attempted
            .load(std::sync::atomic::Ordering::SeqCst),
        "no live mutation must be attempted"
    );

    let remove_res = remove_game_from_catalog(&context, game.id());
    assert!(
        remove_res.is_ok(),
        "catalog removal must succeed after release: {remove_res:?}"
    );
    assert_eq!(remove_res.unwrap().game_id, game.id().as_str());

    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_none(),
        "game row must be absent from catalog"
    );
    assert!(
        install.is_dir(),
        "installation directory must remain intact"
    );
    assert!(sentinel.exists(), "unmanaged sentinel file must survive");
    assert_eq!(
        std::fs::read(&sentinel).expect("read sentinel"),
        b"user-important-save-data"
    );
}

#[test]
fn e2e_prerelease_removal_is_blocked_until_confirmed_absent_release() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Pre-release Blocked Game");
    std::fs::create_dir_all(&install).expect("install dir");
    let sentinel = install.join("unmanaged_user_save.dat");
    std::fs::write(&sentinel, b"user-important-save-data").expect("sentinel");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let profile_name = "RenderPilot - Blocked Game [owner]";
    let setting_id = 0x10B3_292C;

    let (game, binding) = seed_game_with_owned_profile(
        &context,
        &install,
        "manual:prerelease-game",
        profile_name,
        setting_id,
    );

    let err = remove_game_from_catalog(&context, game.id())
        .expect_err("catalog removal before release must fail");
    assert!(
        err.to_string().contains(
            "delete the owned NVIDIA profile from the game details page before removing this game",
        ),
        "error must match existing preflight message: {err}"
    );

    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some()
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game.id().as_str())
            .expect("claims")
            .len(),
        1
    );
    assert!(sentinel.exists());

    let drs = TestDeleteDrs::confirmed_absent();
    let setting_context = test_setting_context(&install, Some(&binding));
    let release_res =
        delete_owned_profile_with_drs(&context, game.id().as_str(), &setting_context, &drs);
    assert!(release_res.is_ok());

    let remove_res = remove_game_from_catalog(&context, game.id());
    assert!(remove_res.is_ok(), "retry must succeed: {remove_res:?}");
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_none()
    );
    assert!(sentinel.exists());
}

#[test]
fn e2e_managed_filesystem_cleanup_runs_only_after_owned_profile_release() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Cleanup Game");
    std::fs::create_dir_all(&install).expect("install dir");
    let sentinel = install.join("unmanaged_user_save.dat");
    std::fs::write(&sentinel, b"user-important-save-data").expect("sentinel");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let profile_name = "RenderPilot - Managed Cleanup Game [owner]";
    let setting_id = 0x10B3_292C;

    let (game, binding) = seed_game_with_owned_profile(
        &context,
        &install,
        "manual:managed-cleanup-game",
        profile_name,
        setting_id,
    );

    let managed_addon_file = install.join("managed_luma.addon64");
    std::fs::write(&managed_addon_file, b"managed-luma-content").expect("write luma");
    let addon = InstalledAddon::new(
        game.id().clone(),
        AddonKind::Luma,
        path_ref(&managed_addon_file),
    )
    .with_created_file(path_ref(&managed_addon_file));
    context
        .storage()
        .upsert_installed_addon(&addon)
        .expect("upsert addon");

    let err = remove_game_from_catalog(&context, game.id()).expect_err("must fail before release");
    assert!(
        err.to_string().contains("delete the owned NVIDIA profile"),
        "preflight must fail with owned profile message: {err}"
    );

    assert!(
        managed_addon_file.exists(),
        "managed file must not be touched before release"
    );
    assert!(sentinel.exists(), "sentinel must remain");
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("addon")
            .is_some()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("find")
            .is_some()
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_some()
    );

    let drs = TestDeleteDrs::confirmed_absent();
    let setting_context = test_setting_context(&install, Some(&binding));
    let release_res =
        delete_owned_profile_with_drs(&context, game.id().as_str(), &setting_context, &drs);
    assert!(release_res.is_ok());

    assert!(managed_addon_file.exists());
    assert!(sentinel.exists());

    let remove_res = remove_game_from_catalog(&context, game.id());
    assert!(
        remove_res.is_ok(),
        "removal must succeed after release: {remove_res:?}"
    );

    assert!(
        !managed_addon_file.exists(),
        "managed add-on file must be deleted by catalog cleanup"
    );
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("addon")
            .is_none()
    );

    assert!(sentinel.exists(), "unmanaged sentinel must survive");
    assert_eq!(
        std::fs::read(&sentinel).expect("read"),
        b"user-important-save-data"
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
fn e2e_failed_release_missing_by_name_unrelated_by_path_does_not_unblock_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Unrelated Path Game");
    std::fs::create_dir_all(&install).expect("install dir");
    let sentinel = install.join("unmanaged_user_save.dat");
    std::fs::write(&sentinel, b"save").expect("sentinel");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let profile_name = "RenderPilot - Unrelated Path Game [owner]";
    let setting_id = 0x10B3_292C;

    let (game, binding) = seed_game_with_owned_profile(
        &context,
        &install,
        "manual:unrelated-path-game",
        profile_name,
        setting_id,
    );
    let managed_addon = install.join("managed.addon64");
    std::fs::write(&managed_addon, b"addon").expect("addon");
    context
        .storage()
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::Luma,
            path_ref(&managed_addon),
        ))
        .expect("addon");

    let unrelated_receipt = observed_receipt("Unrelated Other Profile", &binding, true);
    let drs = TestDeleteDrs::new(vec![Ok(OwnedProfileObservation {
        by_name: DeleteLookup::Missing,
        by_path: DeleteLookup::Found(unrelated_receipt),
    })]);
    let setting_context = test_setting_context(&install, Some(&binding));
    let release_res =
        delete_owned_profile_with_drs(&context, game.id().as_str(), &setting_context, &drs);
    assert!(
        release_res.is_err(),
        "release must be blocked by unrelated profile at path"
    );

    let remove_err =
        remove_game_from_catalog(&context, game.id()).expect_err("removal must remain blocked");
    assert!(
        remove_err
            .to_string()
            .contains("delete the owned NVIDIA profile")
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_some()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game.id().as_str())
            .expect("claims")
            .len(),
        1
    );
    assert!(
        managed_addon.exists(),
        "managed addon file must not be touched"
    );
    assert!(sentinel.exists());
    assert!(
        !drs.live_delete_attempted
            .load(std::sync::atomic::Ordering::SeqCst)
    );
}

#[test]
fn e2e_failed_release_contradictory_lookup_does_not_unblock_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Contradictory Game");
    std::fs::create_dir_all(&install).expect("install dir");
    let sentinel = install.join("unmanaged_user_save.dat");
    std::fs::write(&sentinel, b"save").expect("sentinel");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let profile_name = "RenderPilot - Contradictory Game [owner]";
    let setting_id = 0x10B3_292C;

    let (game, binding) = seed_game_with_owned_profile(
        &context,
        &install,
        "manual:contradictory-game",
        profile_name,
        setting_id,
    );

    let matching_receipt = observed_receipt(profile_name, &binding, false);
    let drs = TestDeleteDrs::new(vec![Ok(OwnedProfileObservation {
        by_name: DeleteLookup::Missing,
        by_path: DeleteLookup::Found(matching_receipt),
    })]);
    let setting_context = test_setting_context(&install, Some(&binding));
    let release_res =
        delete_owned_profile_with_drs(&context, game.id().as_str(), &setting_context, &drs);
    assert!(
        release_res.is_err(),
        "release must be blocked on contradictory observation"
    );

    let remove_err =
        remove_game_from_catalog(&context, game.id()).expect_err("removal must remain blocked");
    assert!(
        remove_err
            .to_string()
            .contains("delete the owned NVIDIA profile")
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_some()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_some()
    );
    assert_eq!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game.id().as_str())
            .expect("claims")
            .len(),
        1
    );
    assert!(sentinel.exists());
}

#[test]
fn e2e_failed_release_failed_lookup_does_not_unblock_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Failed Lookup Game");
    std::fs::create_dir_all(&install).expect("install dir");
    let sentinel = install.join("unmanaged_user_save.dat");
    std::fs::write(&sentinel, b"save").expect("sentinel");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let profile_name = "RenderPilot - Failed Lookup Game [owner]";
    let setting_id = 0x10B3_292C;

    let (game, binding) = seed_game_with_owned_profile(
        &context,
        &install,
        "manual:failed-lookup-game",
        profile_name,
        setting_id,
    );

    let drs = TestDeleteDrs::new(vec![Ok(OwnedProfileObservation {
        by_name: DeleteLookup::Failed("driver communication timeout".to_owned()),
        by_path: DeleteLookup::Missing,
    })]);
    let setting_context = test_setting_context(&install, Some(&binding));
    let release_res =
        delete_owned_profile_with_drs(&context, game.id().as_str(), &setting_context, &drs);
    assert!(release_res.is_err());

    let remove_err =
        remove_game_from_catalog(&context, game.id()).expect_err("removal must remain blocked");
    assert!(
        remove_err
            .to_string()
            .contains("delete the owned NVIDIA profile")
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_some()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_some()
    );
    assert!(sentinel.exists());
}

#[test]
fn e2e_failed_release_uncertain_second_observation_does_not_unblock_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Uncertain Second Obs Game");
    std::fs::create_dir_all(&install).expect("install dir");
    let sentinel = install.join("unmanaged_user_save.dat");
    std::fs::write(&sentinel, b"save").expect("sentinel");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let profile_name = "RenderPilot - Uncertain Second Obs [owner]";
    let setting_id = 0x10B3_292C;

    let (game, binding) = seed_game_with_owned_profile(
        &context,
        &install,
        "manual:uncertain-second-obs",
        profile_name,
        setting_id,
    );

    let drs = TestDeleteDrs::new(vec![
        Ok(OwnedProfileObservation {
            by_name: DeleteLookup::Missing,
            by_path: DeleteLookup::Missing,
        }),
        Err(ServiceError::command_failed(
            "driver disconnected during verification",
        )),
    ]);
    let setting_context = test_setting_context(&install, Some(&binding));
    let release_res =
        delete_owned_profile_with_drs(&context, game.id().as_str(), &setting_context, &drs);
    assert!(release_res.is_err());

    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].phase, "intent");

    let remove_err = remove_game_from_catalog(&context, game.id())
        .expect_err("removal must be blocked by pending operation");
    assert!(
        remove_err
            .to_string()
            .contains("a durable DRS operation must be reconciled before removing this game",),
        "error must indicate pending DRS reconciliation: {remove_err}"
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_some()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_some()
    );
    assert!(sentinel.exists());
}

#[test]
fn e2e_failed_release_overlapping_pending_op_does_not_unblock_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Overlapping Pending Game");
    std::fs::create_dir_all(&install).expect("install dir");
    let sentinel = install.join("unmanaged_user_save.dat");
    std::fs::write(&sentinel, b"save").expect("sentinel");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let profile_name = "RenderPilot - Overlapping Game [owner]";
    let setting_id = 0x10B3_292C;

    let (game, binding) = seed_game_with_owned_profile(
        &context,
        &install,
        "manual:overlapping-game",
        profile_name,
        setting_id,
    );

    context
        .storage()
        .begin_nvapi_operation(
            "op-overlapping-preexisting",
            Some(game.id().as_str()),
            Some(profile_name),
            "setting",
            r#"{"setting_id":280176940}"#,
            r#"{"setting_id":280176940}"#,
        )
        .expect("seed pending op");

    let drs = TestDeleteDrs::confirmed_absent();
    let setting_context = test_setting_context(&install, Some(&binding));
    let release_res =
        delete_owned_profile_with_drs(&context, game.id().as_str(), &setting_context, &drs);
    assert!(
        release_res.is_err(),
        "deletion must fail due to overlapping pending operation"
    );

    let remove_err = remove_game_from_catalog(&context, game.id())
        .expect_err("removal must be blocked by pending op");
    assert!(
        remove_err
            .to_string()
            .contains("a durable DRS operation must be reconciled before removing this game",)
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_some()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_some()
    );
    assert!(sentinel.exists());
}

#[test]
fn e2e_failed_release_other_game_claim_does_not_unblock_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Shared Owned Game");
    std::fs::create_dir_all(&install).expect("install dir");
    let sentinel = install.join("unmanaged_user_save.dat");
    std::fs::write(&sentinel, b"save").expect("sentinel");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let profile_name = "RenderPilot - Shared Owned Game [owner]";
    let setting_id = 0x10B3_292C;

    let (game, binding) = seed_game_with_owned_profile(
        &context,
        &install,
        "manual:shared-owned-game",
        profile_name,
        setting_id,
    );

    let install2 = temp.path().join("Game 2");
    std::fs::create_dir_all(&install2).expect("install 2");
    let game2 = GameInstallation::new(
        GameIdentity::new(
            GameId::new("manual:other-claim-game").expect("id"),
            "Game 2",
            Launcher::Manual,
        )
        .expect("id"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(install2.to_string_lossy()).expect("path"),
    )
    .with_root_authority(RootAuthority::UserConfirmed);
    context
        .storage()
        .upsert_game(&game2)
        .expect("upsert game 2");
    let binding2 = format!(
        "{}/Game2.exe",
        install2.to_string_lossy().replace('\\', "/")
    );
    let witness2 = format!(r#"{{"app_name":"{binding2}"}}"#);
    context
        .storage()
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-claim-game2",
            game_id: game2.id().as_str(),
            profile_name,
            target_kind: "renderpilot",
            profile_is_predefined: false,
            profile_identity_json: &format!(r#"{{"name":"{profile_name}","is_predefined":false}}"#),
            executable_path: &binding2,
            application_witness_json: &witness2,
            setting_id: 0x10B3_292D,
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
                value: Some(5),
            },
        })
        .expect("prepare game2 claim");
    context
        .storage()
        .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
            op_id: "op-claim-game2",
            scope: NvapiSettingOperationScope::Game {
                game_id: game2.id().as_str(),
            },
            target_id: profile_name,
            setting_id: 0x10B3_292D,
            expected: NvapiSettingState {
                present: true,
                value: Some(5),
            },
            profile_identity_json: &format!(r#"{{"name":"{profile_name}","is_predefined":false}}"#),
            composition_json: None,
        })
        .expect("finish game2 claim");

    let drs = TestDeleteDrs::confirmed_absent();
    let setting_context = test_setting_context(&install, Some(&binding));
    let release_res =
        delete_owned_profile_with_drs(&context, game.id().as_str(), &setting_context, &drs);
    assert!(
        release_res.is_err(),
        "deletion must fail because another game references the profile"
    );
    let err = release_res.unwrap_err().to_string();
    assert!(err.contains("another game still references a setting on this profile"));

    let remove_err =
        remove_game_from_catalog(&context, game.id()).expect_err("removal must be blocked");
    assert!(
        remove_err
            .to_string()
            .contains("delete the owned NVIDIA profile")
    );

    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner")
            .is_some()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_some()
    );
    assert!(sentinel.exists());
}

#[test]
fn e2e_scope_isolation_preserves_unrelated_game_records_and_claims() {
    let temp = tempfile::tempdir().expect("temp");
    let install_a = temp.path().join("Game A");
    let install_b = temp.path().join("Game B");
    std::fs::create_dir_all(&install_a).expect("install a");
    std::fs::create_dir_all(&install_b).expect("install b");
    let sentinel_b = install_b.join("b_sentinel.dat");
    std::fs::write(&sentinel_b, b"game-b-unmanaged-data").expect("sentinel b");

    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");

    let target_a = "RenderPilot - Game A [owner]";
    let setting_a = 0x10B3_292C;
    let (game_a, binding_a) =
        seed_game_with_owned_profile(&context, &install_a, "manual:game-a", target_a, setting_a);

    let target_b = "NVIDIA Base Target B";
    let setting_b = 0x10f9dc84;
    let (game_b, _) =
        seed_game_with_external_claim(&context, &install_b, "manual:game-b", target_b, setting_b);

    context
        .storage()
        .begin_nvapi_operation(
            "op-game-b-unrelated",
            Some(game_b.id().as_str()),
            Some(target_b),
            "setting",
            r#"{"setting_id":284802180}"#,
            r#"{"setting_id":284802180}"#,
        )
        .expect("seed Game B pending op");

    let drs = TestDeleteDrs::confirmed_absent();
    let setting_context_a = test_setting_context(&install_a, Some(&binding_a));
    let release_a =
        delete_owned_profile_with_drs(&context, game_a.id().as_str(), &setting_context_a, &drs);
    assert!(release_a.is_ok());

    let remove_a = remove_game_from_catalog(&context, game_a.id());
    assert!(remove_a.is_ok());

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
            .get_nvapi_owned_profile(game_a.id().as_str())
            .expect("owner a")
            .is_none()
    );
    assert!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game_a.id().as_str())
            .expect("claims a")
            .is_empty()
    );

    assert!(
        context
            .storage()
            .find_game(game_b.id())
            .expect("find b")
            .is_some()
    );
    let claims_b = context
        .storage()
        .list_nvapi_setting_claims_for_game(game_b.id().as_str())
        .expect("claims b");
    assert_eq!(claims_b.len(), 1);
    assert_eq!(claims_b[0].target_id, target_b);
    assert_eq!(claims_b[0].setting_id, setting_b);
    assert_eq!(claims_b[0].expected_value, Some(1));
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(target_b, setting_b)
            .expect("refs b"),
        1
    );
    assert_eq!(
        context
            .storage()
            .get_nvapi_target_profile_is_predefined(target_b)
            .expect("predefined b"),
        Some(true)
    );
    let pending = context
        .storage()
        .list_pending_nvapi_operations()
        .expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].op_id, "op-game-b-unrelated");
    assert_eq!(pending[0].game_id.as_deref(), Some(game_b.id().as_str()));
    assert!(sentinel_b.exists());
    assert_eq!(
        std::fs::read(&sentinel_b).expect("read"),
        b"game-b-unmanaged-data"
    );
}
