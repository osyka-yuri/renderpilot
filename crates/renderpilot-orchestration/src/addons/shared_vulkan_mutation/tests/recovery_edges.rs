use super::*;

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{AddonKind, GameId};
use renderpilot_storage_sqlite::{
    BeginSharedVulkanMutation, InstalledAddonMutation, PendingSharedVulkanMutationState,
    SharedVulkanMutationCommit, SharedVulkanMutationReservation, SharedVulkanMutationScope,
};
use serde_json::json;

use super::super::TrustedRoots;

fn typed_shared_manifest(
    id: &str,
    game_id: &GameId,
    roots: &TrustedRoots,
    game_root: &std::path::Path,
) -> String {
    let game_root = game_root.to_string_lossy().replace('\\', "/");
    json!({
        "version": 1,
        "scope": "game_shared",
        "game_id": game_id.as_str(),
        "feature": renderpilot_domain::RENODX_INSTALL,
        "files": [],
        "registry": [],
        "directories": [],
        "peer_program": {
            "format": 1,
            "transaction_owner": id,
            "execution_class": "shared",
            "roots": roots.root_ids(),
            "stage": [],
            "custody": [],
            "created_ancestors": [],
            "endpoints": [{
                "ordinal": 0,
                "path": format!("{game_root}/ReShade.ini"),
                "role": "renodx_reshade_ini",
                "operation": "create",
                "planned_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "planned_length": 4,
                "before": null,
                "read_guards": ["game-0:ReShade.ini"],
                "subtree_publishes": []
            }]
        }
    })
    .to_string()
}

fn shared_recovery_fixture(
    id: &str,
) -> (
    tempfile::TempDir,
    crate::Context,
    GameId,
    std::path::PathBuf,
    TrustedRoots,
    std::path::PathBuf,
) {
    let temp = tempfile::tempdir().expect("tempdir");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game_root = temp.path().join("game");
    let shared_root = temp.path().join("shared");
    std::fs::create_dir_all(&game_root).expect("game root");
    std::fs::create_dir_all(&shared_root).expect("shared root");
    let scope = crate::file_mutation::MutationScope::single(&game_root).expect("scope");
    let roots = TrustedRoots::game_shared(&scope, &shared_root).expect("roots");
    let game_id = GameId::new(format!("manual:shared-recovery-{id}")).expect("game id");
    let manifest = typed_shared_manifest(id, &game_id, &roots, &game_root);
    context
        .storage()
        .try_begin_shared_vulkan_mutation(&BeginSharedVulkanMutation {
            id: id.to_owned(),
            scope: SharedVulkanMutationScope::GameShared,
            game_id: Some(game_id.clone()),
            feature: renderpilot_domain::RENODX_INSTALL.to_owned(),
            initial_manifest_json: manifest.clone(),
            root_capabilities_json: roots.to_json().expect("root capabilities"),
        })
        .expect("reserve")
        .assert_reserved();
    let transaction_root =
        super::super::transaction_root(context.file_mutation_root(), id).expect("transaction root");
    std::fs::create_dir_all(&transaction_root).expect("transaction root");
    std::fs::write(transaction_root.join("foreign.marker"), b"keep").expect("marker");
    context
        .storage()
        .finish_preparing_shared_vulkan_mutation(
            id,
            SharedVulkanMutationScope::GameShared,
            Some(&game_id),
            &manifest,
        )
        .expect("finish preparation");
    (temp, context, game_id, transaction_root, roots, game_root)
}

trait ReservedResultExt {
    fn assert_reserved(self);
}

impl ReservedResultExt for SharedVulkanMutationReservation {
    fn assert_reserved(self) {
        assert!(matches!(self, SharedVulkanMutationReservation::Reserved(_)));
    }
}

fn rewrite_shared_row_field(id: &str, context: &crate::Context, column: &str, value: &str) {
    let database = context
        .storage()
        .catalog_file_path()
        .expect("catalog path")
        .expect("catalog database");
    let connection = rusqlite::Connection::open(database).expect("fixture connection");
    let sql = format!("UPDATE pending_shared_vulkan_mutations SET {column} = ?1 WHERE id = ?2");
    connection
        .execute(&sql, rusqlite::params![value, id])
        .expect("rewrite shared durable field");
}

