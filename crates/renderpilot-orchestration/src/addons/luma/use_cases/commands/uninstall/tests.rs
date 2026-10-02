use std::path::Path;

use renderpilot_application::{
    ComponentRepository, GameRepository, InstalledAddonRepository, OptiScalerStateRepository,
    ProxyTopologyRepository,
};
use renderpilot_domain::{
    AddonKind, ComponentFile, ComponentId, ComponentKind, EngineConfigJournal, EngineConfigReceipt,
    FileOwnership, FileReceipt, GameId, GameIdentity, GameInstallation, GameProxyTopology,
    GameRuntime, InstalledAddon, InstalledAddonHostKind, Launcher, LibraryComponent,
    LibraryTechnology, ManagedAddonFile, ManagedFileBaseline, OptiScalerAdoptionState,
    OptiScalerFileCleanup, OptiScalerFileReceipt, OptiScalerFileRole,
    OptiScalerPrerequisiteBinding, PathRef, Platform, ProxyImplementation, ProxyLink,
    ProxyRootPrestate, Swappability,
};
use tempfile::tempdir;

use super::uninstall;
use crate::Context;
use crate::ServiceError;
use crate::addons::records;

use renderpilot_storage_sqlite::{
    GameMutationCommit, InstalledAddonMutation, OptiScalerAggregateMutation,
};
fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().into_owned()).expect("path")
}

fn seed_game(context: &Context, game_id: &GameId, root: &Path) {
    let identity = GameIdentity::new(game_id.clone(), "Test", Launcher::Manual).expect("identity");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(root),
    );
    context.storage().upsert_game(&game).expect("game");
}

fn exact_receipt(path: &Path, bytes: &[u8], ownership: FileOwnership) -> FileReceipt {
    let (parent, leaf) = crate::fs::verified_parent(path).expect("verified parent");
    let observation = parent
        .observe_leaf(&leaf)
        .expect("observe receipt")
        .expect("receipt file");
    match ownership {
        FileOwnership::Owned => FileReceipt::owned(
            observation.identity,
            renderpilot_detection::sha256_bytes(bytes).expect("hash"),
        )
        .expect("owned receipt"),
        FileOwnership::Reused => FileReceipt::reused(
            observation.identity,
            renderpilot_detection::sha256_bytes(bytes).expect("hash"),
        )
        .expect("reused receipt"),
    }
}

fn stable_engine_journal(path: &Path) -> EngineConfigJournal {
    EngineConfigJournal {
        stable: Some(EngineConfigReceipt {
            schema_version: 1,
            path: path.to_string_lossy().into_owned(),
            file_created: false,
            encoding: "utf8".to_owned(),
            before_digest: "0".repeat(64),
            after_digest: "1".repeat(64),
            recipe_fingerprint: "2".repeat(64),
            contributions: Vec::new(),
            created_headers: Vec::new(),
            created_header_prefixes: Vec::new(),
            created_header_groups: Vec::new(),
            created_header_ordinals: Vec::new(),
        }),
        pending: None,
    }
}

