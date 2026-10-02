use std::{
    cell::RefCell,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, GameIdentity, GameInstallation, GameRuntime, InstalledAddon, InstalledAddonHostKind,
    Launcher, PathRef, Platform, RootAuthority,
};
use renderpilot_platform_windows::vulkan_layer::{
    AppListChange, FileObservation, LayerRegistry, LayerRegistryEntry, RegistryValueState,
    canonical_manifest_bytes, observe_shared_vulkan_layer, plan_register_app, plan_unregister_app,
};
use renderpilot_storage_sqlite::{
    AuthorityCas, BeginSharedVulkanMutation, InstalledAddonMutation, LocalCleanupOwners,
    PendingSharedVulkanMutationRow, PendingSharedVulkanMutationState, SharedArtifactMutation,
    SharedVulkanMutationCommit, SharedVulkanMutationReservation, SharedVulkanMutationScope,
    SqliteStorage,
};
use serde_json::json;

use super::*;
use crate::{
    Context, addons::vulkan_lock::blocking_shared_vulkan_lock,
    catalog::installation_lifecycle::residue::observe_registered_root,
    context::RetiredLeftoversObservation, mutation_boundary::enter_game_observation_boundary,
};

static NEXT_GAME: AtomicU64 = AtomicU64::new(0);

struct FakeRegistry {
    value: RefCell<RegistryValueState>,
}

impl FakeRegistry {
    fn active() -> Self {
        Self {
            value: RefCell::new(RegistryValueState::Present {
                value_type: 4,
                raw_bytes: vec![0; 4],
            }),
        }
    }
}

impl LayerRegistry for FakeRegistry {
    fn registered_layers(&self) -> Vec<LayerRegistryEntry> {
        Vec::new()
    }

    fn observe_canonical_registration(
        &self,
        _manifest_path: &Path,
    ) -> io::Result<RegistryValueState> {
        Ok(self.value.borrow().clone())
    }

    fn restore_canonical_registration(
        &self,
        _manifest_path: &Path,
        state: &RegistryValueState,
    ) -> io::Result<()> {
        *self.value.borrow_mut() = state.clone();
        Ok(())
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    context: Context,
    game: GameInstallation,
    owners: LocalCleanupOwners,
    exe: PathBuf,
    layer_dir: PathBuf,
    registry: FakeRegistry,
    observation: RetiredLeftoversObservation,
}

fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path")
}

