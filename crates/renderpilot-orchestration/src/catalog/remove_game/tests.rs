use renderpilot_application::{
    ComponentRepository, GameRepository, InstalledAddonRepository, OptiScalerStateRepository,
    ProxyTopologyRepository,
};
use renderpilot_domain::{
    AddonKind, ComponentFile, ComponentId, ComponentKind, ComponentRollbackBaseline,
    EngineConfigJournal, EngineConfigTransition, GameIdentity, GameInstallation, GameProxyTopology,
    GameRuntime, InstalledAddon, Launcher, LibraryComponent, LibraryTechnology,
    OptiScalerAdoptionState, OptiScalerFileReceipt, OptiScalerFileRole,
    OptiScalerInstallStateParts, OptiScalerPrerequisiteBinding, PathRef, Platform,
    ProxyImplementation, ProxyLink, ProxyRootPrestate, Sha256Hash, Swappability,
};
use renderpilot_storage_sqlite::{
    AuthorityCas, NvapiGameSettingPreparation, NvapiProfileCreationCompletion,
    NvapiSettingOperationCompletion, NvapiSettingOperationScope, NvapiSettingState,
    NvapiVerifiedProfileReceipt,
};

use super::*;

#[test]
fn removes_user_managed_card_without_touching_the_install_directory() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Manual Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");

    let result = remove_game_from_catalog(&context, game.id()).expect("remove");

    assert_eq!(result.game_id, game.id().as_str());
    assert!(
        install.is_dir(),
        "catalog removal must not delete game files"
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("read")
            .is_none()
    );
}

#[test]
fn absent_registration_is_rejected_before_catalog_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Absent Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");
    let readiness = context
        .storage()
        .catalog_readiness(game.id())
        .expect("read readiness");
    context
        .storage()
        .mark_installation_absent(&game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("mark absent");

    let error = remove_game_from_catalog(&context, game.id()).expect_err("absent must reject");

    assert!(error.to_string().contains("absent installation"));
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game remains registered")
            .is_some()
    );
    assert!(
        install.is_dir(),
        "rejection leaves the installation untouched"
    );
}

#[test]
fn owned_nvapi_profile_requires_explicit_deletion_before_catalog_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Owned NVIDIA Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");
    let profile = "RenderPilot - Game [owner]";
    let binding = format!("{}/Game.exe", install.to_string_lossy().replace('\\', "/"));
    context
        .storage()
        .begin_nvapi_operation(
            "create-owner-receipt",
            Some(game.id().as_str()),
            Some(profile),
            "create_profile",
            r#"{"profile_absent":true}"#,
            r#"{"profile_present":true}"#,
        )
        .expect("journal");
    context
        .storage()
        .complete_nvapi_profile_creation(NvapiProfileCreationCompletion {
            op_id: "create-owner-receipt",
            game_id: game.id().as_str(),
            binding_path: &binding,
            profile: NvapiVerifiedProfileReceipt {
                profile_name: profile,
                profile_identity_json:
                    r#"{"name":"RenderPilot - Game [owner]","is_predefined":false}"#,
                application_witness_json: &format!(r#"{{"app_name":"{binding}"}}"#),
                composition_json: r#"{"applications":[],"settings":[]}"#,
            },
        })
        .expect("owner receipt");

    let setting_id = 0x10B3_292C;
    context
        .storage()
        .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
            op_id: "op-owned-claim",
            game_id: game.id().as_str(),
            profile_name: profile,
            target_kind: "renderpilot",
            profile_is_predefined: false,
            profile_identity_json: r#"{"name":"RenderPilot - Game [owner]","is_predefined":false}"#,
            executable_path: &binding,
            application_witness_json: &format!(r#"{{"app_name":"{binding}"}}"#),
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
            op_id: "op-owned-claim",
            scope: NvapiSettingOperationScope::Game {
                game_id: game.id().as_str(),
            },
            target_id: profile,
            setting_id,
            expected: NvapiSettingState {
                present: true,
                value: Some(1),
            },
            profile_identity_json: r#"{"name":"RenderPilot - Game [owner]","is_predefined":false}"#,
            composition_json: None,
        })
        .expect("finish owned claim");

    let addon_file = install.join("managed.addon64");
    std::fs::write(&addon_file, b"managed addon content").expect("write addon file");
    context
        .storage()
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::Luma,
            path_ref(&addon_file),
        ))
        .expect("seed addon");

    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("pending ops")
            .is_empty(),
        "no pending operations should exist before removal attempt"
    );

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("catalog removal must require explicit profile deletion");

    assert!(
        error
            .to_string()
            .contains("delete the owned NVIDIA profile")
    );
    assert!(
        context
            .storage()
            .get_nvapi_owned_profile(game.id().as_str())
            .expect("owner read")
            .is_some(),
        "ownership receipt must remain"
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game read")
            .is_some(),
        "game record must remain"
    );
    assert_eq!(
        context
            .storage()
            .count_nvapi_setting_claim_refs(profile, setting_id)
            .expect("count claim refs"),
        1,
        "setting claim reference must remain"
    );
    assert_eq!(
        context
            .storage()
            .list_nvapi_setting_claims_for_game(game.id().as_str())
            .expect("list claims")
            .len(),
        1,
        "setting claim row must remain"
    );
    assert!(
        addon_file.exists(),
        "no filesystem managed cleanup must run before preflight rejection"
    );
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("addon read")
            .is_some(),
        "managed addon record must remain"
    );
    assert!(
        context
            .storage()
            .list_pending_nvapi_operations()
            .expect("pending ops")
            .is_empty(),
        "no pending operations must be created or deleted by rejection"
    );
}