fn commit_shared_fixture(context: &crate::Context, game_id: &GameId, id: &str) {
    context
        .storage()
        .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
            id,
            scope: SharedVulkanMutationScope::GameShared,
            game_id: Some(game_id),
            addon: InstalledAddonMutation::Delete(AddonKind::RenoDx),
            shared_artifact: renderpilot_storage_sqlite::SharedArtifactMutation::Keep,
        })
        .expect("commit shared row");
}

fn fake_registry() -> FakeRegistry {
    FakeRegistry {
        value: RefCell::new(RegistryValueState::Absent),
    }
}

#[test]
fn prepared_typed_shared_feature_drift_is_retained_without_repair() {
    let id = "typed-shared-feature-prepared";
    let (_temp, context, _game_id, transaction_root, roots, _game_root) =
        shared_recovery_fixture(id);
    rewrite_shared_row_field(
        id,
        &context,
        "feature",
        renderpilot_domain::mutation_features::LUMA_INSTALL,
    );

    let error = super::super::recovery::recover_pending_with_roots(
        &context,
        &roots,
        Some(&fake_registry()),
    )
    .expect_err("feature drift must fail closed");

    assert!(error.to_string().contains("requires repair"));
    assert!(transaction_root.join("foreign.marker").exists());
    assert!(
        context
            .storage()
            .pending_shared_vulkan_mutation()
            .expect("row")
            .is_some_and(|row| row.state == PendingSharedVulkanMutationState::Prepared)
    );
}

#[test]
fn committed_typed_shared_feature_drift_is_retained_without_cleanup() {
    let id = "typed-shared-feature-committed";
    let (_temp, context, game_id, transaction_root, roots, _game_root) =
        shared_recovery_fixture(id);
    commit_shared_fixture(&context, &game_id, id);
    rewrite_shared_row_field(
        id,
        &context,
        "feature",
        renderpilot_domain::mutation_features::LUMA_INSTALL,
    );

    let error = super::super::recovery::recover_pending_with_roots(
        &context,
        &roots,
        Some(&fake_registry()),
    )
    .expect_err("feature drift must fail closed");

    assert!(error.to_string().contains("requires repair"));
    assert!(transaction_root.join("foreign.marker").exists());
    assert!(
        context
            .storage()
            .pending_shared_vulkan_mutation()
            .expect("row")
            .is_some_and(|row| row.state == PendingSharedVulkanMutationState::Committed)
    );
}

#[test]
fn prepared_typed_shared_root_capability_drift_is_retained_without_repair() {
    let id = "typed-shared-root-prepared";
    let (_temp, context, _game_id, transaction_root, roots, game_root) =
        shared_recovery_fixture(id);
    let altered_shared = transaction_root
        .parent()
        .expect("mutation root")
        .join("altered");
    std::fs::create_dir_all(&altered_shared).expect("altered shared root");
    let scope = crate::file_mutation::MutationScope::single(&game_root).expect("scope");
    let altered_roots = TrustedRoots::game_shared(&scope, &altered_shared).expect("altered roots");
    rewrite_shared_row_field(
        id,
        &context,
        "root_capabilities_json",
        &altered_roots.to_json().expect("altered capabilities"),
    );

    let error = super::super::recovery::recover_pending_with_roots(
        &context,
        &roots,
        Some(&fake_registry()),
    )
    .expect_err("root capability drift must fail closed");

    assert!(error.to_string().contains("root authority changed"));
    assert!(transaction_root.join("foreign.marker").exists());
    assert!(
        context
            .storage()
            .pending_shared_vulkan_mutation()
            .expect("row")
            .is_some_and(|row| row.state == PendingSharedVulkanMutationState::Prepared)
    );
}

