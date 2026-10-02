use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, ComponentId, ComponentRollbackBaseline, D3d12ExecutableBaseline,
    D3d12ExecutableIdentity, FileReceipt, GameIdentity, GameInstallation, GameProxyTopology,
    GameRuntime, InstalledAddon, Launcher, ManagedAddonFile, ManagedFileBaseline,
    OptiScalerAdoptionState, OptiScalerConfigurationBaseline, OptiScalerDirectoryReceipt,
    OptiScalerFileCleanup, OptiScalerFileReceipt, OptiScalerFileRole, OptiScalerInstallStateParts,
    OptiScalerReleaseFileBaseline, PathRef, Platform, ProxyImplementation, ProxyLink,
    ProxyRootPrestate, RootAuthority, Sha256Hash,
};
use renderpilot_storage_sqlite::{AuthorityCas, SqliteStorage};
use sha2::{Digest, Sha256};

use super::{
    CleanupStatus, CleanupStepOutcome, LeaveStatus, LeftoverCategory, LeftoverDisposition,
    LeftoverIssueCode, clean_retired_game_leftovers, leave_retired_game_leftovers,
    list_retired_game_leftovers,
};
use crate::{
    Context,
    addons::engine_config::{EngineIniEntry, EngineIniRecipe, EngineIniRecipeSet},
    catalog::{
        installation_lifecycle::{
            coordinator,
            residue::{self, test_support},
        },
        leftovers::leave_setting_key,
    },
};

static NEXT_GAME: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    _temp: tempfile::TempDir,
    context: Context,
    game: GameInstallation,
    files: Vec<PathBuf>,
}

fn new_game(root: &Path) -> GameInstallation {
    let id = renderpilot_domain::GameId::new(format!(
        "manual:leftovers-{}-{}",
        std::process::id(),
        NEXT_GAME.fetch_add(1, Ordering::Relaxed)
    ))
    .expect("game id");
    GameInstallation::new(
        GameIdentity::new(id, "Leftovers test game", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(root),
    )
    .with_root_authority(RootAuthority::UserConfirmed)
}

fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path")
}

fn sha256(bytes: &[u8]) -> Sha256Hash {
    Sha256Hash::new(hex::encode(Sha256::digest(bytes))).expect("digest")
}

fn owned_file_receipt(path: &Path, bytes: &[u8]) -> FileReceipt {
    let (parent, leaf) = crate::fs::verified_parent(path).expect("verified file parent");
    let observed = parent
        .observe_leaf(&leaf)
        .expect("observe installed file")
        .expect("installed file exists");
    FileReceipt::owned(observed.identity, sha256(bytes)).expect("owned file receipt")
}

fn persist_optiscaler_fixture(
    database: &Path,
    state: &renderpilot_domain::OptiScalerInstallState,
    topology: &GameProxyTopology,
) {
    let connection = rusqlite::Connection::open(database).expect("fixture connection");
    connection
        .execute(
            "INSERT INTO game_proxy_topologies (
                game_id, id, topology_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, 1, 1)",
            rusqlite::params![
                topology.game_id.as_str(),
                &topology.id,
                serde_json::to_string(topology).expect("topology json"),
            ],
        )
        .expect("seed OptiScaler topology");
    connection
        .execute(
            "INSERT INTO optiscaler_install_states (
                game_id, release_id, manifest_revision, archive_sha256, source,
                target_exe_path, target_dir, modules_json, release_files_json,
                runtime_bindings_json, directory_receipts_json, proxy_topology_id,
                config_schema, config_base_release, adoption_state, prerequisite_binding,
                created_at, updated_at, configuration_baseline_json
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11, ?12,
                ?13, ?14, ?15, ?16,
                1, 1, ?17
            )",
            rusqlite::params![
                state.game_id.as_str(),
                &state.release_id,
                &state.manifest_revision,
                state.archive_sha256.as_ref().map(Sha256Hash::as_str),
                state.source.as_deref(),
                state.target_exe_path.as_str(),
                state.target_dir.as_str(),
                serde_json::to_string(&state.modules).expect("modules json"),
                serde_json::to_string(&state.release_files).expect("files json"),
                serde_json::to_string(&state.runtime_bindings).expect("bindings json"),
                serde_json::to_string(&state.directory_receipts).expect("directories json"),
                state.proxy_topology_id.as_deref(),
                state.config_schema,
                &state.config_base_release,
                state.adoption_state.as_str(),
                state.prerequisite_binding.as_str(),
                "{\"format_tag\":\"renderpilot.optiscaler.configuration-baseline\",\"revision\":1,\"kind\":\"absent\"}",
            ],
        )
        .expect("seed OptiScaler state");
}