#[test]
fn launcher_managed_card_is_not_presented_as_durable_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Launcher Game");
    std::fs::create_dir_all(&install).expect("install");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::LauncherManifest);
    context.storage().upsert_game(&game).expect("seed");

    let error = remove_game_from_catalog(&context, game.id()).expect_err("must reject");

    assert!(error.to_string().contains("launcher-managed"));
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("read")
            .is_some()
    );
}

#[test]
fn active_component_replacement_is_rolled_back_before_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    std::fs::create_dir_all(&install).expect("install");
    let live = install.join("nvngx_dlss.dll");
    let backup = crate::fs::backup_path(&live).expect("backup path");
    std::fs::write(&live, b"replacement").expect("live");
    std::fs::write(&backup, b"original").expect("backup");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");
    let component_id = ComponentId::new("component:managed").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_ref(&live))
            .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash")),
    );
    context
        .storage()
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    context
        .storage()
        .recover_component_rollback_baseline(
            game.id(),
            &component_id,
            &ComponentRollbackBaseline::new(vec![
                ComponentFile::new(path_ref(&live))
                    .with_sha256(renderpilot_detection::sha256_file(&backup).expect("backup hash")),
            ]),
        )
        .expect("baseline");

    let result = remove_game_from_catalog(&context, game.id()).expect("remove");

    assert_eq!(result.game_id, game.id().as_str());
    assert_eq!(
        std::fs::read(&live).expect("restored live file"),
        b"original"
    );
    assert!(
        !backup.exists(),
        "successful rollback must consume its sidecar"
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("read")
            .is_none()
    );
}

#[test]
fn orphaned_component_baseline_is_rolled_back_from_recorded_provenance() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    std::fs::create_dir_all(&install).expect("install");
    let live = install.join("nvngx_dlss.dll");
    let backup = crate::fs::backup_path(&live).expect("backup path");
    std::fs::write(&live, b"replacement").expect("live");
    std::fs::write(&backup, b"original").expect("backup");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");
    let component_id = ComponentId::new("component:orphaned").expect("component");
    let active_file = ComponentFile::new(path_ref(&live))
        .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash"));
    context
        .storage()
        .recover_component_rollback_baseline(
            game.id(),
            &component_id,
            &ComponentRollbackBaseline::new(vec![
                ComponentFile::new(path_ref(&live))
                    .with_sha256(renderpilot_detection::sha256_file(&backup).expect("backup hash")),
            ])
            .with_expected_active_files(vec![active_file]),
        )
        .expect("baseline");
    context
        .storage()
        .replace_components_for_game(game.id(), &[])
        .expect("component disappeared after rescan");

    remove_game_from_catalog(&context, game.id()).expect("remove");

    assert_eq!(
        std::fs::read(&live).expect("restored live file"),
        b"original"
    );
    assert!(!backup.exists(), "successful rollback consumes the sidecar");
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game lookup")
            .is_none()
    );
}