#[test]
fn committed_typed_shared_root_capability_drift_is_retained_without_cleanup() {
    let id = "typed-shared-root-committed";
    let (_temp, context, game_id, transaction_root, roots, game_root) = shared_recovery_fixture(id);
    commit_shared_fixture(&context, &game_id, id);
    let altered_shared = transaction_root
        .parent()
        .expect("mutation root")
        .join("altered");
    std::fs::create_dir_all(&altered_shared).expect("altered shared root");
    let scope = crate::file_mutation::MutationScope::single(&game_root).expect("scope");
    let altered_roots = TrustedRoots::game_shared(&scope, &altered_shared).expect("altered roots");
    rewrite_shared_row_field(
        id,
        &context,
        "root_capabilities_json",
        &altered_roots.to_json().expect("altered capabilities"),
    );

    let error = super::super::recovery::recover_pending_with_roots(
        &context,
        &roots,
        Some(&fake_registry()),
    )
    .expect_err("root capability drift must fail closed");

    assert!(error.to_string().contains("root authority changed"));
    assert!(transaction_root.join("foreign.marker").exists());
    assert!(
        context
            .storage()
            .pending_shared_vulkan_mutation()
            .expect("row")
            .is_some_and(|row| row.state == PendingSharedVulkanMutationState::Committed)
    );
}