fn absent_game_with_files(files: &[(&str, &[u8])]) -> Fixture {
    let temp = tempfile::tempdir().expect("fixture directory");
    let database = temp.path().join("catalog.sqlite");
    let root = temp.path().join("game");
    fs::create_dir_all(&root).expect("game root");
    let game = new_game(&root);
    let storage = SqliteStorage::open(database).expect("storage");
    storage.upsert_game(&game).expect("game registration");

    let mut bindings = Vec::new();
    let mut paths = Vec::new();
    for (relative, bytes) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("file parent");
        }
        fs::write(&path, bytes).expect("owned file");
        bindings.push(ManagedAddonFile::owned(
            path_ref(&path),
            ManagedFileBaseline::Absent,
            sha256(bytes),
        ));
        paths.push(path);
    }
    let addon_path = root.join("recorded-addon.addon64");
    let addon = InstalledAddon::new(game.id().clone(), AddonKind::RenoDx, path_ref(&addon_path))
        .try_with_managed_files(bindings)
        .expect("managed ownership");
    storage
        .upsert_installed_addon(&addon)
        .expect("add-on owner");
    mark_absent(&storage, &game);

    Fixture {
        _temp: temp,
        context: Context::from_storage(storage),
        game,
        files: paths,
    }
}

fn mark_absent(storage: &SqliteStorage, game: &GameInstallation) {
    let readiness = storage
        .catalog_readiness(game.id())
        .expect("catalog readiness");
    storage
        .mark_installation_absent(game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("mark installation absent");
}

#[test]
fn list_clean_removes_exact_receipts_and_collects_all_gone_metadata() {
    let fixture = absent_game_with_files(&[("owned.dll", b"RenderPilot-owned bytes")]);
    let before = fs::read(&fixture.files[0]).expect("owned bytes before List");

    let listed = list_retired_game_leftovers(&fixture.context).expect("List");
    assert_eq!(listed.proposals.len(), 1);
    let proposal = &listed.proposals[0];
    assert_eq!(proposal.root_state, super::LeftoverRootState::ResidueOnly);
    assert!(proposal.can_clean);
    assert_eq!(proposal.items.len(), 1);
    assert_eq!(proposal.items[0].category, LeftoverCategory::AddonFile);
    assert_eq!(
        proposal.items[0].disposition,
        LeftoverDisposition::Cleanable
    );
    assert_eq!(
        fs::read(&fixture.files[0]).expect("List stays read-only"),
        before
    );

    let cleaned =
        clean_retired_game_leftovers(&fixture.context, fixture.game.id(), &proposal.intent)
            .expect("Clean");
    assert_eq!(cleaned.status, CleanupStatus::Complete);
    assert!(cleaned.remaining_proposal.is_none());
    assert!(!fixture.files[0].exists());
    assert!(
        fixture
            .context
            .storage()
            .get_installed_addon(fixture.game.id())
            .expect("all-gone collection")
            .is_none()
    );
    assert!(
        fixture
            .context
            .storage()
            .is_installation_absent(fixture.game.id())
            .expect("registration remains absent")
    );
}

#[test]
fn list_keeps_game_scoped_nvapi_conflict_visible_without_mutation() {
    let fixture = absent_game_with_files(&[("owned.dll", b"RenderPilot-owned bytes")]);
    let before = fs::read(&fixture.files[0]).expect("owned bytes before List");
    fixture
        .context
        .storage()
        .begin_nvapi_operation(
            "leftovers-pending-nvapi",
            Some(fixture.game.id().as_str()),
            Some("leftovers-test-profile"),
            "create_profile",
            "{}",
            "{}",
        )
        .expect("persist pending game operation");
    let pending_before = fixture
        .context
        .storage()
        .list_pending_nvapi_operations()
        .expect("pending operations before List");

    let listed = list_retired_game_leftovers(&fixture.context).expect("List");

    assert_eq!(listed.proposals.len(), 1);
    let proposal = &listed.proposals[0];
    assert!(!proposal.can_clean);
    assert_eq!(proposal.items.len(), 1);
    assert_eq!(
        proposal.items[0].disposition,
        LeftoverDisposition::Cleanable
    );
    assert!(listed.issues.iter().any(|entry| {
        entry.game_id.as_deref() == Some(fixture.game.id().as_str())
            && entry.issue.code == LeftoverIssueCode::PendingConflict
            && entry.issue.detail.as_deref() == Some("A game-scoped native operation is pending.")
    }));
    assert_eq!(
        fs::read(&fixture.files[0]).expect("List stays read-only"),
        before
    );
    assert_eq!(
        fixture
            .context
            .storage()
            .list_pending_nvapi_operations()
            .expect("pending operations after List"),
        pending_before
    );
    assert!(
        fixture
            .context
            .storage()
            .get_installed_addon(fixture.game.id())
            .expect("owner after List")
            .is_some()
    );
}

#[test]
fn clean_prunes_multiple_nested_branches_and_empty_root_but_keeps_parent_siblings() {
    let fixture = absent_game_with_files(&[
        ("branch-one/deep/owned-one.bin", b"first nested bytes"),
        ("branch-two/inner/owned-two.bin", b"second nested bytes"),
    ]);
    let root = PathBuf::from(fixture.game.install_path().as_str());
    let parent = root.parent().expect("game root parent");
    let sibling = parent.join("unrelated-sibling.txt");
    fs::write(&sibling, b"keep parent sibling").expect("sibling bytes");
    fs::create_dir_all(root.join("empty-branch/inner/leaf")).expect("empty nested branch");

    let listed = list_retired_game_leftovers(&fixture.context).expect("List nested residue");
    assert_eq!(listed.proposals.len(), 1);
    assert_eq!(listed.proposals[0].items.len(), 2);
    assert_eq!(
        listed.proposals[0].items[0].category,
        LeftoverCategory::AddonFile
    );
    assert!(root.join("branch-one/deep").is_dir());
    assert!(root.join("branch-two/inner").is_dir());
    assert!(root.join("empty-branch/inner/leaf").is_dir());

    let cleaned = clean_retired_game_leftovers(
        &fixture.context,
        fixture.game.id(),
        &listed.proposals[0].intent,
    )
    .expect("Clean nested residue");
    assert_eq!(cleaned.status, CleanupStatus::Complete);
    assert!(cleaned.remaining_proposal.is_none());
    assert!(fixture.files.iter().all(|path| !path.exists()));
    assert!(!root.exists());
    assert!(parent.is_dir());
    assert_eq!(
        fs::read(&sibling).expect("parent sibling remains"),
        b"keep parent sibling"
    );
    assert!(
        fixture
            .context
            .storage()
            .get_installed_addon(fixture.game.id())
            .expect("all-gone metadata collection")
            .is_none()
    );
}

#[test]
fn clean_prunes_unclaimed_empty_descendants_before_removing_optiscaler_directory_receipt() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let database = temp.path().join("catalog.sqlite");
    let root = temp.path().join("game");
    let recorded_directory = root.join("OptiScaler");
    fs::create_dir_all(&recorded_directory).expect("game and recorded OptiScaler directory");
    let game = new_game(&root);
    let storage = SqliteStorage::open(&database).expect("storage");
    storage.upsert_game(&game).expect("game registration");

    let config_bytes = b"[OptiScaler]\n";
    let config_path = root.join("OptiScaler.ini");
    fs::write(&config_path, config_bytes).expect("owned OptiScaler configuration");
    let proxy_bytes = b"OptiScaler proxy";
    let proxy_path = root.join("dxgi.dll");
    fs::write(&proxy_path, proxy_bytes).expect("owned OptiScaler proxy");
    let directory_identity = crate::fs::VerifiedDir::open(&recorded_directory)
        .expect("verified OptiScaler directory")
        .identity()
        .to_owned();

    // These ordinary empty descendants were created after the directory
    // receipt; the later residue proof observes them but has no receipt for
    // any of them.
    fs::create_dir_all(recorded_directory.join("unclaimed-empty/deep"))
        .expect("unclaimed empty descendants");

    let topology_id = format!("optiscaler:{}", game.id().as_str());
    let topology = GameProxyTopology {
        id: topology_id.clone(),
        game_id: game.id().clone(),
        root_slot: path_ref(&proxy_path),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: path_ref(&proxy_path),
            receipt: owned_file_receipt(&proxy_path, proxy_bytes),
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    };
    let state = renderpilot_domain::from_new_install(
        OptiScalerInstallStateParts {
            game_id: game.id().clone(),
            release_id: "test-release".to_owned(),
            manifest_revision: "test-revision".to_owned(),
            archive_sha256: None,
            source: None,
            target_exe_path: path_ref(&root.join("ExampleGame.exe")),
            target_dir: path_ref(&root),
            modules: vec!["core".to_owned()],
            release_files: vec![OptiScalerFileReceipt {
                path: path_ref(&config_path),
                installed: owned_file_receipt(&config_path, config_bytes),
                role: OptiScalerFileRole::Configuration,
                cleanup: OptiScalerFileCleanup::RemoveIfUnchanged,
                baseline: OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: Vec::new(),
            directory_receipts: vec![OptiScalerDirectoryReceipt {
                path: path_ref(&recorded_directory),
                identity: directory_identity,
            }],
            proxy_topology_id: Some(topology_id),
            config_schema: 1,
            config_base_release: "test-release".to_owned(),
            adoption_state: OptiScalerAdoptionState::Managed,
            prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
            created_at: None,
            updated_at: None,
        },
        OptiScalerConfigurationBaseline::absent(),
    )
    .expect("canonical OptiScaler state with a created-directory receipt");
    persist_optiscaler_fixture(&database, &state, &topology);
    mark_absent(&storage, &game);
    let context = Context::from_storage(storage);

    let listed = list_retired_game_leftovers(&context).expect("List mixed OptiScaler residue");
    assert_eq!(listed.proposals.len(), 1);
    let proposal = &listed.proposals[0];
    assert!(proposal.can_clean);
    assert!(proposal.items.iter().any(|item| {
        item.category == LeftoverCategory::OptiscalerFile
            && item
                .path
                .as_deref()
                .is_some_and(|path| crate::paths::same_path(Path::new(path), &config_path))
            && item.disposition == LeftoverDisposition::Cleanable
    }));
    assert!(proposal.items.iter().any(|item| {
        item.category == LeftoverCategory::Directory
            && item
                .path
                .as_deref()
                .is_some_and(|path| crate::paths::same_path(Path::new(path), &recorded_directory))
            && item.disposition == LeftoverDisposition::Cleanable
    }));

    let parent = root.parent().expect("installation parent");
    let sibling = parent.join("unrelated-sibling.txt");
    fs::write(&sibling, b"keep parent sibling").expect("parent sibling");
    let cleaned = clean_retired_game_leftovers(&context, game.id(), &proposal.intent)
        .expect("Clean mixed OptiScaler residue");

    assert_eq!(cleaned.status, CleanupStatus::Complete);
    assert!(cleaned.remaining_proposal.is_none());
    assert!(cleaned.steps.iter().any(|step| {
        step.category == LeftoverCategory::Directory && step.outcome == CleanupStepOutcome::Removed
    }));
    assert!(!config_path.exists());
    assert!(!recorded_directory.exists());
    assert!(!root.exists());
    assert_eq!(
        fs::read(sibling).expect("parent sibling remains"),
        b"keep parent sibling"
    );
}

#[test]
fn pruning_tolerates_directories_already_removed_by_cleanup_steps() {
    let fixture = absent_game_with_files(&[("branch/deep/owned.bin", b"owned bytes")]);
    let root = PathBuf::from(fixture.game.install_path().as_str());
    let owners = fixture
        .context
        .storage()
        .read_local_cleanup_owners(fixture.game.id())
        .expect("owners before cleanup");
    let residue::RetiredRootObservation::ResidueOnly(proof) =
        residue::observe_registered_root(&owners)
    else {
        panic!("complete residue proof");
    };
    fs::remove_file(&fixture.files[0]).expect("remove recorded file");
    fs::remove_dir(root.join("branch/deep")).expect("remove directory during cleanup");
    fs::remove_dir(root.join("branch")).expect("remove parent during cleanup");

    super::local_removal::prune_empty_directories(&proof).expect("already-absent directories skip");
    assert!(!root.exists());
    assert!(root.parent().expect("game parent").is_dir());
}

#[test]
fn list_and_leave_keep_nested_directories_unchanged() {
    let fixture = absent_game_with_files(&[("branch/deep/owned.bin", b"nested bytes")]);
    let root = PathBuf::from(fixture.game.install_path().as_str());
    let listed = list_retired_game_leftovers(&fixture.context).expect("List nested residue");
    assert_eq!(listed.proposals.len(), 1);
    let intent = listed.proposals[0].intent.clone();
    assert!(root.join("branch/deep").is_dir());

    let left = leave_retired_game_leftovers(&fixture.context, fixture.game.id(), &intent)
        .expect("Leave nested residue");
    assert_eq!(left.status, LeaveStatus::Left);
    assert!(root.is_dir());
    assert!(root.join("branch/deep").is_dir());
    assert_eq!(
        fs::read(&fixture.files[0]).expect("Leave keeps file"),
        b"nested bytes"
    );
}

#[test]
fn unknown_file_added_after_list_stales_clean_and_preserves_the_tree() {
    let fixture = absent_game_with_files(&[("branch/deep/owned.bin", b"owned bytes")]);
    let root = PathBuf::from(fixture.game.install_path().as_str());
    let listed = list_retired_game_leftovers(&fixture.context).expect("List nested residue");
    assert_eq!(listed.proposals.len(), 1);
    let intent = listed.proposals[0].intent.clone();
    let unknown = root.join("branch/deep/unrecorded.bin");
    fs::write(&unknown, b"user bytes").expect("unknown file after List");

    let cleaned = clean_retired_game_leftovers(&fixture.context, fixture.game.id(), &intent)
        .expect("stale Clean");
    assert_eq!(cleaned.status, CleanupStatus::Stale);
    assert_eq!(
        fs::read(&fixture.files[0]).expect("owned file retained"),
        b"owned bytes"
    );
    assert_eq!(
        fs::read(&unknown).expect("user file retained"),
        b"user bytes"
    );
    assert!(root.join("branch/deep").is_dir());
    assert!(root.is_dir());
}

#[test]
fn changed_file_stales_the_action_and_preserves_user_bytes() {
    let fixture = absent_game_with_files(&[("owned.dll", b"receipt bytes")]);
    let listed = list_retired_game_leftovers(&fixture.context).expect("List");
    let intent = listed.proposals[0].intent.clone();
    fs::write(&fixture.files[0], b"user replacement").expect("change file after List");

    let cleaned = clean_retired_game_leftovers(&fixture.context, fixture.game.id(), &intent)
        .expect("stale Clean");
    assert_eq!(cleaned.status, CleanupStatus::Stale);
    assert_eq!(
        fs::read(&fixture.files[0]).expect("user file remains"),
        b"user replacement"
    );
    assert!(
        list_retired_game_leftovers(&fixture.context)
            .expect("current List")
            .proposals
            .is_empty()
    );
}

#[test]
fn a_missing_root_without_a_supported_trace_has_no_modal_proposal() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("missing-game-root");
    let game = new_game(&root);
    let storage = SqliteStorage::open(temp.path().join("catalog.sqlite")).expect("storage");
    storage.upsert_game(&game).expect("game registration");
    mark_absent(&storage, &game);
    let context = Context::from_storage(storage);

    let listed = list_retired_game_leftovers(&context).expect("List");
    assert!(listed.proposals.is_empty());
    assert!(listed.issues.is_empty());
    assert!(!root.exists());
}

#[test]
fn leave_rejects_changed_owner_then_suppresses_the_fresh_semantic_revision() {
    let fixture = absent_game_with_files(&[("owned.dll", b"receipt bytes")]);
    let listed = list_retired_game_leftovers(&fixture.context).expect("initial List");
    let old_intent = listed.proposals[0].intent.clone();
    let addon = fixture
        .context
        .storage()
        .get_installed_addon(fixture.game.id())
        .expect("read add-on")
        .expect("owner exists");
    fixture
        .context
        .storage()
        .upsert_installed_addon(&addon.with_addon_version("changed-owner"))
        .expect("change canonical owner");

    let stale = leave_retired_game_leftovers(&fixture.context, fixture.game.id(), &old_intent)
        .expect("stale Leave");
    assert_eq!(stale.status, LeaveStatus::Stale);
    assert_eq!(
        stale.issue.as_ref().map(|issue| issue.code),
        Some(LeftoverIssueCode::StaleIntent)
    );
    let fresh = stale.remaining_proposal.expect("current proposal");
    assert_ne!(fresh.intent, old_intent);
    let left = leave_retired_game_leftovers(&fixture.context, fixture.game.id(), &fresh.intent)
        .expect("Leave current proposal");
    assert_eq!(left.status, LeaveStatus::Left);
    assert!(left.remaining_proposal.is_none());
    assert_eq!(
        fixture
            .context
            .storage()
            .get_setting(&leave_setting_key(fixture.game.id()))
            .expect("Leave preference")
            .as_deref(),
        left.leave_revision.as_deref()
    );
    assert!(
        list_retired_game_leftovers(&fixture.context)
            .expect("List after Leave")
            .proposals
            .is_empty()
    );
}

#[cfg(windows)]
#[test]
fn partial_delete_returns_fresh_remaining_proposal_and_retries_after_real_share_failure() {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};

    let fixture = absent_game_with_files(&[
        ("branch/a-first.dll", b"first"),
        ("z-locked.dll", b"second"),
    ]);
    let first_parent = fixture.files[0].parent().expect("nested first file parent");
    let listed = list_retired_game_leftovers(&fixture.context).expect("List");
    let intent = listed.proposals[0].intent.clone();
    let held = OpenOptions::new()
        .read(true)
        .share_mode(0x0000_0001) // FILE_SHARE_READ: deny delete while allowing verification reads.
        .open(&fixture.files[1])
        .expect("open bounded sharing blocker");

    let partial = clean_retired_game_leftovers(&fixture.context, fixture.game.id(), &intent)
        .expect("partial Clean");
    assert_eq!(partial.status, CleanupStatus::Partial);
    assert!(!fixture.files[0].exists());
    assert!(first_parent.is_dir());
    assert!(fixture.files[1].exists());
    let remaining = partial.remaining_proposal.expect("remaining actual item");
    assert_eq!(remaining.items.len(), 1);
    assert!(
        remaining.items[0]
            .path
            .as_deref()
            .is_some_and(|path| crate::paths::same_path(Path::new(path), &fixture.files[1]))
    );
    drop(held);

    let retried =
        clean_retired_game_leftovers(&fixture.context, fixture.game.id(), &remaining.intent)
            .expect("retry current item");
    assert_eq!(retried.status, CleanupStatus::Complete);
    assert!(retried.remaining_proposal.is_none());
    assert!(!fixture.files[1].exists());
    assert!(!first_parent.exists());
}