#[test]
fn legacy_orphan_with_unproven_live_bytes_is_bundled_without_mutation() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    std::fs::create_dir_all(&install).expect("install");
    let live = install.join("nvngx_dlss.dll");
    let backup = crate::fs::backup_path(&live).expect("backup path");
    std::fs::write(&live, b"unknown-active").expect("live");
    std::fs::write(&backup, b"original").expect("backup");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");
    let component_id = ComponentId::new("component:legacy-orphan").expect("component");
    context
        .storage()
        .recover_component_rollback_baseline(
            game.id(),
            &component_id,
            &ComponentRollbackBaseline::new(vec![
                ComponentFile::new(path_ref(&live))
                    .with_sha256(renderpilot_detection::sha256_file(&backup).expect("backup hash")),
            ]),
        )
        .expect("legacy baseline");

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("unproven live bytes must not be overwritten");
    let ServiceError::ManagedCleanupAmbiguous {
        recovery_bundle_path,
        ..
    } = error
    else {
        panic!("expected typed cleanup ambiguity");
    };

    assert!(std::path::Path::new(&recovery_bundle_path).is_dir());
    assert_eq!(std::fs::read(&live).expect("live"), b"unknown-active");
    assert_eq!(std::fs::read(&backup).expect("backup"), b"original");
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game lookup")
            .is_some()
    );
}

#[test]
fn failed_automatic_rollback_keeps_the_card_and_recovery_state() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    std::fs::create_dir_all(&install).expect("install");
    let live = install.join("nvngx_dlss.dll");
    std::fs::write(&live, b"replacement").expect("live");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");
    let component_id = ComponentId::new("component:managed").expect("component");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(
        ComponentFile::new(path_ref(&live))
            .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash")),
    );
    context
        .storage()
        .replace_components_for_game(game.id(), &[component])
        .expect("component");
    context
        .storage()
        .recover_component_rollback_baseline(
            game.id(),
            &component_id,
            &ComponentRollbackBaseline::new(vec![ComponentFile::new(path_ref(&live)).with_sha256(
                renderpilot_detection::sha256_bytes(b"original").expect("original hash"),
            )]),
        )
        .expect("baseline");

    let error = remove_game_from_catalog(&context, game.id()).expect_err("must preserve card");

    assert!(matches!(
        error,
        ServiceError::GameRemovalCleanupFailed { .. }
    ));
    assert_eq!(
        std::fs::read(&live).expect("unchanged live file"),
        b"replacement"
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("read")
            .is_some()
    );
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("baseline lookup")
            .is_some(),
        "failed rollback must retain retryable recovery metadata"
    );
}

#[test]
fn ambiguous_overlapping_component_history_is_bundled_without_mutation() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    std::fs::create_dir_all(&install).expect("install");
    let live = install.join("nvngx_dlss.dll");
    let backup = crate::fs::backup_path(&live).expect("backup path");
    std::fs::write(&live, b"replacement").expect("live");
    std::fs::write(&backup, b"original").expect("backup");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");

    let component_ids = [
        ComponentId::new("component:overlap-a").expect("component"),
        ComponentId::new("component:overlap-b").expect("component"),
    ];
    let components = component_ids
        .iter()
        .map(|component_id| {
            LibraryComponent::new(
                component_id.clone(),
                game.id().clone(),
                ComponentKind::NativeLibrary,
                LibraryTechnology::DlssSuperResolution,
                Swappability::Swappable,
            )
            .with_file(
                ComponentFile::new(path_ref(&live))
                    .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash")),
            )
        })
        .collect::<Vec<_>>();
    context
        .storage()
        .replace_components_for_game(game.id(), &components)
        .expect("components");
    for component_id in &component_ids {
        context
            .storage()
            .recover_component_rollback_baseline(
                game.id(),
                component_id,
                &ComponentRollbackBaseline::new(vec![
                    ComponentFile::new(path_ref(&live)).with_sha256(
                        renderpilot_detection::sha256_file(&backup).expect("backup hash"),
                    ),
                ]),
            )
            .expect("baseline");
    }

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("ambiguous history must not be guessed");
    let ServiceError::ManagedCleanupAmbiguous {
        recovery_bundle_path,
        targets,
        ..
    } = error
    else {
        panic!("expected managed cleanup ambiguity");
    };

    assert!(!targets.is_empty());
    assert!(
        std::path::Path::new(&recovery_bundle_path).is_dir(),
        "pre-mutation recovery bundle must be published"
    );
    assert_eq!(
        std::fs::read(&live).expect("unchanged live file"),
        b"replacement"
    );
    assert_eq!(
        std::fs::read(&backup).expect("unchanged backup"),
        b"original"
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("read")
            .is_some()
    );
}