fn seed_active_topology(
    context: &Context,
    game_id: &GameId,
    root: &Path,
    host: Option<(&Path, FileOwnership, &[u8])>,
    prerequisite_binding: OptiScalerPrerequisiteBinding,
) -> GameProxyTopology {
    let proxy = root.join("dxgi.dll");
    let config = root.join("OptiScaler.ini");
    let executable = root.join("Game.exe");
    std::fs::write(&proxy, b"outer").expect("proxy");
    std::fs::write(&config, b"[OptiScaler]\n").expect("config");
    std::fs::write(&executable, b"exe").expect("executable");
    let config_receipt = exact_receipt(&config, b"[OptiScaler]\n", FileOwnership::Reused);
    if let Some((path, _, bytes)) = host {
        std::fs::write(path, bytes).expect("host");
    }
    let topology_id = format!("optiscaler:luma-uninstall:{}", game_id.as_str());
    let topology = GameProxyTopology {
        id: topology_id.clone(),
        game_id: game_id.clone(),
        root_slot: path_ref(&proxy),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: path_ref(&proxy),
            receipt: exact_receipt(&proxy, b"outer", FileOwnership::Reused),
        },
        downstream: host.map(|(path, ownership, bytes)| ProxyLink {
            implementation: ProxyImplementation::ReShade,
            path: path_ref(path),
            receipt: exact_receipt(path, bytes, ownership),
        }),
        downstream_origin: host.map(|_| path_ref(&proxy)),
        root_prestate: ProxyRootPrestate::Absent,
    };
    let state = renderpilot_domain::from_persisted(
        renderpilot_domain::OptiScalerInstallStateParts {
            game_id: game_id.clone(),
            release_id: "luma-uninstall".to_owned(),
            manifest_revision: "luma-uninstall".to_owned(),
            archive_sha256: None,
            source: None,
            target_exe_path: path_ref(&executable),
            target_dir: path_ref(root),
            modules: vec!["core".to_owned()],
            release_files: vec![OptiScalerFileReceipt {
                path: path_ref(&config),
                installed: config_receipt.clone(),
                role: OptiScalerFileRole::Configuration,
                cleanup: OptiScalerFileCleanup::PreserveUnchanged,
                baseline: renderpilot_domain::OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: Vec::new(),
            directory_receipts: Vec::new(),
            proxy_topology_id: Some(topology_id),
            config_schema: 1,
            config_base_release: "luma-uninstall".to_owned(),
            adoption_state: OptiScalerAdoptionState::AdoptedExact,
            prerequisite_binding,
            created_at: None,
            updated_at: None,
        },
        renderpilot_domain::OptiScalerConfigurationBaseline::present(
            config_receipt,
            b"[OptiScaler]\n".to_vec(),
        )
        .expect("configuration baseline"),
    )
    .expect("state");
    context
        .storage()
        .commit_game_mutation(GameMutationCommit {
            game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: InstalledAddonMutation::OptiScaler(
                OptiScalerAggregateMutation::AdoptExactMetadata {
                    state: &state,
                    topology: &topology,
                },
            ),
            mutation_id: None,
        })
        .expect("active topology");
    topology
}

fn seed_dlss_component(
    context: &Context,
    game_id: &GameId,
    live: &Path,
    baseline: &[ComponentFile],
) -> ComponentId {
    let component_id =
        ComponentId::new(format!("component:{}:dlss", game_id.as_str())).expect("component id");
    let current = ComponentFile::new(path_ref(live))
        .with_sha256(renderpilot_detection::sha256_file(live).expect("live hash"));
    let component = LibraryComponent::new(
        component_id.clone(),
        game_id.clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(current);
    context
        .storage()
        .replace_components_for_game(game_id, &[component])
        .expect("component");
    context
        .storage()
        .recover_component_backup(game_id, &component_id, baseline)
        .expect("backup");
    component_id
}

#[test]
fn uninstall_reports_not_installed_for_a_renodx_record_and_leaves_it_untouched() {
    let db_dir = tempdir().expect("db dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091500").expect("game id");
    let renodx_record = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(r"C:\Games\Test\renodx-test.addon64").expect("path"),
    );
    context
        .storage()
        .upsert_installed_addon(&renodx_record)
        .expect("seed renodx record");

    let error = uninstall(&context, &game_id).expect_err("luma uninstall must be refused");
    assert!(matches!(error, ServiceError::InvalidInput(_)));

    let still_present = records::foreign_record(&context, &game_id, AddonKind::Luma)
        .expect("get")
        .expect("the renodx record must survive untouched");
    assert_eq!(still_present.kind(), AddonKind::RenoDx);
}

#[test]
fn uninstall_reports_not_installed_when_nothing_is_installed() {
    let db_dir = tempdir().expect("db dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091501").expect("game id");

    let error = uninstall(&context, &game_id).expect_err("nothing installed");
    assert!(matches!(error, ServiceError::InvalidInput(_)));
}

#[test]
fn persisted_optiscaler_luma_binding_blocks_direct_uninstall_until_public_optiscaler_uninstall() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-required-by-optiscaler").expect("game id");
    seed_game(&context, &game_id, game.path());

    let addon_root = game.path().join("ReShadeAddons");
    std::fs::create_dir_all(&addon_root).expect("nested Luma root");
    let addon = addon_root.join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("addon");
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon));
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("Luma record");
    let persisted_record = records::record_of_kind(&context, &game_id, AddonKind::Luma)
        .expect("read Luma record")
        .expect("persisted Luma record");
    let topology = seed_active_topology(
        &context,
        &game_id,
        game.path(),
        None,
        OptiScalerPrerequisiteBinding::Luma,
    );

    let error = uninstall(&context, &game_id).expect_err("dependency blocks direct uninstall");
    assert!(matches!(error, ServiceError::LumaRequiredByOptiScaler));
    assert!(addon.exists(), "the guard runs before any filesystem plan");
    assert_eq!(
        records::record_of_kind(&context, &game_id, AddonKind::Luma).expect("record"),
        Some(persisted_record),
    );
    assert_eq!(
        context
            .storage()
            .get_proxy_topology(&game_id)
            .expect("topology"),
        Some(topology),
    );

    // The public OptiScaler path is the authority that removes both its
    // persisted state and topology.  Once it has committed, the former Luma
    // prerequisite no longer exists, so the ordinary Luma uninstall becomes
    // eligible again without relying on a UI preview or an in-memory flag.
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime")
        .block_on(crate::addons::optiscaler::uninstall(&context, &game_id))
        .expect("public OptiScaler uninstall");
    assert_eq!(
        context
            .storage()
            .get_optiscaler_install_state(&game_id)
            .expect("OptiScaler state"),
        None,
    );
    assert_eq!(
        context
            .storage()
            .get_proxy_topology(&game_id)
            .expect("topology"),
        None,
    );
    assert_eq!(
        crate::addons::luma::dependency::uninstall_blocker(&context, &game_id)
            .expect("uninstall blocker"),
        None,
        "availability derives its disabled state from the same durable binding"
    );

    uninstall(&context, &game_id).expect("Luma uninstall after OptiScaler release");
    assert!(
        !addon.exists(),
        "Luma payload is removed after the binding clears"
    );
    assert!(
        records::record_of_kind(&context, &game_id, AddonKind::Luma)
            .expect("Luma record")
            .is_none(),
        "the Luma record is removed after a successful uninstall"
    );
}