#[test]
fn engine_release_preserves_user_keys_comments_and_the_owned_addon_is_collected_afterward() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("game");
    fs::create_dir_all(&root).expect("game root");
    let game = new_game(&root);
    let storage = SqliteStorage::open(temp.path().join("catalog.sqlite")).expect("storage");
    storage.upsert_game(&game).expect("game registration");
    let addon_path = root.join("recorded-addon.addon64");
    storage
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::RenoDx,
            path_ref(&addon_path),
        ))
        .expect("add-on owner");
    let config = temp.path().join("Saved/Config/Windows");
    fs::create_dir_all(&config).expect("config directory");
    let engine_ini = config.join("Engine.ini");
    fs::write(
        &engine_ini,
        b"[SystemSettings]\r\n; keep this user comment\r\nr.UserSetting=2\r\n",
    )
    .expect("initial Engine.ini");
    let recipe = EngineIniRecipe::new(
        "leftovers.test",
        1,
        vec![EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = EngineIniRecipeSet::from_recipes([&recipe]).expect("recipe set");
    crate::addons::engine_config::service::apply(
        &storage,
        game.id(),
        AddonKind::RenoDx,
        &engine_ini,
        &recipes,
        "leftovers-test-apply",
    )
    .expect("create exact Engine.ini receipt");
    mark_absent(&storage, &game);
    let context = Context::from_storage(storage);

    let listed = list_retired_game_leftovers(&context).expect("List");
    let proposal = listed.proposals.first().expect("Engine.ini proposal");
    assert!(
        proposal
            .items
            .iter()
            .any(|item| item.category == LeftoverCategory::EngineConfig)
    );
    let cleaned = clean_retired_game_leftovers(&context, game.id(), &proposal.intent)
        .expect("Clean Engine.ini");
    assert_eq!(cleaned.status, CleanupStatus::Complete);
    let bytes = fs::read(&engine_ini).expect("user Engine.ini remains");
    let text = String::from_utf8(bytes).expect("UTF-8 fixture");
    assert!(text.contains("; keep this user comment"));
    assert!(text.contains("r.UserSetting=2"));
    assert!(!text.contains("r.AllowHDR=1"));
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("after release")
            .is_none()
    );
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("Engine.ini owner released")
            .is_none()
    );
}