#[test]
fn equivalent_overlapping_component_inverses_execute_once_without_ambiguity() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    std::fs::create_dir_all(&install).expect("install");
    let live = install.join("nvngx_dlss.dll");
    let backup = crate::fs::backup_path(&live).expect("backup path");
    std::fs::write(&live, b"replacement").expect("live");
    std::fs::write(&backup, b"original").expect("backup");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");

    let active_file = ComponentFile::new(path_ref(&live))
        .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash"));
    let original_file = ComponentFile::new(path_ref(&live))
        .with_sha256(renderpilot_detection::sha256_file(&backup).expect("backup hash"));
    let component_ids = [
        ComponentId::new("component:duplicate-a").expect("component"),
        ComponentId::new("component:duplicate-b").expect("component"),
    ];
    let components = component_ids
        .iter()
        .map(|component_id| {
            LibraryComponent::new(
                component_id.clone(),
                game.id().clone(),
                ComponentKind::NativeLibrary,
                LibraryTechnology::DlssSuperResolution,
                Swappability::Swappable,
            )
            .with_file(active_file.clone())
        })
        .collect::<Vec<_>>();
    context
        .storage()
        .replace_components_for_game(game.id(), &components)
        .expect("components");
    let inverse = ComponentRollbackBaseline::new(vec![original_file])
        .with_expected_active_files(vec![active_file]);
    for component_id in &component_ids {
        context
            .storage()
            .recover_component_rollback_baseline(game.id(), component_id, &inverse)
            .expect("baseline");
    }

    remove_game_from_catalog(&context, game.id()).expect("remove");

    assert_eq!(std::fs::read(&live).expect("restored"), b"original");
    assert!(!backup.exists());
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game lookup")
            .is_none()
    );
}

#[test]
fn installed_addon_is_uninstalled_automatically_before_card_removal() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Managed Game");
    std::fs::create_dir_all(&install).expect("install");
    let addon_path = install.join("addon.addon64");
    std::fs::write(&addon_path, b"addon").expect("addon file");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed");
    let addon = InstalledAddon::new(
        game.id().clone(),
        AddonKind::RenoDx,
        PathRef::new(addon_path.to_string_lossy()).expect("addon path"),
    );
    context
        .storage()
        .upsert_installed_addon(&addon)
        .expect("addon");

    remove_game_from_catalog(&context, game.id()).expect("remove");

    assert!(!addon_path.exists(), "durable add-on uninstall must run");
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("addon lookup")
            .is_none()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game lookup")
            .is_none()
    );
}

#[test]
fn dedicated_optiscaler_is_removed_before_its_luma_prerequisite() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Opti Luma Game");
    std::fs::create_dir_all(&install).expect("install");
    let outer = install.join("dxgi.dll");
    let config = install.join("OptiScaler.ini");
    let luma_addon = install.join("Luma.addon64");
    std::fs::write(&luma_addon, b"luma").expect("luma addon");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("game");

    let topology_id = format!("optiscaler:{}", game.id().as_str());
    let outer_receipt = renderpilot_domain::FileReceipt::reused(
        "missing-optiscaler-outer",
        Sha256Hash::new("0".repeat(64)).expect("digest"),
    )
    .expect("outer receipt");
    let topology = GameProxyTopology {
        id: topology_id.clone(),
        game_id: game.id().clone(),
        root_slot: path_ref(&outer),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: path_ref(&outer),
            receipt: outer_receipt,
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    };
    let config_receipt = renderpilot_domain::FileReceipt::reused(
        "reused-config",
        renderpilot_detection::sha256_bytes(b"user config").expect("digest"),
    )
    .expect("config receipt");
    let state = renderpilot_domain::from_new_adoption(
        OptiScalerInstallStateParts {
            game_id: game.id().clone(),
            release_id: "adopted".to_owned(),
            manifest_revision: "test".to_owned(),
            archive_sha256: None,
            source: None,
            target_exe_path: path_ref(&install.join("Game.exe")),
            target_dir: path_ref(&install),
            modules: vec!["core".to_owned()],
            release_files: vec![OptiScalerFileReceipt {
                path: path_ref(&config),
                installed: config_receipt.clone(),
                role: OptiScalerFileRole::Configuration,
                cleanup: renderpilot_domain::OptiScalerFileCleanup::PreserveUnchanged,
                baseline: renderpilot_domain::OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: Vec::new(),
            directory_receipts: Vec::new(),
            proxy_topology_id: Some(topology_id),
            config_schema: 1,
            config_base_release: "adopted".to_owned(),
            adoption_state: OptiScalerAdoptionState::AdoptedExact,
            prerequisite_binding: OptiScalerPrerequisiteBinding::Luma,
            created_at: None,
            updated_at: None,
        },
        renderpilot_domain::OptiScalerConfigurationBaseline::present(
            config_receipt,
            b"user config".to_vec(),
        )
        .expect("configuration baseline"),
    )
    .expect("state");
    let luma = InstalledAddon::new(game.id().clone(), AddonKind::Luma, path_ref(&luma_addon))
        .with_created_file(path_ref(&luma_addon));
    context
        .storage()
        .upsert_installed_addon(&luma)
        .expect("install Luma");
    context
        .storage()
        .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
            game_id: game.id(),
            component_set: None,
            baseline_mutations: &[],
            addon: renderpilot_storage_sqlite::InstalledAddonMutation::OptiScaler(
                renderpilot_storage_sqlite::OptiScalerAggregateMutation::AdoptExactMetadata {
                    state: &state,
                    topology: &topology,
                },
            ),
            mutation_id: None,
        })
        .expect("adopt OptiScaler");

    remove_game_from_catalog(&context, game.id()).expect("remove");

    assert!(!outer.exists(), "OptiScaler outer must be removed first");
    assert!(
        !luma_addon.exists(),
        "Luma must be removed after OptiScaler"
    );
    assert!(
        context
            .storage()
            .get_optiscaler_install_state(game.id())
            .expect("state lookup")
            .is_none()
    );
    assert!(
        context
            .storage()
            .get_proxy_topology(game.id())
            .expect("topology")
            .is_none()
    );
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("addon")
            .is_none()
    );
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_none()
    );
}