#[test]
fn uninstall_removes_files_then_deletes_the_db_row() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091502").expect("game id");
    let addon = game_dir.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("write addon");
    let payload = game_dir.path().join("Luma").join("Global");
    std::fs::create_dir_all(&payload).expect("mkdir");
    let shader = payload.join("A.hlsl");
    std::fs::write(&shader, b"technique {}").expect("write shader");

    let record = InstalledAddon::from_parts(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
        None,
        vec![
            PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            PathRef::new(shader.to_string_lossy().into_owned()).expect("path"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("seed");

    uninstall(&context, &game_id).expect("uninstall");

    assert!(!addon.exists());
    assert!(!shader.exists());
    assert!(
        records::record_of_kind(&context, &game_id, AddonKind::Luma)
            .expect("get")
            .is_none(),
        "DB row must be deleted only after successful file uninstall"
    );
}

#[test]
fn uninstall_keeps_the_db_row_when_file_removal_fails() {
    // A tracked created path that is a non-empty directory cannot be removed
    // via remove_file -- file uninstall fails. The row must stay so UI/DB agree.
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091503").expect("game id");
    let addon = game_dir.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("write addon");
    let stuck = game_dir.path().join("Luma-stuck.addon");
    std::fs::create_dir_all(stuck.join("nested")).expect("mkdir stuck");
    std::fs::write(stuck.join("nested").join("x"), b"x").expect("nested file");

    let record = InstalledAddon::from_parts(
        game_id.clone(),
        AddonKind::Luma,
        PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
        None,
        vec![
            PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            PathRef::new(stuck.to_string_lossy().into_owned()).expect("path"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("seed");

    uninstall(&context, &game_id).expect_err("file removal must fail");

    let still = records::record_of_kind(&context, &game_id, AddonKind::Luma)
        .expect("get")
        .expect("row must survive file-uninstall failure");
    assert_eq!(still.kind(), AddonKind::Luma);
}

#[test]
fn engine_config_release_failure_blocks_luma_payload_and_row_mutation() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-engine-release-failure").expect("game id");
    let addon = game_dir.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("write addon");
    // A directory at the journal target makes the audited Engine.ini release
    // fail before any payload/cascade deletion is allowed to begin.
    let blocked_target = game_dir.path().join("Engine.ini");
    std::fs::create_dir(&blocked_target).expect("blocked target");
    let receipt = EngineConfigReceipt {
        schema_version: 1,
        path: blocked_target.to_string_lossy().into_owned(),
        file_created: false,
        encoding: "utf8".to_owned(),
        before_digest: "0".repeat(64),
        after_digest: "1".repeat(64),
        recipe_fingerprint: "2".repeat(64),
        contributions: Vec::new(),
        created_headers: Vec::new(),
        created_header_prefixes: Vec::new(),
        created_header_groups: Vec::new(),
        created_header_ordinals: Vec::new(),
    };
    let journal = EngineConfigJournal {
        stable: Some(receipt),
        pending: None,
    };
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .with_engine_config_journal(Some(journal))
        .expect("record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("seed");
    context
        .storage()
        .compare_and_swap_engine_config_journal(
            &game_id,
            AddonKind::Luma,
            None,
            record.engine_config_journal(),
        )
        .expect("journal");

    uninstall(&context, &game_id).expect_err("Engine.ini release must block uninstall");

    assert!(addon.exists(), "payload remains after release failure");
    assert_eq!(
        records::record_of_kind(&context, &game_id, AddonKind::Luma)
            .expect("record")
            .expect("row remains")
            .engine_config_journal(),
        record.engine_config_journal()
    );
}

#[test]
fn uninstall_cascades_swap_and_restores_exact_owned_baseline() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-cascade-present").expect("id");
    seed_game(&context, &game_id, game.path());
    let addon = game.path().join("Luma-Game.addon");
    let live = game.path().join("nvngx_dlss.dll");
    let sidecar = game.path().join("nvngx_dlss.dll.bak");
    std::fs::write(&addon, b"addon").expect("addon");
    std::fs::write(&live, b"catalog-overlay").expect("live");
    std::fs::write(&sidecar, b"exact-original").expect("sidecar");
    let original_hash = renderpilot_detection::sha256_file(&sidecar).expect("hash");
    let baseline = vec![ComponentFile::new(path_ref(&live)).with_sha256(original_hash.clone())];
    let component_id = seed_dlss_component(&context, &game_id, &live, &baseline);
    let binding = ManagedAddonFile::owned(
        path_ref(&live),
        ManagedFileBaseline::Present {
            sha256: original_hash,
        },
        renderpilot_detection::sha256_file(&live).expect("live hash"),
    );
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .try_with_managed_files(vec![binding])
        .expect("valid binding");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");

    uninstall(&context, &game_id).expect("uninstall");

    assert_eq!(std::fs::read(&live).unwrap(), b"exact-original");
    assert!(!sidecar.exists());
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn uninstall_of_owned_absent_path_removes_swap_and_sidecar_claim() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-cascade-absent").expect("id");
    seed_game(&context, &game_id, game.path());
    let addon = game.path().join("Luma-Game.addon");
    let live = game.path().join("nvngx_dlss.dll");
    std::fs::write(&addon, b"addon").expect("addon");
    std::fs::write(&live, b"catalog-overlay").expect("live");
    let component_id = seed_dlss_component(&context, &game_id, &live, &[]);
    let binding = ManagedAddonFile::owned(
        path_ref(&live),
        ManagedFileBaseline::Absent,
        renderpilot_detection::sha256_file(&live).expect("hash"),
    );
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .try_with_managed_files(vec![binding])
        .expect("valid binding");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");

    uninstall(&context, &game_id).expect("uninstall");

    assert!(!live.exists());
    assert!(!game.path().join("nvngx_dlss.dll.bak").exists());
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn uninstall_reused_dlss_leaves_independent_library_swap_intact() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-reused-swap").expect("id");
    seed_game(&context, &game_id, game.path());
    let addon = game.path().join("Luma-Game.addon");
    let live = game.path().join("nvngx_dlss.dll");
    let sidecar = game.path().join("nvngx_dlss.dll.bak");
    std::fs::write(&addon, b"addon").expect("addon");
    std::fs::write(&live, b"independent-library-swap").expect("live");
    std::fs::write(&sidecar, b"game-original").expect("sidecar");
    let original_hash = renderpilot_detection::sha256_file(&sidecar).expect("hash");
    let baseline = vec![ComponentFile::new(path_ref(&live)).with_sha256(original_hash)];
    let component_id = seed_dlss_component(&context, &game_id, &live, &baseline);
    let live_hash = renderpilot_detection::sha256_file(&live).expect("live hash");
    let binding = ManagedAddonFile::reused(path_ref(&live), live_hash);
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .try_with_managed_files(vec![binding])
        .expect("valid binding");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");

    uninstall(&context, &game_id).expect("uninstall");

    assert_eq!(std::fs::read(&live).unwrap(), b"independent-library-swap");
    assert_eq!(std::fs::read(&sidecar).unwrap(), b"game-original");
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .unwrap()
            .is_some()
    );
}

#[test]
fn active_owned_host_uninstall_commits_files_topology_and_luma_row_together() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-active-owned-host").expect("id");
    seed_game(&context, &game_id, game.path());
    let runtime = game.path().join("Binaries").join("Win64");
    std::fs::create_dir_all(&runtime).expect("runtime root");
    let host = runtime.join("ReShade64.dll");
    let addon = runtime.join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("addon");
    let binding = ManagedAddonFile::owned(
        path_ref(&host),
        ManagedFileBaseline::Absent,
        renderpilot_detection::sha256_bytes(b"managed host").expect("host hash"),
    );
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .try_with_managed_files(vec![binding])
        .expect("record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");
    let engine_ini = game.path().join("Engine.ini");
    std::fs::write(&engine_ini, b"[SystemSettings]\n").expect("Engine.ini");
    let journal = EngineConfigJournal {
        stable: Some(EngineConfigReceipt {
            schema_version: 1,
            path: engine_ini.to_string_lossy().into_owned(),
            file_created: false,
            encoding: "utf8".to_owned(),
            before_digest: "0".repeat(64),
            after_digest: "1".repeat(64),
            recipe_fingerprint: "2".repeat(64),
            contributions: Vec::new(),
            created_headers: Vec::new(),
            created_header_prefixes: Vec::new(),
            created_header_groups: Vec::new(),
            created_header_ordinals: Vec::new(),
        }),
        pending: None,
    };
    context
        .storage()
        .compare_and_swap_engine_config_journal(&game_id, AddonKind::Luma, None, Some(&journal))
        .expect("journal");
    let topology = seed_active_topology(
        &context,
        &game_id,
        &runtime,
        Some((&host, FileOwnership::Owned, b"managed host")),
        OptiScalerPrerequisiteBinding::None,
    );

    uninstall(&context, &game_id).expect("active uninstall");

    assert!(!addon.exists());
    assert!(!host.exists());
    assert!(
        records::record_of_kind(&context, &game_id, AddonKind::Luma)
            .expect("record")
            .is_none()
    );
    let mut expected_topology = topology;
    expected_topology.downstream = None;
    expected_topology.downstream_origin = None;
    expected_topology.root_prestate = renderpilot_domain::ProxyRootPrestate::Absent;
    assert_eq!(
        context
            .storage()
            .get_proxy_topology(&game_id)
            .expect("topology")
            .expect("topology remains"),
        expected_topology
    );
}

#[test]
fn active_uninstall_rejects_a_topology_runtime_outside_the_catalog_game_root_before_mutation() {
    let game = tempdir().expect("catalog game");
    let outside_runtime = tempdir().expect("outside runtime");
    let game_id = GameId::new("manual:luma-active-outside-catalog-root").expect("id");
    let host = outside_runtime.path().join("ReShade64.dll");
    let outer = outside_runtime.path().join("dxgi.dll");
    let receipt = FileReceipt::owned(
        "outside-runtime-receipt",
        renderpilot_domain::Sha256Hash::new("a".repeat(64)).expect("hash"),
    )
    .expect("receipt");
    let topology = GameProxyTopology {
        id: "optiscaler:luma-outside-catalog-root".to_owned(),
        game_id,
        root_slot: path_ref(&outer),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: path_ref(&outer),
            receipt: receipt.clone(),
        },
        downstream: Some(ProxyLink {
            implementation: ProxyImplementation::ReShade,
            path: path_ref(&host),
            receipt,
        }),
        downstream_origin: Some(path_ref(&outer)),
        root_prestate: ProxyRootPrestate::Absent,
    };

    let error = super::plan::resolve_active_runtime_authority(game.path(), &topology, &host)
        .expect_err("outside topology must be rejected before filesystem planning");
    assert!(matches!(
        error,
        ServiceError::InvalidInput(ref message)
            if message.contains("runtime root is outside the catalog game root")
    ));
}

#[test]
fn active_reused_host_uninstall_preserves_host_and_exact_topology() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-active-reused-host").expect("id");
    seed_game(&context, &game_id, game.path());
    let host = game.path().join("ReShade64.dll");
    let addon = game.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("addon");
    let binding = ManagedAddonFile::reused(
        path_ref(&host),
        renderpilot_detection::sha256_bytes(b"foreign host").expect("host hash"),
    );
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .try_with_managed_files(vec![binding])
        .expect("record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");
    let topology = seed_active_topology(
        &context,
        &game_id,
        game.path(),
        Some((&host, FileOwnership::Reused, b"foreign host")),
        OptiScalerPrerequisiteBinding::None,
    );

    uninstall(&context, &game_id).expect("active uninstall");

    assert!(!addon.exists());
    assert_eq!(std::fs::read(&host).expect("host"), b"foreign host");
    assert_eq!(
        context
            .storage()
            .get_proxy_topology(&game_id)
            .expect("topology")
            .expect("topology remains"),
        topology
    );
}