#[test]
fn normal_receipt_renodx_receipts_sweep_to_absence_then_public_clean_removes_eight_exact_files() {
    let fixture = test_support::normal_renodx_fixture();
    let before = test_support::file_snapshot(&fixture.files);
    let before_status_record = fixture
        .context
        .storage()
        .get_installed_addon(fixture.game.id())
        .expect("ordinary record")
        .expect("record retained");
    assert_eq!(before_status_record.created_files().len(), 3);
    assert!(before_status_record.managed_files().is_empty());
    let _status_before_sweep = crate::addons::renodx::use_cases::queries::status::status(
        &fixture.context,
        fixture.game.id(),
    )
    .expect("read-only pre-sweep RenoDX status");
    let after_status_record = fixture
        .context
        .storage()
        .get_installed_addon(fixture.game.id())
        .expect("record after status")
        .expect("record retained after status");
    assert_eq!(after_status_record, before_status_record);
    assert_eq!(test_support::file_snapshot(&fixture.files), before);

    assert_eq!(
        fixture
            .context
            .storage()
            .get_installed_addon(fixture.game.id())
            .expect("ordinary record")
            .expect("record retained")
            .created_files()
            .len(),
        3
    );

    let sweep = coordinator::reconcile_registered_roots(&fixture.context).expect("root sweep");
    assert_eq!(sweep.removed_game_ids, vec![fixture.game.id().clone()]);
    assert!(sweep.errors.is_empty());
    assert_eq!(test_support::file_snapshot(&fixture.files), before);
    assert!(
        fixture
            .context
            .storage()
            .find_active_game(fixture.game.id())
            .expect("active lookup")
            .is_none()
    );
    assert!(
        crate::catalog::list_games(&fixture.context)
            .expect("active catalog")
            .is_empty()
    );
    assert!(crate::catalog::get_game_details(&fixture.context, fixture.game.id()).is_err());
    assert_eq!(
        crate::addons::renodx::use_cases::queries::status::status(
            &fixture.context,
            fixture.game.id()
        )
        .expect("RenoDX status"),
        renderpilot_domain::RenoDxInstallState::NotInstalled
    );
    assert_eq!(test_support::file_snapshot(&fixture.files), before);

    let owners = fixture
        .context
        .storage()
        .read_local_cleanup_owners(fixture.game.id())
        .expect("owners remain after read-only observation");
    assert!(owners.addon().is_some());
    assert_eq!(owners.baselines().len(), 1);

    let listed = list_retired_game_leftovers(&fixture.context).expect("public List");
    assert_eq!(listed.proposals.len(), 1);
    let proposal = &listed.proposals[0];
    assert_eq!(proposal.root_state, super::LeftoverRootState::ResidueOnly);
    assert!(proposal.can_clean);
    assert_eq!(proposal.items.len(), 8);
    assert_eq!(test_support::file_snapshot(&fixture.files), before);

    let cleaned =
        clean_retired_game_leftovers(&fixture.context, fixture.game.id(), &proposal.intent)
            .expect("public Clean");
    assert_eq!(cleaned.status, CleanupStatus::Complete);
    assert!(cleaned.remaining_proposal.is_none());
    assert!(fixture.files.iter().all(|path| !path.exists()));
    let owners = fixture
        .context
        .storage()
        .read_local_cleanup_owners(fixture.game.id())
        .expect("owners after all-gone collection");
    assert!(owners.addon().is_none());
    assert!(owners.baselines().is_empty());
}