fn fixture(other_apps: &[&Path]) -> Fixture {
    let temp = tempfile::tempdir().expect("fixture directory");
    let game_root = temp.path().join("removed-game");
    let id = renderpilot_domain::GameId::new(format!(
        "manual:leftovers-vulkan-{}-{}",
        std::process::id(),
        NEXT_GAME.fetch_add(1, Ordering::Relaxed)
    ))
    .expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(id, "Vulkan cleanup fixture", Launcher::Manual).expect("game identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(&game_root),
    )
    .with_root_authority(RootAuthority::UserConfirmed);
    let storage = SqliteStorage::open(temp.path().join("catalog.sqlite")).expect("storage");
    storage.upsert_game(&game).expect("game registration");
    let addon_path = temp.path().join("recorded-addon.addon64");
    let exe = temp.path().join("removed-game.exe");
    let addon = InstalledAddon::new(game.id().clone(), AddonKind::RenoDx, path_ref(&addon_path))
        .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
        .with_registered_exe_path(path_ref(&exe));
    storage
        .upsert_installed_addon(&addon)
        .expect("shared-layer owner");
    let readiness = storage
        .catalog_readiness(game.id())
        .expect("catalog readiness");
    storage
        .mark_installation_absent(&game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("mark removed installation absent");
    let context = Context::from_storage(storage);

    let layer_dir = temp.path().join("shared-reshade");
    fs::create_dir(&layer_dir).expect("fake shared-layer root");
    let dll = layer_dir.join("ReShade64.dll");
    let manifest = layer_dir.join("ReShade64.json");
    let apps_file = layer_dir.join("ReShadeApps.ini");
    fs::write(&dll, b"global fixture DLL").expect("global DLL");
    fs::write(
        &manifest,
        canonical_manifest_bytes().expect("canonical manifest"),
    )
    .expect("global manifest");
    let target_plan = plan_register_app(None, &exe).expect("register target app");
    let mut bytes = match target_plan.change {
        AppListChange::Replacement(next) => next,
        AppListChange::Unchanged => unreachable!("new target must be inserted"),
    };
    for app in other_apps {
        let plan = plan_register_app(Some(&bytes), app).expect("register other fixture app");
        if let AppListChange::Replacement(next) = plan.change {
            bytes = next;
        }
    }
    fs::write(&apps_file, bytes).expect("app list");
    let registry = FakeRegistry::active();
    let native = observe_shared_vulkan_layer(&registry, &layer_dir).expect("native observation");
    let owners = context
        .storage()
        .read_local_cleanup_owners(game.id())
        .expect("current owners");
    let observation = RetiredLeftoversObservation {
        root: observe_registered_root(&owners),
        owners: owners.clone(),
        pending_files: Vec::new(),
        pending_shared_vulkan: None,
        pending_nvapi: Vec::new(),
        native_shared_vulkan: Some(Ok(native)),
    };
    Fixture {
        _temp: temp,
        context,
        game,
        owners,
        exe,
        layer_dir,
        registry,
        observation,
    }
}

fn item_id(game: &GameInstallation, exe: &Path) -> String {
    let exe = path_ref(exe);
    super::super::super::projection::canonical_item_id(
        game.id(),
        crate::catalog::leftovers::LeftoverCategory::VulkanRegistration,
        Some(exe.as_str()),
        "shared-vulkan-registration",
    )
}

fn clean_fixture(fixture: &Fixture) -> CleanupStep {
    let game_guard = enter_game_observation_boundary(fixture.game.id());
    let shared_guard = blocking_shared_vulkan_lock();
    let authority = VulkanAuthority {
        layer_dir: &fixture.layer_dir,
        registry: &fixture.registry,
    };
    unregister_vulkan_owner_with_authority(
        &fixture.context,
        &game_guard,
        &shared_guard,
        &fixture.owners,
        &fixture.observation,
        &item_id(&fixture.game, &fixture.exe),
        &authority,
    )
    .expect("ordinary app-only unregister")
}

fn commit_ownerless_app_list_cleanup(
    fixture: &Fixture,
) -> (String, PathBuf, PendingSharedVulkanMutationRow) {
    let owners = fixture
        .context
        .storage()
        .read_local_cleanup_owners(fixture.game.id())
        .expect("current owners");
    let exe = owners
        .addon()
        .and_then(InstalledAddon::registered_exe_path)
        .expect("registered executable");
    let operation_id = unregister_operation_id(
        fixture.game.id(),
        owners.availability().revision(),
        exe.as_str(),
    );
    let apps_path = fixture.layer_dir.join("ReShadeApps.ini");
    let before_bytes = fs::read(&apps_path).expect("before app list");
    let app_plan =
        plan_unregister_app(Some(&before_bytes), &fixture.exe).expect("plan app-list-only removal");
    let AppListChange::Replacement(after_bytes) = app_plan.change else {
        panic!("target app must be removed from the app list");
    };
    let roots = TrustedRoots::game_shared_without_game_files(&fixture.layer_dir)
        .expect("shared-only game-shared authority");
    let initial = Manifest::empty(
        Scope::GameShared,
        Some(fixture.game.id().as_str().to_owned()),
        renderpilot_domain::mutation_features::RETIRED_GAME_LEFTOVERS_UNREGISTER,
    )
    .to_json()
    .expect("initial manifest");
    match fixture
        .context
        .storage()
        .try_begin_shared_vulkan_mutation(&BeginSharedVulkanMutation {
            id: operation_id.clone(),
            scope: SharedVulkanMutationScope::GameShared,
            game_id: Some(fixture.game.id().clone()),
            feature: renderpilot_domain::mutation_features::RETIRED_GAME_LEFTOVERS_UNREGISTER
                .to_owned(),
            initial_manifest_json: initial,
            root_capabilities_json: roots.to_json().expect("root capabilities"),
        })
        .expect("reserve ordinary pending transaction")
    {
        SharedVulkanMutationReservation::Reserved(_) => {}
        SharedVulkanMutationReservation::Occupied(_) => panic!("fixture has one pending row"),
    }
    let transaction_root = crate::addons::shared_vulkan_mutation::transaction_root(
        fixture.context.file_mutation_root(),
        &operation_id,
    )
    .expect("ordinary transaction root");
    fs::create_dir_all(transaction_root.join("snapshots")).expect("snapshot directory");
    let snapshot = transaction_root.join("snapshots/file-0.bin");
    fs::write(&snapshot, &before_bytes).expect("before-image snapshot");
    let stage_path = fixture
        .layer_dir
        .join(format!(".renderpilot-svam-{operation_id}-0.stage"));
    let manifest = json!({
        "version": 1,
        "scope": "game_shared",
        "game_id": fixture.game.id().as_str(),
        "feature": renderpilot_domain::mutation_features::RETIRED_GAME_LEFTOVERS_UNREGISTER,
        "files": [{
            "live_path": roots.authorize(&apps_path).expect("app-list capability"),
            "before": {"Snapshot": {
                "snapshot_path": "snapshots/file-0.bin",
                "sha256": digest(&before_bytes),
                "len": before_bytes.len(),
            }},
            "after": {"Present": {
                "sha256": digest(&after_bytes),
                "len": after_bytes.len(),
            }},
            "stage_path": roots.authorize(&stage_path).expect("stage capability"),
            "tomb_path": null,
        }],
        "registry": [],
        "directories": [],
        "peer_program": null,
    })
    .to_string();
    fixture
        .context
        .storage()
        .finish_preparing_shared_vulkan_mutation(
            &operation_id,
            SharedVulkanMutationScope::GameShared,
            Some(fixture.game.id()),
            &manifest,
        )
        .expect("publish ordinary prepared row");
    let prepared_row = fixture
        .context
        .storage()
        .pending_shared_vulkan_mutation()
        .expect("prepared transaction")
        .expect("prepared transaction row");
    fs::write(&apps_path, &after_bytes).expect("publish app-list postimage");
    fixture
        .context
        .storage()
        .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
            id: &operation_id,
            scope: SharedVulkanMutationScope::GameShared,
            game_id: Some(fixture.game.id()),
            addon: InstalledAddonMutation::Delete(AddonKind::RenoDx),
            shared_artifact: SharedArtifactMutation::Keep,
        })
        .expect("commit ordinary metadata deletion");
    (operation_id, snapshot, prepared_row)
}