#[test]
fn active_catalog_dlss_commit_projects_exact_after_components_and_deletes_baseline() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-active-catalog").expect("id");
    seed_game(&context, &game_id, game.path());
    let host = game.path().join("ReShade64.dll");
    let addon = game.path().join("Luma-Game.addon");
    let live = game.path().join("nvngx_dlss.dll");
    let sidecar = game.path().join("nvngx_dlss.dll.bak");
    std::fs::write(&addon, b"addon").expect("addon");
    std::fs::write(&live, b"catalog-overlay").expect("live");
    std::fs::write(&sidecar, b"exact-original").expect("sidecar");
    let baseline_hash = renderpilot_detection::sha256_file(&sidecar).expect("baseline hash");
    let component_id = seed_dlss_component(
        &context,
        &game_id,
        &live,
        &[ComponentFile::new(path_ref(&live)).with_sha256(baseline_hash.clone())],
    );
    let binding = ManagedAddonFile::owned(
        path_ref(&live),
        ManagedFileBaseline::Present {
            sha256: baseline_hash.clone(),
        },
        renderpilot_detection::sha256_file(&live).expect("live hash"),
    );
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .try_with_managed_files(vec![binding])
        .expect("record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");
    let topology = seed_active_topology(
        &context,
        &game_id,
        game.path(),
        Some((&host, FileOwnership::Reused, b"foreign host")),
        OptiScalerPrerequisiteBinding::None,
    );

    uninstall(&context, &game_id).expect("active catalog uninstall");

    assert_eq!(std::fs::read(&live).expect("live"), b"exact-original");
    assert!(!sidecar.exists());
    assert!(host.exists());
    assert_eq!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("baseline"),
        None
    );
    let components = context
        .storage()
        .list_components_for_game(&game_id)
        .expect("components");
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].files()[0].sha256(), Some(&baseline_hash));
    assert_eq!(
        context
            .storage()
            .get_proxy_topology(&game_id)
            .expect("topology")
            .expect("topology remains"),
        topology
    );
}