#[test]
fn normal_receipt_original_game_payload_blocks_cleanup_and_a_returned_game_stales_the_old_intent() {
    let protected = test_support::normal_renodx_fixture();
    let game_payload = protected.root.join("ExampleGame.exe");
    fs::write(&game_payload, b"original game payload").expect("game payload");
    test_support::mark_absent(&protected.context, &protected.game);
    let owners = protected
        .context
        .storage()
        .read_local_cleanup_owners(protected.game.id())
        .expect("protected owners");
    assert!(matches!(
        residue::observe_registered_root(&owners),
        residue::RetiredRootObservation::PresentNonResidue { .. }
    ));
    let listed = list_retired_game_leftovers(&protected.context).expect("List with game payload");
    assert!(listed.proposals.is_empty());
    assert_eq!(
        fs::read(&game_payload).expect("game payload retained"),
        b"original game payload"
    );
    assert!(protected.files.iter().all(|path| path.exists()));

    let returned = test_support::normal_renodx_fixture();
    test_support::mark_absent(&returned.context, &returned.game);
    let listed = list_retired_game_leftovers(&returned.context).expect("initial List");
    assert_eq!(listed.proposals.len(), 1);
    let intent = listed.proposals[0].intent.clone();
    let returned_payload = returned.root.join("ExampleGame.exe");
    fs::write(&returned_payload, b"returned game payload").expect("return game payload");

    let cleaned = clean_retired_game_leftovers(&returned.context, returned.game.id(), &intent)
        .expect("stale Clean after game return");
    assert_eq!(cleaned.status, CleanupStatus::Stale);
    assert!(returned.files.iter().all(|path| path.exists()));
    assert_eq!(
        fs::read(&returned_payload).expect("returned game payload retained"),
        b"returned game payload"
    );
}