fn assert_globals_remain(fixture: &Fixture) {
    let dll = fixture.layer_dir.join("ReShade64.dll");
    let manifest = fixture.layer_dir.join("ReShade64.json");
    assert_eq!(
        fs::read(dll).expect("global DLL retained"),
        b"global fixture DLL"
    );
    assert_eq!(
        fs::read(manifest).expect("global manifest retained"),
        canonical_manifest_bytes().expect("canonical manifest")
    );
    assert!(fixture.layer_dir.is_dir());
    assert_eq!(
        fixture.registry.value.borrow().clone(),
        RegistryValueState::Present {
            value_type: 4,
            raw_bytes: vec![0; 4],
        }
    );
    assert!(
        fixture
            .context
            .storage()
            .get_installed_addon(fixture.game.id())
            .expect("per-game metadata")
            .is_none()
    );
}

#[test]
fn app_only_cleanup_preserves_other_consumers_and_every_global_layer_participant() {
    let other = PathBuf::from(r"C:\Games\still-installed\game.exe");
    let fixture = fixture(&[&other]);
    let current_apps = match &fixture
        .observation
        .native_shared_vulkan
        .as_ref()
        .expect("native result")
        .as_ref()
        .expect("native observation")
        .apps
    {
        FileObservation::Present(bytes) => {
            renderpilot_platform_windows::vulkan_layer::parse_app_list(bytes)
                .expect("before app list")
        }
        FileObservation::Absent => panic!("test app list exists"),
    };
    assert_eq!(current_apps.len(), 2);
    clean_fixture(&fixture);

    let apps_path = fixture.layer_dir.join("ReShadeApps.ini");
    let after = renderpilot_platform_windows::vulkan_layer::parse_app_list(
        &fs::read(apps_path).expect("app list retained"),
    )
    .expect("parse remaining app");
    assert_eq!(after.len(), 1);
    assert!(crate::paths::same_path(&after[0], &other));
    assert_globals_remain(&fixture);
}

#[test]
fn unregistering_the_last_app_keeps_global_dll_manifest_registry_and_directory() {
    let fixture = fixture(&[]);
    clean_fixture(&fixture);
    let apps_path = fixture.layer_dir.join("ReShadeApps.ini");
    let after = renderpilot_platform_windows::vulkan_layer::parse_app_list(
        &fs::read(apps_path).expect("empty app list file retained"),
    )
    .expect("parse empty app list");
    assert!(after.is_empty());
    assert_globals_remain(&fixture);
}