#[test]
fn committed_ownerless_retired_leftovers_app_list_recovery_clears_fence_with_or_without_snapshot() {
    for retain_snapshot in [true, false] {
        let id = if retain_snapshot {
            "retired-leftovers-app-list-retained"
        } else {
            "retired-leftovers-app-list-consumed"
        };
        let temp = tempfile::tempdir().expect("tempdir");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game_root = temp.path().join("game");
        let shared_root = temp.path().join("shared-reshade");
        std::fs::create_dir_all(&shared_root).expect("shared root");
        let game_id = GameId::new(format!("manual:{id}")).expect("game id");
        let game_path =
            renderpilot_domain::PathRef::new(game_root.to_string_lossy().replace('\\', "/"))
                .expect("game path");
        let game = renderpilot_domain::GameInstallation::new(
            renderpilot_domain::GameIdentity::new(
                game_id.clone(),
                "Retired Vulkan cleanup fixture",
                renderpilot_domain::Launcher::Manual,
            )
            .expect("game identity"),
            renderpilot_domain::Platform::Windows,
            renderpilot_domain::GameRuntime::NativeWindows,
            game_path,
        )
        .with_root_authority(renderpilot_domain::RootAuthority::UserConfirmed);
        context
            .storage()
            .upsert_game(&game)
            .expect("game registration");
        let target_exe = game_root.join("game.exe");
        let addon_path = game_root.join("ReShade64.addon64");
        let addon = renderpilot_domain::InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            renderpilot_domain::PathRef::new(addon_path.to_string_lossy().replace('\\', "/"))
                .expect("addon path"),
        )
        .with_host_kind(renderpilot_domain::InstalledAddonHostKind::SharedVulkanLayer)
        .with_registered_exe_path(
            renderpilot_domain::PathRef::new(target_exe.to_string_lossy().replace('\\', "/"))
                .expect("registered executable"),
        );
        context
            .storage()
            .upsert_installed_addon(&addon)
            .expect("shared Vulkan owner");
        let readiness = context
            .storage()
            .catalog_readiness(&game_id)
            .expect("catalog readiness");
        context
            .storage()
            .mark_installation_absent(
                &game,
                renderpilot_storage_sqlite::AuthorityCas::new(readiness.authority_epoch()),
            )
            .expect("mark retired installation absent");

        let other_exe = temp.path().join("other-game.exe");
        let apps_path = shared_root.join("ReShadeApps.ini");
        let global_dll = shared_root.join("ReShade64.dll");
        let global_manifest = shared_root.join("ReShade64.json");
        std::fs::write(&global_dll, b"global layer DLL").expect("global DLL");
        std::fs::write(&global_manifest, b"global layer manifest").expect("global manifest");
        let target_only =
            match renderpilot_platform_windows::vulkan_layer::plan_register_app(None, &target_exe)
                .expect("register target app")
                .change
            {
                renderpilot_platform_windows::vulkan_layer::AppListChange::Replacement(bytes) => {
                    bytes
                }
                renderpilot_platform_windows::vulkan_layer::AppListChange::Unchanged => {
                    unreachable!("new target app changes the list")
                }
            };
        let before_bytes = match renderpilot_platform_windows::vulkan_layer::plan_register_app(
            Some(&target_only),
            &other_exe,
        )
        .expect("register other app")
        .change
        {
            renderpilot_platform_windows::vulkan_layer::AppListChange::Replacement(bytes) => bytes,
            renderpilot_platform_windows::vulkan_layer::AppListChange::Unchanged => {
                unreachable!("new other app changes the list")
            }
        };
        let after_bytes = match renderpilot_platform_windows::vulkan_layer::plan_unregister_app(
            Some(&before_bytes),
            &target_exe,
        )
        .expect("remove retired app")
        .change
        {
            renderpilot_platform_windows::vulkan_layer::AppListChange::Replacement(bytes) => bytes,
            renderpilot_platform_windows::vulkan_layer::AppListChange::Unchanged => {
                unreachable!("registered target app is removed")
            }
        };
        let remaining_apps =
            renderpilot_platform_windows::vulkan_layer::parse_app_list(&after_bytes)
                .expect("other app remains");
        assert_eq!(remaining_apps.len(), 1);
        assert!(crate::paths::same_path(&remaining_apps[0], &other_exe));
        std::fs::write(&apps_path, &before_bytes).expect("initial app list");

        let roots = TrustedRoots::game_shared_without_game_files(&shared_root)
            .expect("shared-only GameShared authority");
        let initial = Manifest::empty(
            super::super::manifest::Scope::GameShared,
            Some(game_id.as_str().to_owned()),
            renderpilot_domain::mutation_features::RETIRED_GAME_LEFTOVERS_UNREGISTER,
        )
        .to_json()
        .expect("initial manifest");
        context
            .storage()
            .try_begin_shared_vulkan_mutation(&BeginSharedVulkanMutation {
                id: id.to_owned(),
                scope: SharedVulkanMutationScope::GameShared,
                game_id: Some(game_id.clone()),
                feature: renderpilot_domain::mutation_features::RETIRED_GAME_LEFTOVERS_UNREGISTER
                    .to_owned(),
                initial_manifest_json: initial,
                root_capabilities_json: roots.to_json().expect("root capabilities"),
            })
            .expect("reserve cleanup")
            .assert_reserved();
        let transaction_root = super::super::transaction_root(context.file_mutation_root(), id)
            .expect("transaction root");
        std::fs::create_dir_all(&transaction_root).expect("transaction root directory");
        let plan = MutationPlan::build(Request {
            transaction_root: transaction_root.clone(),
            mutation_id: id.to_owned(),
            roots: roots.clone(),
            scope: super::super::manifest::Scope::GameShared,
            game_id: Some(game_id.as_str().to_owned()),
            feature: renderpilot_domain::mutation_features::RETIRED_GAME_LEFTOVERS_UNREGISTER
                .to_owned(),
            intents: vec![FileIntent {
                live_path: apps_path.clone(),
                before: Some(before_bytes),
                after: Some(after_bytes.clone()),
            }],
            registry: Vec::new(),
            registry_authority: None,
            created_dirs: Vec::new(),
        })
        .expect("plan app-list-only cleanup");
        plan.manifest
            .validate_for_transaction(id)
            .expect("validate manifest");
        super::super::io::materialize_stages(&plan).expect("materialize app-list stage");
        super::super::io::sync_prepared_artifacts(&transaction_root);
        context
            .storage()
            .finish_preparing_shared_vulkan_mutation(
                id,
                SharedVulkanMutationScope::GameShared,
                Some(&game_id),
                &plan.manifest.to_json().expect("prepared manifest"),
            )
            .expect("publish prepared manifest");
        super::super::io::apply_files(&transaction_root, &plan.manifest, &plan.payloads, &roots)
            .expect("publish app-list postimage");
        commit_shared_fixture(&context, &game_id, id);
        let snapshot = match &plan.manifest.files[0].before {
            super::super::manifest::FileBefore::Snapshot { snapshot_path, .. } => {
                transaction_root.join(snapshot_path)
            }
            super::super::manifest::FileBefore::Absent => unreachable!("preimage was captured"),
        };
        if !retain_snapshot {
            std::fs::remove_file(&snapshot).expect("ordinary cleanup consumed preimage");
        }

        let owners = context
            .storage()
            .read_local_cleanup_owners(&game_id)
            .expect("committed owners");
        assert!(owners.addon().is_none());
        assert_eq!(snapshot.exists(), retain_snapshot);
        let row = context
            .storage()
            .pending_shared_vulkan_mutation()
            .expect("pending row")
            .expect("committed fence remains");
        assert_eq!(row.id, id);
        assert_eq!(row.scope, SharedVulkanMutationScope::GameShared);
        assert_eq!(row.game_id.as_ref(), Some(&game_id));
        assert_eq!(
            row.feature,
            renderpilot_domain::mutation_features::RETIRED_GAME_LEFTOVERS_UNREGISTER
        );
        assert_eq!(row.state, PendingSharedVulkanMutationState::Committed);

        let registry = FakeRegistry {
            value: RefCell::new(RegistryValueState::Present {
                value_type: 4,
                raw_bytes: vec![0; 4],
            }),
        };
        let registry_before = registry.value.borrow().clone();
        super::super::recovery::recover_pending_with_roots(&context, &roots, Some(&registry))
            .expect("ordinary committed app-list recovery");

        assert!(
            context
                .storage()
                .pending_shared_vulkan_mutation()
                .expect("cleared committed fence")
                .is_none()
        );
        assert!(!transaction_root.exists());
        assert_eq!(
            std::fs::read(&apps_path).expect("retained app list"),
            after_bytes
        );
        let retained_apps = renderpilot_platform_windows::vulkan_layer::parse_app_list(
            &std::fs::read(&apps_path).expect("retained app list"),
        )
        .expect("parse retained app list");
        assert_eq!(retained_apps.len(), 1);
        assert!(crate::paths::same_path(&retained_apps[0], &other_exe));
        assert_eq!(
            std::fs::read(&global_dll).expect("global DLL retained"),
            b"global layer DLL"
        );
        assert_eq!(
            std::fs::read(&global_manifest).expect("global manifest retained"),
            b"global layer manifest"
        );
        assert_eq!(*registry.value.borrow(), registry_before);
    }
}