#[test]
fn normal_receipt_changed_claimed_bytes_and_an_arbitrary_backup_name_never_make_a_cleanup_proposal()
{
    let fixture = test_support::normal_renodx_fixture();
    test_support::mark_absent(&fixture.context, &fixture.game);
    let before = test_support::file_snapshot(&fixture.files);
    let addon_path = fixture
        .files
        .iter()
        .find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("addon64"))
        })
        .expect("payload path");
    let sidecar_path = fixture
        .files
        .iter()
        .find(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("bak"))
        })
        .expect("component sidecar path");
    let host_path = fixture
        .files
        .iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("dxgi.dll"))
        })
        .expect("proxy path");
    let config_path = fixture
        .files
        .iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("ReShade.ini"))
        })
        .expect("standard config path");

    for (index, path) in [addon_path, host_path, config_path, sidecar_path]
        .into_iter()
        .enumerate()
    {
        fs::write(path, format!("changed claimed bytes {index}")).expect("mutate a claimed file");
        assert!(
            list_retired_game_leftovers(&fixture.context)
                .expect("List changed claimed file")
                .proposals
                .is_empty()
        );
        let file_index = fixture
            .files
            .iter()
            .position(|candidate| candidate == path)
            .expect("changed file index");
        fs::write(path, &before[file_index]).expect("restore claimed bytes");
    }

    let arbitrary_backup = fixture.root.join("unrecorded.dll.bak");
    fs::write(&arbitrary_backup, b"unknown backup bytes").expect("write arbitrary backup");
    assert!(
        list_retired_game_leftovers(&fixture.context)
            .expect("List arbitrary backup")
            .proposals
            .is_empty()
    );
    assert_eq!(
        fs::read(&arbitrary_backup).expect("backup retained"),
        b"unknown backup bytes"
    );
    assert_eq!(test_support::file_snapshot(&fixture.files), before);
}