#[test]
fn ownerless_committed_app_list_retry_is_projected_with_or_without_snapshot() {
    let other = PathBuf::from(r"C:\Games\still-installed\game.exe");
    for retain_snapshot in [true, false] {
        let fixture = fixture(&[&other]);
        let (operation_id, snapshot, _) = commit_ownerless_app_list_cleanup(&fixture);
        if !retain_snapshot {
            fs::remove_file(&snapshot).expect("ordinary cleanup consumed preimage");
        }
        let owners = fixture
            .context
            .storage()
            .read_local_cleanup_owners(fixture.game.id())
            .expect("ownerless current snapshot");
        assert!(owners.addon().is_none());
        let row = fixture
            .context
            .storage()
            .pending_shared_vulkan_mutation()
            .expect("pending transaction")
            .expect("committed fence remains");
        assert_eq!(row.id, operation_id);
        assert_eq!(row.state, PendingSharedVulkanMutationState::Committed);
        assert_eq!(snapshot.exists(), retain_snapshot);
        let native = observe_shared_vulkan_layer(&fixture.registry, &fixture.layer_dir)
            .expect("current native participants");
        let observation = RetiredLeftoversObservation {
            root: observe_registered_root(&owners),
            owners: owners.clone(),
            pending_files: Vec::new(),
            pending_shared_vulkan: Some(row.clone()),
            pending_nvapi: Vec::new(),
            native_shared_vulkan: Some(Ok(native)),
        };
        assert_eq!(
            super::classify_vulkan_pending(&fixture.context, &observation),
            super::VulkanPendingClass::OwnCommitted
        );
        let items = super::super::super::projection::build_items(
            &fixture.context,
            &fixture.game,
            &observation,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].category,
            crate::catalog::leftovers::LeftoverCategory::VulkanRegistration
        );
        assert_eq!(
            items[0].disposition,
            crate::catalog::leftovers::LeftoverDisposition::Retryable
        );
        assert_eq!(
            fixture
                .context
                .storage()
                .pending_shared_vulkan_mutation()
                .expect("pending row after projection")
                .as_ref(),
            Some(&row)
        );
        let apps = renderpilot_platform_windows::vulkan_layer::parse_app_list(
            &fs::read(fixture.layer_dir.join("ReShadeApps.ini")).expect("current app list"),
        )
        .expect("parse other app");
        assert_eq!(apps.len(), 1);
        assert!(crate::paths::same_path(&apps[0], &other));
        assert_eq!(snapshot.exists(), retain_snapshot);
        assert_globals_remain(&fixture);
    }
}

#[test]
fn prepared_app_list_cleanup_is_owned_but_preparing_is_foreign() {
    let fixture = fixture(&[]);
    let (_, _, prepared_row) = commit_ownerless_app_list_cleanup(&fixture);
    assert_eq!(
        prepared_row.state,
        PendingSharedVulkanMutationState::Prepared
    );

    let mut observation = fixture.observation.clone();
    observation.pending_shared_vulkan = Some(prepared_row.clone());
    assert_eq!(
        super::classify_vulkan_pending(&fixture.context, &observation),
        super::VulkanPendingClass::OwnPrepared
    );

    let mut preparing_row = prepared_row;
    preparing_row.state = PendingSharedVulkanMutationState::Preparing;
    observation.pending_shared_vulkan = Some(preparing_row);
    assert_eq!(
        super::classify_vulkan_pending(&fixture.context, &observation),
        super::VulkanPendingClass::Foreign
    );
}

#[test]
fn leave_revision_ignores_other_games_app_entries_but_tracks_this_game() {
    let first_other = PathBuf::from(r"C:\Games\first\game.exe");
    let fixture = fixture(&[&first_other]);
    let first = fixture.observation.clone();
    let mut second = fixture.observation.clone();
    let first_native = first
        .native_shared_vulkan
        .as_ref()
        .and_then(|value| value.as_ref().ok())
        .expect("first native observation")
        .clone();
    let mut second_native = first_native.clone();
    second_native.apps = FileObservation::Present(app_list(&[
        fixture.exe.as_path(),
        Path::new(r"C:\Games\different\game.exe"),
    ]));
    second.native_shared_vulkan = Some(Ok(second_native));

    let first_items =
        super::super::super::projection::build_items(&fixture.context, &fixture.game, &first);
    let second_items =
        super::super::super::projection::build_items(&fixture.context, &fixture.game, &second);
    let first_revision = super::super::super::revision::semantic_revision(&first, &first_items)
        .expect("first semantic revision");
    let second_revision = super::super::super::revision::semantic_revision(&second, &second_items)
        .expect("second semantic revision");
    assert_eq!(first_revision, second_revision);

    let mut target_absent = second;
    let mut absent_native = first_native;
    absent_native.apps =
        FileObservation::Present(app_list(&[Path::new(r"C:\Games\different\game.exe")]));
    target_absent.native_shared_vulkan = Some(Ok(absent_native));
    let absent_items = super::super::super::projection::build_items(
        &fixture.context,
        &fixture.game,
        &target_absent,
    );
    let absent_revision =
        super::super::super::revision::semantic_revision(&target_absent, &absent_items)
            .expect("target change semantic revision");
    assert_ne!(first_revision, absent_revision);
}

fn app_list(paths: &[&Path]) -> Vec<u8> {
    let Some((first, rest)) = paths.split_first() else {
        return b"[GENERAL]\nApps=\n".to_vec();
    };
    let first = plan_register_app(None, first).expect("first app list");
    let AppListChange::Replacement(mut bytes) = first.change else {
        unreachable!("first app is new");
    };
    for path in rest {
        let plan = plan_register_app(Some(&bytes), path).expect("append other app");
        if let AppListChange::Replacement(next) = plan.change {
            bytes = next;
        }
    }
    bytes
}