#[test]
fn preparing_typed_looking_shared_row_is_abandoned_without_manifest_interpretation() {
    let id = "typed-shared-preparing";
    let temp = tempfile::tempdir().expect("tempdir");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let game_root = temp.path().join("game");
    let shared_root = temp.path().join("shared");
    std::fs::create_dir_all(&game_root).expect("game root");
    std::fs::create_dir_all(&shared_root).expect("shared root");
    let scope = crate::file_mutation::MutationScope::single(&game_root).expect("scope");
    let roots = TrustedRoots::game_shared(&scope, &shared_root).expect("roots");
    let game_id = GameId::new("manual:typed-shared-preparing").expect("game id");
    let initial = json!({
        "feature": renderpilot_domain::RENODX_INSTALL,
        "peer_program": { "endpoints": [{ "role": "renodx_reshade_ini" }] }
    })
    .to_string();
    context
        .storage()
        .try_begin_shared_vulkan_mutation(&BeginSharedVulkanMutation {
            id: id.to_owned(),
            scope: SharedVulkanMutationScope::GameShared,
            game_id: Some(game_id),
            feature: renderpilot_domain::RENODX_INSTALL.to_owned(),
            initial_manifest_json: initial,
            root_capabilities_json: roots.to_json().expect("root capabilities"),
        })
        .expect("reserve")
        .assert_reserved();
    let transaction_root =
        super::super::transaction_root(context.file_mutation_root(), id).expect("transaction root");
    std::fs::create_dir_all(&transaction_root).expect("transaction root");

    super::super::recovery::recover_pending_with_roots(&context, &roots, Some(&fake_registry()))
        .expect("preparing row is abandon-only");

    assert!(!transaction_root.exists());
    assert!(
        context
            .storage()
            .pending_shared_vulkan_mutation()
            .expect("row")
            .is_none()
    );
}