#[test]
fn completed_d3d12_executable_baseline_offers_only_its_exact_original_backup_for_clean() {
    let original = b"D3D12 original executable";
    let fixture = d3d12_backup_fixture(original, b"patched executable", original, true);

    let listed = list_retired_game_leftovers(&fixture.context).expect("List D3D12 residue");
    assert_eq!(listed.proposals.len(), 1);
    let proposal = &listed.proposals[0];
    assert!(proposal.can_clean);
    assert_eq!(proposal.items.len(), 1);
    assert_eq!(proposal.items[0].category, LeftoverCategory::ComponentFile);
    assert_eq!(
        proposal.items[0].path.as_deref().map(Path::new),
        Some(fixture.files[0].as_path())
    );

    let cleaned =
        clean_retired_game_leftovers(&fixture.context, fixture.game.id(), &proposal.intent)
            .expect("Clean exact D3D12 backup");
    assert_eq!(cleaned.status, CleanupStatus::Complete);
    assert!(!fixture.files[0].exists());
}

#[test]
fn modified_unowned_and_equal_identity_d3d12_backups_remain_protected() {
    for (name, original, active, observed, recorded) in [
        (
            "modified",
            b"original executable".as_slice(),
            b"patched executable".as_slice(),
            b"user replacement".as_slice(),
            true,
        ),
        (
            "unowned",
            b"original executable".as_slice(),
            b"patched executable".as_slice(),
            b"original executable".as_slice(),
            false,
        ),
        (
            "equal",
            b"same executable".as_slice(),
            b"same executable".as_slice(),
            b"same executable".as_slice(),
            true,
        ),
    ] {
        let fixture = d3d12_backup_fixture(original, active, observed, recorded);
        let listed = list_retired_game_leftovers(&fixture.context).expect("List");
        assert!(
            listed.proposals.is_empty(),
            "{name} backup is not claimable"
        );
        assert_eq!(
            fs::read(&fixture.files[0]).expect("backup remains"),
            observed,
            "{name}"
        );
    }
}