#[test]
fn active_external_addon_path_uninstall_removes_payload_under_sealed_root() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let payload = tempdir().expect("payload");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-active-external").expect("id");
    seed_game(&context, &game_id, game.path());
    std::fs::write(
        game.path().join("ReShade.ini"),
        format!("[ADDON]\r\nAddonPath={}\r\n", payload.path().display()),
    )
    .expect("reshade config");
    let host = game.path().join("ReShade64.dll");
    let addon = payload.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("addon");
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon));
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");
    let topology = seed_active_topology(
        &context,
        &game_id,
        game.path(),
        Some((&host, FileOwnership::Reused, b"foreign host")),
        OptiScalerPrerequisiteBinding::None,
    );

    uninstall(&context, &game_id).expect("active external uninstall");

    assert!(!addon.exists());
    assert!(host.exists());
    assert_eq!(
        context
            .storage()
            .get_proxy_topology(&game_id)
            .expect("topology")
            .expect("topology remains"),
        topology
    );
}

#[test]
fn active_missing_downstream_fails_before_any_write() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("game");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-active-missing-downstream").expect("id");
    seed_game(&context, &game_id, game.path());
    let addon = game.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("addon");
    let engine_ini = game.path().join("Engine.ini");
    std::fs::write(&engine_ini, b"[SystemSettings]\n").expect("Engine.ini");
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon));
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");
    let journal = EngineConfigJournal {
        stable: Some(EngineConfigReceipt {
            schema_version: 1,
            path: engine_ini.to_string_lossy().into_owned(),
            file_created: false,
            encoding: "utf8".to_owned(),
            before_digest: "0".repeat(64),
            after_digest: "1".repeat(64),
            recipe_fingerprint: "2".repeat(64),
            contributions: Vec::new(),
            created_headers: Vec::new(),
            created_header_prefixes: Vec::new(),
            created_header_groups: Vec::new(),
            created_header_ordinals: Vec::new(),
        }),
        pending: None,
    };
    context
        .storage()
        .compare_and_swap_engine_config_journal(&game_id, AddonKind::Luma, None, Some(&journal))
        .expect("journal");
    let persisted_before = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("record")
        .expect("persisted record");
    let topology = seed_active_topology(
        &context,
        &game_id,
        game.path(),
        None,
        OptiScalerPrerequisiteBinding::None,
    );
    let proxy = game.path().join("dxgi.dll");
    let config = game.path().join("OptiScaler.ini");
    let executable = game.path().join("Game.exe");
    let files_before = [
        (proxy.clone(), std::fs::read(&proxy).expect("proxy")),
        (config.clone(), std::fs::read(&config).expect("config")),
        (
            executable.clone(),
            std::fs::read(&executable).expect("executable"),
        ),
    ];

    let error = uninstall(&context, &game_id).expect_err("missing downstream");

    assert!(matches!(error, ServiceError::InvalidInput(_)));
    assert!(addon.exists());
    assert_eq!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("record")
            .as_ref(),
        Some(&persisted_before)
    );
    assert_eq!(
        context
            .storage()
            .get_proxy_topology(&game_id)
            .expect("topology")
            .expect("topology remains"),
        topology
    );
    for (path, bytes) in files_before {
        assert_eq!(std::fs::read(&path).expect("file remains"), bytes);
    }
}