#[test]
fn cleanup_refuses_to_delete_a_drifted_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"before").expect("seed");
    let plan = MutationPlan::build(Request {
        transaction_root: transaction_root.clone(),
        mutation_id: "snapshot-drift".to_owned(),
        roots: super::super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live,
            before: Some(b"before".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    io::materialize_stages(&plan).expect("stage");
    io::apply_files(
        &transaction_root,
        &plan.manifest,
        &plan.payloads,
        &plan.roots,
    )
    .expect("apply");
    let super::super::manifest::FileBefore::Snapshot { snapshot_path, .. } =
        &plan.manifest.files[0].before
    else {
        panic!("snapshot");
    };
    let snapshot = transaction_root.join(snapshot_path);
    std::fs::write(&snapshot, b"foreign").expect("drift snapshot");

    assert!(
        io::cleanup_artifacts(
            &transaction_root,
            &plan.manifest,
            &plan.roots,
            ParticipantState::After,
        )
        .is_err()
    );
    assert_eq!(std::fs::read(snapshot).expect("preserved"), b"foreign");
}

#[test]
fn restored_before_state_can_finish_cleanup_after_snapshot_was_already_removed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"before").expect("seed");
    let plan = MutationPlan::build(Request {
        transaction_root: transaction_root.clone(),
        mutation_id: "cleanup-retry".to_owned(),
        roots: super::super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live,
            before: Some(b"before".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    let super::super::manifest::FileBefore::Snapshot { snapshot_path, .. } =
        &plan.manifest.files[0].before
    else {
        panic!("snapshot")
    };
    std::fs::remove_file(transaction_root.join(snapshot_path)).expect("partial cleanup");

    assert_eq!(
        io::classify_all(&transaction_root, &plan.manifest, &plan.roots, None)
            .expect("classify retry"),
        vec![ParticipantState::Before]
    );
    io::cleanup_artifacts(
        &transaction_root,
        &plan.manifest,
        &plan.roots,
        ParticipantState::Before,
    )
    .expect("finish cleanup");
    assert!(!transaction_root.exists());
}

#[test]
fn stale_file_preimage_is_rejected_before_snapshot_or_target_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"current").expect("seed");

    let result = MutationPlan::build(Request {
        transaction_root: transaction_root.clone(),
        mutation_id: "stale-file".to_owned(),
        roots: super::super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live.clone(),
            before: Some(b"stale".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    });

    assert!(result.is_err());
    assert_eq!(std::fs::read(live).expect("unchanged"), b"current");
    assert!(!transaction_root.join("snapshots").exists());
}

#[test]
fn stale_registry_preimage_is_rejected_without_mutation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let registry = FakeRegistry {
        value: RefCell::new(RegistryValueState::Present {
            value_type: 4,
            raw_bytes: vec![0; 4],
        }),
    };

    let result = MutationPlan::build(Request {
        transaction_root,
        mutation_id: "stale-registry".to_owned(),
        roots: super::super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: Vec::new(),
        registry: vec![super::super::RegistryIntent {
            manifest_path: temp.path().join("ReShade64.json"),
            before: RegistryValue::Absent,
            after: RegistryValue::Present {
                value_type: 4,
                raw_bytes: vec![0; 4],
            },
        }],
        registry_authority: Some(&registry),
        created_dirs: Vec::new(),
    });

    assert!(result.is_err());
    assert_eq!(
        *registry.value.borrow(),
        RegistryValueState::Present {
            value_type: 4,
            raw_bytes: vec![0; 4],
        }
    );
}