fn d3d12_backup_fixture(
    original_bytes: &[u8],
    active_bytes: &[u8],
    backup_bytes: &[u8],
    recorded: bool,
) -> Fixture {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("game");
    fs::create_dir_all(&root).expect("game root");
    let game = new_game(&root);
    let storage = SqliteStorage::open(temp.path().join("catalog.sqlite")).expect("storage");
    storage.upsert_game(&game).expect("game registration");
    let executable = root.join("ExampleGame.exe");
    let backup = crate::fs::backup_path(&executable).expect("D3D12 backup path");
    fs::write(&backup, backup_bytes).expect("backup bytes");

    if recorded {
        let baseline = ComponentRollbackBaseline::new(Vec::new()).with_d3d12_executable(
            D3d12ExecutableBaseline::new(
                path_ref(&executable),
                D3d12ExecutableIdentity::new(606, sha256(original_bytes)),
                D3d12ExecutableIdentity::new(611, sha256(active_bytes)),
            ),
        );
        let component_id = ComponentId::new(format!(
            "component:{}:d3d12-executable:{}",
            game.id().as_str(),
            root.to_string_lossy().replace('\\', "/")
        ))
        .expect("component id");
        storage
            .recover_component_rollback_baseline(game.id(), &component_id, &baseline)
            .expect("persist completed D3D12 baseline");
    }
    mark_absent(&storage, &game);

    Fixture {
        _temp: temp,
        context: Context::from_storage(storage),
        game,
        files: vec![backup],
    }
}

#[test]
fn root_local_engine_ini_does_not_keep_the_game_active_or_lose_its_owner() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().join("game");
    fs::create_dir_all(&root).expect("game root");
    let game = new_game(&root)
        .with_executable_candidate(PathRef::new("ExampleGame.exe").expect("missing candidate"));
    let storage = SqliteStorage::open(temp.path().join("catalog.sqlite")).expect("storage");
    storage.upsert_game(&game).expect("game registration");
    storage
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::RenoDx,
            path_ref(&root.join("renodx.addon64")),
        ))
        .expect("independent add-on owner");

    let engine_ini = root.join("Engine.ini");
    fs::write(&engine_ini, b"[SystemSettings]\r\nr.UserSetting=2\r\n").expect("Engine.ini");
    let recipe = EngineIniRecipe::new(
        "leftovers.presence",
        1,
        vec![EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = EngineIniRecipeSet::from_recipes([&recipe]).expect("recipe set");
    crate::addons::engine_config::service::apply(
        &storage,
        game.id(),
        AddonKind::RenoDx,
        &engine_ini,
        &recipes,
        "leftovers-presence-test",
    )
    .expect("apply Engine.ini contribution");
    let before = fs::read(&engine_ini).expect("Engine.ini before observation");
    let context = Context::from_storage(storage);

    let sweep = coordinator::reconcile_registered_roots(&context).expect("reconcile");

    assert_eq!(sweep.removed_game_ids, vec![game.id().clone()]);
    assert!(sweep.errors.is_empty());
    assert!(
        context
            .storage()
            .is_installation_absent(game.id())
            .expect("absence")
    );
    assert!(
        context
            .storage()
            .get_installed_addon(game.id())
            .expect("retained add-on owner")
            .is_some()
    );
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("Engine.ini owner read")
            .is_some()
    );
    assert_eq!(fs::read(&engine_ini).expect("Engine.ini retained"), before);
}