#[test]
fn public_luma_uninstall_waits_at_the_game_mutation_boundary() {
    let context = std::sync::Arc::new(Context::from_storage(
        renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("storage"),
    ));
    let game_id = GameId::new(format!("manual:luma-lock-{}", ulid::Ulid::generate())).expect("id");
    let held = crate::game_mutation_lock::blocking_lock(&game_id);
    let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    crate::game_mutation_lock::set_lock_attempt_hook(&game_id, attempt_tx);

    let worker_context = std::sync::Arc::clone(&context);
    let worker_game = game_id;
    let worker = std::thread::spawn(move || {
        done_tx
            .send(uninstall(&worker_context, &worker_game))
            .expect("report result");
    });

    attempt_rx.recv().expect("entrypoint reached lock");
    assert!(matches!(
        done_rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    drop(held);
    assert!(done_rx.recv().expect("completed").is_err());
    worker.join().expect("worker");
}

#[test]
fn external_owner_release_is_receipt_bounded_and_leaves_engine_and_catalog_owners_untouched() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("current game");
    let external = tempdir().expect("old external payload root");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-external-owner-release").expect("id");
    seed_game(&context, &game_id, game.path());

    let current_ini = game.path().join("ReShade.ini");
    std::fs::write(&current_ini, b"[ADDON]\nAddonPath=current\n").expect("current INI");
    let current_ini_before = std::fs::read(&current_ini).expect("current INI bytes");
    let addon = external.path().join("Luma-Game.addon");
    let owned_ini = external.path().join("ReShade.ini");
    let owned_host = external.path().join("ReShade64.dll");
    let inferred_log = external.path().join("ReShade.log");
    let unrelated = external.path().join("neighbor.ini");
    std::fs::write(&owned_ini, b"old owner receipt").expect("owned external INI");
    std::fs::write(&owned_host, b"old proxy receipt").expect("owned external host");
    std::fs::write(&inferred_log, b"unowned host log").expect("unowned external log");
    std::fs::write(&unrelated, b"unowned neighbor").expect("unowned neighbor");

    let live = external.path().join("nvngx_dlss.dll");
    let sidecar = external.path().join("nvngx_dlss.dll.bak");
    std::fs::write(&live, b"managed overlay").expect("managed live");
    std::fs::write(&sidecar, b"managed baseline").expect("managed backup");
    let baseline_hash = renderpilot_detection::sha256_file(&sidecar).expect("baseline hash");

    // Keep an unrelated catalog component under the actual installation root.
    // Luma's managed file may live under an external ReShade AddonPath, but the
    // game component catalog is scanned from GameInstallation.install_path.
    let catalog_live = game.path().join("nvngx_dlss.dll");
    let catalog_sidecar = crate::fs::backup_path(&catalog_live).expect("catalog sidecar");
    std::fs::write(&catalog_live, b"catalog overlay").expect("catalog live");
    std::fs::write(&catalog_sidecar, b"catalog baseline").expect("catalog backup");
    let catalog_baseline_hash =
        renderpilot_detection::sha256_file(&catalog_sidecar).expect("catalog baseline hash");
    let component_id = seed_dlss_component(
        &context,
        &game_id,
        &catalog_live,
        &[ComponentFile::new(path_ref(&catalog_live)).with_sha256(catalog_baseline_hash)],
    );
    let binding = ManagedAddonFile::owned(
        path_ref(&live),
        ManagedFileBaseline::Present {
            sha256: baseline_hash,
        },
        renderpilot_detection::sha256_file(&live).expect("managed hash"),
    );
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_created_file(path_ref(&owned_ini))
        .with_created_file(path_ref(&owned_host))
        .try_with_managed_files(vec![binding])
        .expect("managed owner record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("owner record");

    let engine_ini = db.path().join("AppData").join("Engine.ini");
    std::fs::create_dir_all(engine_ini.parent().expect("Engine.ini parent"))
        .expect("Engine.ini directory");
    std::fs::write(&engine_ini, b"[SystemSettings]\nold contribution\n").expect("Engine.ini");
    let journal = stable_engine_journal(&engine_ini);
    context
        .storage()
        .compare_and_swap_engine_config_journal(&game_id, AddonKind::Luma, None, Some(&journal))
        .expect("standalone Engine owner");
    let persisted_before = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("owner read")
        .expect("owner");
    let engine_owner_before = context
        .storage()
        .engine_config_journal_owner(&game_id)
        .expect("Engine owner read")
        .expect("standalone Engine owner");

    let prepared = super::prepare_external_owner_release(&context, &game_id, &persisted_before)
        .expect("prepare inactive external owner release");
    assert!(prepared.baseline_mutations().is_empty());
    assert_eq!(prepared.next_components().len(), 1);
    assert!(prepared.targets().paths.iter().any(|path| path == &live));
    assert!(prepared.targets().paths.iter().any(|path| path == &sidecar));
    let canonical_game_root = crate::paths::canonical_candidate(game.path()).expect("game root");
    for path in prepared
        .targets()
        .paths
        .iter()
        .chain(&prepared.targets().roots)
    {
        let canonical = crate::paths::canonical_candidate(path).expect("prepared target");
        assert_eq!(
            renderpilot_domain::normalized_path_relation(
                &canonical_game_root.to_string_lossy(),
                &canonical.to_string_lossy(),
            ),
            renderpilot_domain::NormalizedPathRelation::Disjoint,
            "target overlaps current game root: {}",
            path.display()
        );
    }

    prepared
        .apply_filesystem_only()
        .expect("apply receipt-only file effects");

    assert!(
        !owned_ini.exists(),
        "the retained created-file receipt is removed"
    );
    assert_eq!(
        std::fs::read(&live).expect("restored DLSS"),
        b"managed baseline"
    );
    assert!(!sidecar.exists(), "the owned backup sidecar was consumed");
    assert_eq!(
        std::fs::read(&unrelated).expect("neighbor"),
        b"unowned neighbor"
    );
    assert!(
        !owned_host.exists(),
        "the retained host file receipt is removed"
    );
    assert_eq!(
        std::fs::read(&inferred_log).expect("inferred log"),
        b"unowned host log",
        "host-adjacent logs are outside the retained receipt closure"
    );
    assert_eq!(
        std::fs::read(&current_ini).expect("current INI"),
        current_ini_before
    );
    assert_eq!(
        std::fs::read(&engine_ini).expect("Engine.ini"),
        b"[SystemSettings]\nold contribution\n"
    );
    assert_eq!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("owner read")
            .as_ref(),
        Some(&persisted_before),
        "filesystem-only release must not replace or delete canonical ownership"
    );
    assert_eq!(
        context
            .storage()
            .engine_config_journal_owner(&game_id)
            .expect("Engine owner read")
            .as_ref(),
        Some(&engine_owner_before),
        "filesystem-only release must not release standalone Engine ownership"
    );
    assert!(
        context
            .storage()
            .get_component_backup(&component_id)
            .expect("baseline inventory")
            .is_some(),
        "an unrelated in-root catalog component remains untouched"
    );
}

#[test]
fn external_owner_release_projects_exact_sv_files_and_rejects_catalog_effects() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("current game");
    let external = tempdir().expect("old external payload root");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-external-svam-intents").expect("id");
    seed_game(&context, &game_id, game.path());

    let addon = external.path().join("Luma-Game.addon");
    let created = external.path().join("Luma").join("Shader.hlsl");
    let shadowed = external.path().join("ReShade64.dll");
    let backup = crate::fs::backup_path(&shadowed).expect("backup path");
    std::fs::create_dir_all(created.parent().expect("created parent")).expect("Luma dir");
    std::fs::write(&created, b"created by old Luma").expect("created payload");
    std::fs::write(&shadowed, b"old proxy").expect("shadowed host");
    std::fs::write(&backup, b"host baseline").expect("host backup");

    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_created_file(path_ref(&created))
        .with_backed_up_file(path_ref(&shadowed));
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("owner record");
    let persisted = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("owner read")
        .expect("owner");

    let prepared = super::prepare_external_owner_release(&context, &game_id, &persisted)
        .expect("prepare external owner release");
    prepared
        .ensure_no_catalog_effects()
        .expect("receipt-only release leaves the catalog unchanged");
    let intents = prepared
        .file_intents_for_shared_vulkan()
        .expect("exact SVAM file participants");
    assert_eq!(
        intents,
        vec![
            crate::addons::shared_vulkan_mutation::FileIntent {
                live_path: created,
                before: Some(b"created by old Luma".to_vec()),
                after: None,
            },
            crate::addons::shared_vulkan_mutation::FileIntent {
                live_path: shadowed,
                before: Some(b"old proxy".to_vec()),
                after: Some(b"host baseline".to_vec()),
            },
            crate::addons::shared_vulkan_mutation::FileIntent {
                live_path: backup,
                before: Some(b"host baseline".to_vec()),
                after: None,
            },
        ]
    );

    let cascade_context = Context::open_at(db.path().join("cascade.sqlite")).expect("context");
    let cascade_game = tempdir().expect("cascade current game");
    let cascade_external = tempdir().expect("cascade external root");
    let cascade_id = GameId::new("manual:luma-external-svam-cascade").expect("cascade id");
    seed_game(&cascade_context, &cascade_id, cascade_game.path());
    let managed = cascade_game.path().join("nvngx_dlss.dll");
    let sidecar = crate::fs::backup_path(&managed).expect("managed sidecar");
    std::fs::write(&managed, b"managed overlay").expect("managed file");
    std::fs::write(&sidecar, b"managed baseline").expect("managed backup");
    let baseline_hash = renderpilot_detection::sha256_file(&sidecar).expect("baseline hash");
    let component_id = seed_dlss_component(
        &cascade_context,
        &cascade_id,
        &managed,
        &[ComponentFile::new(path_ref(&managed)).with_sha256(baseline_hash.clone())],
    );
    let managed_owner = ManagedAddonFile::owned(
        path_ref(&managed),
        ManagedFileBaseline::Present {
            sha256: baseline_hash,
        },
        renderpilot_detection::sha256_file(&managed).expect("managed hash"),
    );
    let addon = cascade_external.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"external owner payload").expect("external addon");
    let owner = InstalledAddon::new(cascade_id.clone(), AddonKind::Luma, path_ref(&addon))
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_created_file(path_ref(&addon))
        .try_with_managed_files(vec![managed_owner])
        .expect("managed owner");
    cascade_context
        .storage()
        .upsert_installed_addon(&owner)
        .expect("owner record");
    let persisted = cascade_context
        .storage()
        .get_installed_addon(&cascade_id)
        .expect("owner read")
        .expect("owner");
    let Err(error) =
        super::prepare_external_owner_release(&cascade_context, &cascade_id, &persisted)
    else {
        panic!("a game-root managed receipt cannot join an external-only release");
    };
    assert!(
        matches!(error, ServiceError::InvalidInput(_)),
        "cross-root component rollback is rejected before file-only projection"
    );
    assert_eq!(
        std::fs::read(&managed).expect("game-root managed file remains"),
        b"managed overlay"
    );
    assert_eq!(
        std::fs::read(&sidecar).expect("game-root managed baseline remains"),
        b"managed baseline"
    );
    assert!(
        cascade_context
            .storage()
            .get_installed_addon(&cascade_id)
            .expect("owner read")
            .as_ref()
            .is_some_and(|current| current.eq_ignoring_persistence_timestamps(&persisted)),
        "preflight rejection preserves the canonical owner"
    );
    assert!(
        cascade_context
            .storage()
            .get_component_backup(&component_id)
            .expect("component backup remains")
            .is_some()
    );
}