#[test]
fn malformed_shared_renodx_is_rejected_before_any_cleanup() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Malformed Shared Game");
    std::fs::create_dir_all(&install).expect("install");
    let addon_path = install.join("renodx.addon64");
    std::fs::write(&addon_path, b"renodx").expect("addon");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed game");
    context
        .storage()
        .upsert_installed_addon(
            &InstalledAddon::new(game.id().clone(), AddonKind::RenoDx, path_ref(&addon_path))
                .with_host_kind(renderpilot_domain::InstalledAddonHostKind::SharedVulkanLayer),
        )
        .expect("seed malformed shared record");

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("shared RenoDX without executable must be rejected");

    assert!(matches!(
        error,
        ServiceError::GameRemovalCleanupFailed { .. }
    ));
    assert!(
        addon_path.exists(),
        "preflight must not mutate the addon file"
    );
}

#[test]
fn luma_shared_vulkan_record_is_rejected_before_any_cleanup() {
    let temp = tempfile::tempdir().expect("temp");
    let install = temp.path().join("Malformed Luma Shared Game");
    std::fs::create_dir_all(&install).expect("install");
    let addon_path = install.join("luma.addon64");
    std::fs::write(&addon_path, b"luma").expect("addon");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game = game(&install, RootAuthority::UserConfirmed);
    context.storage().upsert_game(&game).expect("seed game");
    context
        .storage()
        .upsert_installed_addon(
            &InstalledAddon::new(game.id().clone(), AddonKind::Luma, path_ref(&addon_path))
                .with_host_kind(renderpilot_domain::InstalledAddonHostKind::SharedVulkanLayer),
        )
        .expect("seed malformed shared record");

    let error = remove_game_from_catalog(&context, game.id())
        .expect_err("shared Luma must be rejected before cleanup");

    assert!(matches!(
        error,
        ServiceError::GameRemovalCleanupFailed { .. }
    ));
    assert!(addon_path.exists(), "preflight must not mutate Luma");
    assert!(
        context
            .storage()
            .find_game(game.id())
            .expect("game")
            .is_some()
    );
}

fn game(path: &std::path::Path, authority: RootAuthority) -> GameInstallation {
    GameInstallation::new(
        GameIdentity::new(
            GameId::new("game:remove-test").expect("id"),
            "Game",
            Launcher::Manual,
        )
        .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(path.to_string_lossy()).expect("path"),
    )
    .with_root_authority(authority)
}

fn path_ref(path: &std::path::Path) -> PathRef {
    PathRef::new(path.to_string_lossy()).expect("path")
}

fn seed_game_with_external_claim(
    context: &crate::Context,
    install: &std::path::Path,
    game_id_str: &str,
    target_name: &str,
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

mod drs_fences;
mod e2e_nvapi;
mod engine_cleanup;
mod nvapi_claim_release;