#[test]
fn external_owner_release_rejects_modified_managed_file_before_any_write() {
    let db = tempdir().expect("db");
    let game = tempdir().expect("current game");
    let external = tempdir().expect("old external payload root");
    let context = Context::open_at(db.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-external-owner-modified-managed").expect("id");
    seed_game(&context, &game_id, game.path());
    let addon = external.path().join("Luma-Game.addon");
    let live = external.path().join("nvngx_dlss.dll");
    let untracked = external.path().join("fresh.ini");
    std::fs::write(&live, b"changed managed bytes").expect("modified managed file");
    std::fs::write(&untracked, b"leave me").expect("untracked neighbor");
    let binding = ManagedAddonFile::owned(
        path_ref(&live),
        ManagedFileBaseline::Absent,
        renderpilot_detection::sha256_bytes(b"expected bytes").expect("expected hash"),
    );
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon))
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .try_with_managed_files(vec![binding])
        .expect("managed owner record");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("owner record");
    let persisted = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("owner read")
        .expect("owner");

    let Err(error) = super::prepare_external_owner_release(&context, &game_id, &persisted) else {
        panic!("modified managed path must fail during planning");
    };

    assert!(matches!(error, ServiceError::CommandFailed(_)));
    assert_eq!(
        std::fs::read(&live).expect("modified file remains"),
        b"changed managed bytes"
    );
    assert_eq!(
        std::fs::read(&untracked).expect("neighbor remains"),
        b"leave me"
    );
    assert_eq!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("owner read")
            .as_ref(),
        Some(&persisted)
    );
}
