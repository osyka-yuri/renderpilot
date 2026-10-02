use super::*;
use crate::repositories::CompleteScanWriteUnit;
use renderpilot_application::{
    ComponentRepository, GameRepository, InstalledAddonRepository, OptiScalerStateRepository,
    ProxyTopologyRepository, SharedArtifactRepository,
};
use renderpilot_domain::{
    AddonKind, ComponentFile, ComponentId, ComponentKind, EngineConfigJournal, EngineConfigReceipt,
    FileReceipt, GameIdentity, GameProxyTopology, GameRuntime, InstalledAddon,
    InstalledAddonHostKind, Launcher, LibraryComponent, LibraryTechnology, ManagedAddonFile,
    ManagedFileBaseline, OptiScalerAdoptionState, OptiScalerConfigurationBaseline,
    OptiScalerFileBaseline, OptiScalerFileCleanup, OptiScalerFileReceipt, OptiScalerFileRole,
    OptiScalerInstallStateParts, OptiScalerModuleRuntimeBinding, OptiScalerPrerequisiteBinding,
    PathRef, Platform, ProxyImplementation, ProxyLink, ProxyRootPrestate, RenoDxConfigReceipt,
    RenoDxSetPathBaseline, RenoDxSetPathValue, RootAuthority, Sha256Hash, SharedArtifactKind,
    SharedArtifactOrigin, SharedArtifactRecord, Swappability, TrackedSource, TrackedSourceRole,
    from_persisted,
};

fn sample_game(id: &str, root: &str, launcher: Launcher) -> GameInstallation {
    GameInstallation::new(
        GameIdentity::new(GameId::new(id).expect("id"), "Game", launcher).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(root).expect("root"),
    )
    .with_root_authority(RootAuthority::UserConfirmed)
}

fn addon(game: &GameInstallation) -> InstalledAddon {
    let file = PathRef::new(format!("{}/renodx.addon64", game.install_path().as_str()))
        .expect("addon file");
    InstalledAddon::new(game.id().clone(), AddonKind::RenoDx, file.clone()).with_created_file(file)
}

fn duplicate_component(game: &GameInstallation, file: &str) -> LibraryComponent {
    LibraryComponent::new(
        ComponentId::new("component:duplicate").expect("component id"),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(
        PathRef::new(file).expect("component file"),
    ))
}

fn seed_artifact(storage: &SqliteStorage, id: &str, game_id: &str, trust: &str) {
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO library_artifacts
                (id, technology, file_name, files_json, metadata_json, source,
                 source_game_id, trust_level, created_at, updated_at)
             VALUES (?1, 'dlss_super_resolution', ?2, '[{\"path\":\"C:/Games/shared.dll\"}]',
                     '{}', 'scan', ?3, ?4, 1, 1)",
            rusqlite::params![id, format!("{id}.dll"), game_id, trust],
        )
        .expect("seed artifact");
}

#[test]
fn absence_keeps_registration_but_hides_active_reads_and_complete_scan_reactivates_atomically() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = sample_game("manual:one", "C:/Games/One", Launcher::Manual);
    let unrelated = sample_game("steam:two", "C:/Games/Two", Launcher::Steam);
    storage
        .upsert_games(&[game.clone(), unrelated.clone()])
        .expect("games");
    storage
        .replace_components_for_game(
            game.id(),
            &[duplicate_component(&game, "C:/Games/One/a.dll")],
        )
        .expect("stale component");
    storage
        .upsert_installed_addon(&addon(&game))
        .expect("local add-on");
    storage
        .upsert_installed_addon(&addon(&unrelated))
        .expect("other local add-on");
    seed_artifact(
        &storage,
        "artifact:local",
        game.id().as_str(),
        "local_observed",
    );
    seed_artifact(
        &storage,
        "artifact:downloaded",
        game.id().as_str(),
        "catalog_downloaded",
    );
    seed_artifact(
        &storage,
        "artifact:imported",
        game.id().as_str(),
        "user_imported",
    );
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO component_backups (component_id, game_id, files_json, created_at, updated_at)
             VALUES ('component:stale', ?1, '[]', 1, 1)",
            [game.id().as_str()],
        )
        .expect("backup");

    let before_generation = storage.catalog_generation();
    let authority_before_absence = storage
        .catalog_readiness(game.id())
        .expect("current authority");
    let absent = storage
        .mark_installation_absent(
            &game,
            AuthorityCas::new(authority_before_absence.authority_epoch()),
        )
        .expect("mark absent");
    assert!(absent.is_absent());
    assert_eq!(absent.revision(), 1);
    assert!(storage.catalog_generation() > before_generation);
    assert_eq!(storage.list_games().expect("raw registrations").len(), 2);
    assert_eq!(
        storage.list_active_games().expect("active games"),
        vec![unrelated.clone()]
    );
    assert_eq!(
        storage.find_active_game(game.id()).expect("active lookup"),
        None
    );
    assert_eq!(
        storage
            .list_distinct_game_libraries()
            .expect("active facets"),
        Vec::<String>::new()
    );
    assert_eq!(
        storage
            .list_distinct_game_launchers()
            .expect("active launchers"),
        vec!["Steam"]
    );

    let epoch = storage
        .catalog_readiness(game.id())
        .expect("readiness")
        .authority_epoch();
    let duplicate = [
        duplicate_component(&game, "C:/Games/One/a.dll"),
        duplicate_component(&game, "C:/Games/One/b.dll"),
    ];
    let failed = storage.save_complete_scan_write_unit(CompleteScanWriteUnit {
        game: &game,
        components: &duplicate,
        artifacts: &[],
        observations: &[],
        authority: AuthorityCas::new(epoch),
        prune_empty_operations: false,
    });
    assert!(failed.is_err(), "duplicate component ids force rollback");
    assert!(
        storage
            .is_installation_absent(game.id())
            .expect("still absent")
    );
    assert!(
        storage
            .get_installed_addon(game.id())
            .expect("local row")
            .is_some()
    );
    assert_eq!(
        count_for_game(&storage, "component_backups", game.id().as_str()),
        1
    );
    assert_eq!(count_for_artifact(&storage, "artifact:local"), 1);

    storage
        .save_complete_scan_write_unit(CompleteScanWriteUnit {
            game: &game,
            components: &[],
            artifacts: &[],
            observations: &[],
            authority: AuthorityCas::new(epoch),
            prune_empty_operations: false,
        })
        .expect("complete scan cleans then republishes");
    assert!(!storage.is_installation_absent(game.id()).expect("active"));
    assert!(
        storage
            .get_installed_addon(game.id())
            .expect("local GC")
            .is_none()
    );
    assert_eq!(
        count_for_game(&storage, "component_backups", game.id().as_str()),
        0
    );
    assert_eq!(count_for_artifact(&storage, "artifact:local"), 0);
    assert_eq!(count_for_artifact(&storage, "artifact:downloaded"), 1);
    assert_eq!(count_for_artifact(&storage, "artifact:imported"), 1);
    assert!(
        storage
            .get_installed_addon(unrelated.id())
            .expect("other game")
            .is_some()
    );
    assert_eq!(storage.list_active_games().expect("active games").len(), 2);
}

#[test]
fn absent_collection_deletes_optiscaler_state_before_its_restricted_topology() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = sample_game(
        "manual:collection-optiscaler-fk",
        "C:/Games/Test",
        Launcher::Manual,
    );
    storage.upsert_game(&game).expect("game");

    let topology_id = "optiscaler:collection-optiscaler-fk";
    let proxy = PathRef::new("C:/Games/Test/dxgi.dll").expect("proxy path");
    let topology = GameProxyTopology {
        id: topology_id.to_owned(),
        game_id: game.id().clone(),
        root_slot: proxy.clone(),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: proxy,
            receipt: FileReceipt::owned(
                "proxy-id",
                Sha256Hash::new("a".repeat(64)).expect("proxy hash"),
            )
            .expect("proxy receipt"),
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    };
    let state = from_persisted(
        OptiScalerInstallStateParts {
            game_id: game.id().clone(),
            release_id: "v0.9.4".to_owned(),
            manifest_revision: "test".to_owned(),
            archive_sha256: None,
            source: None,
            target_exe_path: PathRef::new("C:/Games/Test/Game.exe").expect("exe path"),
            target_dir: PathRef::new("C:/Games/Test").expect("target path"),
            modules: vec!["core".to_owned()],
            release_files: vec![OptiScalerFileReceipt {
                path: PathRef::new("C:/Games/Test/OptiScaler.ini").expect("config path"),
                installed: FileReceipt::owned(
                    "config-id",
                    Sha256Hash::new("b".repeat(64)).expect("config hash"),
                )
                .expect("config receipt"),
                role: OptiScalerFileRole::Configuration,
                cleanup: OptiScalerFileCleanup::RemoveIfUnchanged,
                baseline: renderpilot_domain::OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: vec![OptiScalerModuleRuntimeBinding {
                module: "core".to_owned(),
                path: PathRef::new("C:/Games/Test/core.dll").expect("runtime path"),
                installed: FileReceipt::owned(
                    "runtime-id",
                    Sha256Hash::new("c".repeat(64)).expect("runtime hash"),
                )
                .expect("runtime receipt"),
                baseline: OptiScalerFileBaseline::Absent,
            }],
            directory_receipts: Vec::new(),
            proxy_topology_id: Some(topology_id.to_owned()),
            config_schema: 1,
            config_base_release: "v0.9.4".to_owned(),
            adoption_state: OptiScalerAdoptionState::Managed,
            prerequisite_binding: OptiScalerPrerequisiteBinding::None,
            created_at: None,
            updated_at: None,
        },
        OptiScalerConfigurationBaseline::absent(),
    )
    .expect("valid installed OptiScaler state");
    storage
        .with_transaction(|transaction| {
            crate::repositories::proxy_topologies::upsert_within_transaction(
                transaction,
                &topology,
            )?;
            crate::repositories::optiscaler_states::upsert_within_transaction(transaction, &state)
        })
        .expect("persist topology and referencing state");

    let foreign_keys = storage
        .with_connection(|connection| {
            connection
                .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .map_err(crate::error::storage_error)
        })
        .expect("foreign-key state");
    assert_eq!(
        foreign_keys, 1,
        "the regression requires enforced foreign keys"
    );
    assert!(
        storage
            .get_optiscaler_install_state(game.id())
            .expect("state before collection")
            .is_some()
    );
    assert!(
        storage
            .get_proxy_topology(game.id())
            .expect("topology before collection")
            .is_some()
    );

    let readiness = storage.catalog_readiness(game.id()).expect("readiness");
    storage
        .mark_installation_absent(&game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("mark absent");
    let absent_readiness = storage
        .catalog_readiness(game.id())
        .expect("readiness after absence");
    storage
        .collect_absent_installation(&game, AuthorityCas::new(absent_readiness.authority_epoch()))
        .expect("collect the OptiScaler aggregate in foreign-key order");

    assert!(
        storage
            .get_optiscaler_install_state(game.id())
            .expect("state after collection")
            .is_none()
    );
    assert!(
        storage
            .get_proxy_topology(game.id())
            .expect("topology after collection")
            .is_none()
    );
    assert!(storage.is_installation_absent(game.id()).expect("absent"));
    assert!(
        storage
            .find_game(game.id())
            .expect("raw registration")
            .is_some()
    );
    assert!(
        storage
            .find_active_game(game.id())
            .expect("active reads")
            .is_none()
    );
}

#[test]
fn pending_binding_preserves_authority_and_stale_collector_cas_cannot_collect() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = sample_game("manual:pending", "C:/Games/Pending", Launcher::Manual);
    storage.upsert_game(&game).expect("game");
    storage
        .upsert_installed_addon(&addon(&game))
        .expect("local add-on");
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO pending_file_mutations (id, game_id, feature, state, manifest_json)
             VALUES ('mutation:pending', ?1, 'fixture', 'prepared', '{}')",
            [game.id().as_str()],
        )
        .expect("pending mutation");

    let before = storage.catalog_readiness(game.id()).expect("before");
    let absent = storage
        .mark_installation_absent(&game, AuthorityCas::new(before.authority_epoch()))
        .expect("mark with actual pending binding");
    assert!(absent.is_absent());
    assert_eq!(
        storage
            .catalog_readiness(game.id())
            .expect("authority held"),
        before
    );
    assert!(
        storage
            .collect_absent_installation(&game, AuthorityCas::new(before.authority_epoch()))
            .is_err()
    );
    assert!(
        storage
            .get_installed_addon(game.id())
            .expect("preserved local row")
            .is_some()
    );

    storage
        .with_transaction(|transaction| {
            transaction
                .execute(
                    "DELETE FROM pending_file_mutations WHERE id='mutation:pending'",
                    [],
                )
                .map_err(crate::error::storage_error)?;
            observations::invalidate_game_authority_within_transaction(
                transaction,
                game.id(),
                "test_pending_completed",
                None,
            )?;
            Ok(())
        })
        .expect("resolve the native fixture");
    assert!(
        storage
            .collect_absent_installation(&game, AuthorityCas::new(before.authority_epoch()))
            .is_err(),
        "old epoch cannot collect after pending completion"
    );
    assert!(
        storage
            .get_installed_addon(game.id())
            .expect("still present")
            .is_some()
    );
    let current = storage
        .catalog_readiness(game.id())
        .expect("current readiness");
    storage
        .collect_absent_installation(&game, AuthorityCas::new(current.authority_epoch()))
        .expect("fresh authority collects local metadata");
    assert!(
        storage
            .get_installed_addon(game.id())
            .expect("collected")
            .is_none()
    );
    assert!(
        storage
            .is_installation_absent(game.id())
            .expect("stays absent")
    );
    let after_collection = storage
        .catalog_readiness(game.id())
        .expect("collection readiness");
    assert!(after_collection.authority_epoch() > current.authority_epoch());
    let generation_after_collection = storage.catalog_generation();
    let artifact_revision_after_collection = storage
        .library_artifact_revision()
        .expect("artifact revision after collection");
    storage
        .collect_absent_installation(&game, AuthorityCas::new(after_collection.authority_epoch()))
        .expect("second collection is a no-op");
    assert_eq!(
        storage
            .catalog_readiness(game.id())
            .expect("still invalidated")
            .authority_epoch(),
        after_collection.authority_epoch()
    );
    assert_eq!(storage.catalog_generation(), generation_after_collection);
    assert_eq!(
        storage
            .library_artifact_revision()
            .expect("unchanged artifact revision"),
        artifact_revision_after_collection
    );
}

#[test]
fn empty_collection_invalidates_ready_authority_after_pending_native_work_terminalizes() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = sample_game(
        "manual:ready-pending",
        "C:/Games/ReadyPending",
        Launcher::Manual,
    );
    storage.upsert_game(&game).expect("game");
    storage
        .save_complete_scan_write_unit(CompleteScanWriteUnit {
            game: &game,
            components: &[],
            artifacts: &[],
            observations: &[],
            authority: AuthorityCas::new(0),
            prune_empty_operations: false,
        })
        .expect("publish ready scan");
    let ready = storage
        .catalog_readiness(game.id())
        .expect("ready authority");
    assert!(matches!(ready, observations::CatalogReadiness::Complete(_)));

    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO pending_file_mutations (id, game_id, feature, state, manifest_json)
             VALUES ('mutation:ready', ?1, 'fixture', 'prepared', '{}')",
            [game.id().as_str()],
        )
        .expect("pending native binding");
    storage
        .mark_installation_absent(&game, AuthorityCas::new(ready.authority_epoch()))
        .expect("mark absent while binding is pending");
    assert_eq!(
        storage
            .catalog_readiness(game.id())
            .expect("preserved readiness"),
        ready
    );

    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "DELETE FROM pending_file_mutations WHERE id='mutation:ready'",
            [],
        )
        .expect("native work reached terminal state");
    let generation_before_collection = storage.catalog_generation();
    storage
        .collect_absent_installation(&game, AuthorityCas::new(ready.authority_epoch()))
        .expect("invalidate ready authority even with no local rows");
    let invalidated = storage
        .catalog_readiness(game.id())
        .expect("invalidated readiness");
    assert!(matches!(
        invalidated,
        observations::CatalogReadiness::Invalidated { .. }
    ));
    assert_eq!(invalidated.authority_epoch(), ready.authority_epoch() + 1);
    assert!(storage.catalog_generation() > generation_before_collection);

    let generation_after_collection = storage.catalog_generation();
    let artifact_revision = storage
        .library_artifact_revision()
        .expect("artifact revision");
    storage
        .collect_absent_installation(&game, AuthorityCas::new(invalidated.authority_epoch()))
        .expect("repeated empty collection");
    assert_eq!(
        storage
            .catalog_readiness(game.id())
            .expect("same invalidated authority")
            .authority_epoch(),
        invalidated.authority_epoch()
    );
    assert_eq!(storage.catalog_generation(), generation_after_collection);
    assert_eq!(
        storage
            .library_artifact_revision()
            .expect("stable artifact revision"),
        artifact_revision
    );
}

#[test]
fn collection_clears_only_contained_stable_engine_owner_and_preserves_global_shared_owner() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let contained = sample_game("manual:contained", "C:/Games/Contained", Launcher::Manual);
    let external = sample_game("manual:external", "C:/Games/External", Launcher::Manual);
    storage
        .upsert_games(&[contained.clone(), external.clone()])
        .expect("games");

    let contained_journal = stable_journal("C:/Games/Contained/Engine.ini");
    let external_journal = stable_journal("C:/Users/Shared/AppData/Engine.ini");
    seed_engine_journal(&storage, contained.id().as_str(), &contained_journal);
    seed_engine_journal(&storage, external.id().as_str(), &external_journal);
    storage
        .upsert_shared_artifact(&SharedArtifactRecord::new(
            SharedArtifactKind::RenoDxVulkanLayer,
            PathRef::new("C:/ProgramData/ReShade").expect("shared dir"),
            PathRef::new("C:/ProgramData/ReShade/ReShade64.json").expect("manifest"),
            PathRef::new("C:/ProgramData/ReShade/ReShade64.dll").expect("shared dll"),
            SharedArtifactOrigin::RenderPilotCreated,
        ))
        .expect("global shared artifact");

    for game in [&contained, &external] {
        storage
            .mark_installation_absent(game, AuthorityCas::new(0))
            .expect("mark absent");
        let current_epoch = storage
            .catalog_readiness(game.id())
            .expect("readiness after absence transition")
            .authority_epoch();
        storage
            .collect_absent_installation(game, AuthorityCas::new(current_epoch))
            .expect("collect local rows");
    }

    assert_eq!(
        storage
            .engine_config_journal_owner(contained.id())
            .expect("contained owner"),
        None,
        "an Engine.ini receipt under the removed root is source-local"
    );
    assert_eq!(
        storage
            .engine_config_journal_owner(external.id())
            .expect("external owner")
            .expect("outside owner retained")
            .journal,
        external_journal,
        "Engine.ini outside the removed root retains its canonical owner"
    );
    assert_eq!(
        storage
            .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
            .expect("global owner lookup")
            .expect("global owner retained")
            .install_dir()
            .as_str(),
        "C:/ProgramData/ReShade",
        "game absence must not collect app-wide shared artifact ownership"
    );
}

#[test]
fn shared_vulkan_game_binding_survives_collection_and_reactivation_but_proxy_receipt_does_not() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let shared_game = sample_game("manual:shared-vulkan", "C:/Games/Shared", Launcher::Manual);
    let proxy_game = sample_game("manual:proxy", "C:/Games/Proxy", Launcher::Manual);
    storage
        .upsert_games(&[shared_game.clone(), proxy_game.clone()])
        .expect("games");
    let shared_addon = addon(&shared_game)
        .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
        .with_registered_exe_path(
            PathRef::new("C:/Games/Shared/Game.exe").expect("registered executable"),
        );
    let proxy_addon = addon(&proxy_game).with_host_kind(InstalledAddonHostKind::Proxy);
    storage
        .upsert_installed_addon(&shared_addon)
        .expect("shared host binding");
    storage
        .upsert_installed_addon(&proxy_addon)
        .expect("local proxy receipt");
    seed_artifact(
        &storage,
        "artifact:shared-source-local",
        shared_game.id().as_str(),
        "local_observed",
    );
    let artifact_revision_before_collection = storage
        .library_artifact_revision()
        .expect("artifact revision before collection");

    let component = duplicate_component(&shared_game, "C:/Games/Shared/stale.dll");
    storage
        .replace_components_for_game(shared_game.id(), std::slice::from_ref(&component))
        .expect("local component");
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO component_backups (component_id, game_id, files_json, created_at, updated_at)
             VALUES (?1, ?2, '[]', 1, 1)",
            rusqlite::params![component.id().as_str(), shared_game.id().as_str()],
        )
        .expect("local rollback backup");

    for game in [&shared_game, &proxy_game] {
        let readiness = storage.catalog_readiness(game.id()).expect("authority");
        storage
            .mark_installation_absent(game, AuthorityCas::new(readiness.authority_epoch()))
            .expect("mark absent");
        let after_mark = storage
            .catalog_readiness(game.id())
            .expect("authority after mark");
        storage
            .collect_absent_installation(game, AuthorityCas::new(after_mark.authority_epoch()))
            .expect("collect root-local state");
    }

    let retained = storage
        .get_installed_addon(shared_game.id())
        .expect("shared host owner")
        .expect("external owner retained");
    assert_eq!(
        retained.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    );
    assert_eq!(
        retained.registered_exe_path().map(PathRef::as_str),
        Some("C:/Games/Shared/Game.exe")
    );
    assert!(
        storage
            .get_installed_addon(proxy_game.id())
            .expect("proxy receipt")
            .is_none()
    );
    assert_eq!(
        count_for_game(&storage, "components", shared_game.id().as_str()),
        0
    );
    assert_eq!(
        count_for_game(&storage, "component_backups", shared_game.id().as_str()),
        0
    );
    assert_eq!(
        count_for_artifact(&storage, "artifact:shared-source-local"),
        0
    );
    let artifact_revision_after_collection = storage
        .library_artifact_revision()
        .expect("artifact revision after collection");
    assert_ne!(
        artifact_revision_after_collection,
        artifact_revision_before_collection
    );
    let generation_after_collection = storage.catalog_generation();
    let authority_after_collection = storage
        .catalog_readiness(shared_game.id())
        .expect("authority after collection");
    storage
        .collect_absent_installation(
            &shared_game,
            AuthorityCas::new(authority_after_collection.authority_epoch()),
        )
        .expect("repeat after actual cleanup");
    assert_eq!(storage.catalog_generation(), generation_after_collection);
    assert_eq!(
        storage
            .library_artifact_revision()
            .expect("artifact revision remains stable"),
        artifact_revision_after_collection
    );
    assert!(
        storage
            .find_active_game(shared_game.id())
            .expect("absent lookup")
            .is_none()
    );

    let readiness = storage
        .catalog_readiness(shared_game.id())
        .expect("reactivation authority");
    storage
        .save_complete_scan_write_unit(CompleteScanWriteUnit {
            game: &shared_game,
            components: &[],
            artifacts: &[],
            observations: &[],
            authority: AuthorityCas::new(readiness.authority_epoch()),
            prune_empty_operations: false,
        })
        .expect("complete scan reactivates and keeps shared owner");
    assert!(
        storage
            .find_active_game(shared_game.id())
            .expect("active lookup")
            .is_some()
    );
    assert_eq!(
        storage
            .get_installed_addon(shared_game.id())
            .expect("shared host owner after reactivation")
            .and_then(|addon| addon.registered_exe_path().cloned()),
        Some(PathRef::new("C:/Games/Shared/Game.exe").expect("registered executable"))
    );
}

#[test]
fn absence_projects_external_renodx_and_luma_receipts_across_restart_and_reactivation() {
    let directory = tempfile::tempdir().expect("temp directory");
    let database = directory.path().join("catalog.db");
    let renodx_game = sample_game("manual:renodx-external", "C:/Games/Reno", Launcher::Manual);
    let luma_game = sample_game("manual:luma-external", "C:/Games/Luma", Launcher::Manual);

    {
        let storage = SqliteStorage::open(&database).expect("storage");
        storage
            .upsert_games(&[renodx_game.clone(), luma_game.clone()])
            .expect("game registrations");

        let renodx = InstalledAddon::new(
            renodx_game.id().clone(),
            AddonKind::RenoDx,
            PathRef::new("C:/Users/Mods/Reno/renodx.addon64").expect("external payload"),
        )
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_created_file(PathRef::new("C:/Games/Reno/dxgi.dll").expect("local host"))
        .with_backed_up_file(PathRef::new("C:/Games/Reno/dxgi.dll.backup").expect("local backup"))
        .with_backed_up_file(
            PathRef::new("C:/Users/Mods/Reno/old-addon.backup").expect("external backup"),
        )
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example.invalid/renodx.addon64",
            None,
            "payload-digest",
        ))
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example.invalid/reshade.zip",
            None,
            "host-digest",
        ))
        .with_renodx_config_receipt(Some(RenoDxConfigReceipt::new(
            PathRef::new("C:/Games/Reno/ReShade.ini").expect("local config"),
            RenoDxSetPathBaseline::Absent,
            false,
            RenoDxSetPathValue::One,
        )))
        .expect("RenoDX config receipt");

        let prior = Sha256Hash::new("a".repeat(64)).expect("prior hash");
        let installed = Sha256Hash::new("b".repeat(64)).expect("installed hash");
        let luma = InstalledAddon::new(
            luma_game.id().clone(),
            AddonKind::Luma,
            PathRef::new("C:/Users/Mods/Luma/Luma-Game.addon").expect("external payload"),
        )
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_created_file(PathRef::new("C:/Games/Luma/dxgi.dll").expect("local host"))
        .with_created_file(
            PathRef::new("C:/Games/Luma/reshade-shaders/User.fx").expect("local effect"),
        )
        .with_backed_up_file(PathRef::new("C:/Games/Luma/dxgi.dll.backup").expect("local backup"))
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example.invalid/luma.zip",
            None,
            "luma-digest",
        ))
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example.invalid/reshade.zip",
            None,
            "host-digest",
        ))
        .with_tracked_source(
            TrackedSource::new(
                TrackedSourceRole::DgVoodooWrapper,
                "https://github.com/dege-diosg/dgVoodoo2/releases/download/v2.87.3/dgVoodoo2_87_3.zip",
                Some("\"dg-etag-1\"".to_owned()),
                "dgvoodoo-archive-digest",
            )
            .with_last_modified(Some("Mon, 06 Jul 2026 10:00:00 GMT".to_owned()))
            .with_channel("dgvoodoo2@2.87.3"),
        )
        .try_with_managed_files(vec![
            ManagedAddonFile::owned(
                PathRef::new("D:/Luma/DLSS.dll").expect("external managed binding"),
                ManagedFileBaseline::Present {
                    sha256: prior.clone(),
                },
                installed.clone(),
            ),
            ManagedAddonFile::owned(
                PathRef::new("C:/Games/Luma/D3D9.dll").expect("local D3D9 wrapper"),
                ManagedFileBaseline::Present { sha256: prior },
                installed,
            ),
        ])
        .expect("managed bindings");

        storage
            .upsert_installed_addon(&renodx)
            .expect("RenoDX receipt");
        storage.upsert_installed_addon(&luma).expect("Luma receipt");
        for game in [&renodx_game, &luma_game] {
            storage
                .mark_installation_absent(
                    game,
                    AuthorityCas::new(
                        storage
                            .catalog_readiness(game.id())
                            .expect("initial authority")
                            .authority_epoch(),
                    ),
                )
                .expect("mark absent");
            let readiness = storage
                .catalog_readiness(game.id())
                .expect("absence authority");
            storage
                .collect_absent_installation(game, AuthorityCas::new(readiness.authority_epoch()))
                .expect("project external receipt closure");
        }
    }

    let storage = SqliteStorage::open(&database).expect("restart storage");
    let renodx = storage
        .get_installed_addon(renodx_game.id())
        .expect("restart RenoDX lookup")
        .expect("external RenoDX owner retained");
    assert_eq!(
        renodx.addon_file().as_str(),
        "C:/Users/Mods/Reno/renodx.addon64"
    );
    assert_eq!(renodx.created_files().len(), 1);
    assert_eq!(
        renodx.created_files()[0].as_str(),
        "C:/Users/Mods/Reno/renodx.addon64"
    );
    assert_eq!(renodx.backed_up_files().len(), 1);
    assert_eq!(
        renodx.backed_up_files()[0].as_str(),
        "C:/Users/Mods/Reno/old-addon.backup"
    );
    assert!(
        renodx
            .tracked_sources()
            .iter()
            .all(|source| { source.role() != TrackedSourceRole::HostBinary })
    );
    assert!(renodx.renodx_config_receipt().is_none());

    let luma = storage
        .get_installed_addon(luma_game.id())
        .expect("restart Luma lookup")
        .expect("external Luma owner retained");
    assert_eq!(luma.created_files().len(), 1);
    assert_eq!(
        luma.created_files()[0].as_str(),
        "C:/Users/Mods/Luma/Luma-Game.addon"
    );
    assert!(luma.backed_up_files().is_empty());
    assert!(
        luma.tracked_sources()
            .iter()
            .all(|source| { source.role() != TrackedSourceRole::HostBinary })
    );
    assert_eq!(luma.managed_files().len(), 1);
    assert_eq!(luma.managed_files()[0].path().as_str(), "D:/Luma/DLSS.dll");
    assert_eq!(
        luma.managed_files()[0].mode(),
        renderpilot_domain::ManagedFileMode::Owned
    );
    assert_eq!(
        luma.managed_files()[0].baseline(),
        &ManagedFileBaseline::Present {
            sha256: Sha256Hash::new("a".repeat(64)).expect("prior hash")
        }
    );
    assert_eq!(
        luma.managed_files()[0].installed_sha256(),
        &Sha256Hash::new("b".repeat(64)).expect("installed hash")
    );
    assert!(luma.tracked_sources().iter().any(|source| {
        source.role() == TrackedSourceRole::AddonPayload
            && source.url() == "https://example.invalid/luma.zip"
    }));
    let dgvoodoo_source = luma
        .tracked_sources()
        .iter()
        .find(|source| source.role() == TrackedSourceRole::DgVoodooWrapper)
        .expect("supported Luma DgVoodoo source retained");
    assert_eq!(
        dgvoodoo_source.url(),
        "https://github.com/dege-diosg/dgVoodoo2/releases/download/v2.87.3/dgVoodoo2_87_3.zip"
    );
    assert_eq!(dgvoodoo_source.digest(), "dgvoodoo-archive-digest");
    assert_eq!(dgvoodoo_source.channel(), Some("dgvoodoo2@2.87.3"));

    for game in [&renodx_game, &luma_game] {
        let readiness = storage
            .catalog_readiness(game.id())
            .expect("reactivation authority");
        storage
            .save_complete_scan_write_unit(CompleteScanWriteUnit {
                game,
                components: &[],
                artifacts: &[],
                observations: &[],
                authority: AuthorityCas::new(readiness.authority_epoch()),
                prune_empty_operations: false,
            })
            .expect("reactivate after fresh complete scan");
        assert!(
            !storage
                .is_installation_absent(game.id())
                .expect("active again")
        );
        assert!(
            storage
                .get_installed_addon(game.id())
                .expect("retained external row")
                .is_some()
        );
    }
}

#[test]
fn mixed_location_proxy_hosts_block_collection_and_preserve_absent_rows() {
    assert_proxy_host_receipt_conflict_is_atomic(true);
}

#[test]
fn zero_recognized_proxy_hosts_block_collection_and_preserve_absent_rows() {
    assert_proxy_host_receipt_conflict_is_atomic(false);
}

fn assert_proxy_host_receipt_conflict_is_atomic(mixed_locations: bool) {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = sample_game(
        if mixed_locations {
            "manual:mixed-proxy-hosts"
        } else {
            "manual:zero-proxy-hosts"
        },
        "C:/Games/ProxyHostConflict",
        Launcher::Manual,
    );
    storage.upsert_game(&game).expect("game");

    let payload = PathRef::new("D:/Mods/Luma/Luma-Game.addon").expect("external payload");
    let mut addon = InstalledAddon::new(game.id().clone(), AddonKind::Luma, payload.clone())
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_created_file(payload)
        .with_tracked_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example.invalid/reshade.zip",
            None,
            "host-digest",
        ));
    if mixed_locations {
        addon = addon
            .with_created_file(
                PathRef::new("C:/Games/ProxyHostConflict/dxgi.dll").expect("local dxgi host"),
            )
            .with_created_file(
                PathRef::new("D:/Mods/Luma/D3D9.dll").expect("external managed host"),
            );
    }
    storage
        .upsert_installed_addon(&addon)
        .expect("external Proxy owner");
    let persisted_addon = storage
        .get_installed_addon(game.id())
        .expect("persisted owner")
        .expect("owner row");
    storage
        .replace_components_for_game(
            game.id(),
            &[duplicate_component(
                &game,
                "C:/Games/ProxyHostConflict/stale.dll",
            )],
        )
        .expect("local component");
    seed_artifact(
        &storage,
        "artifact:proxy-host-conflict",
        game.id().as_str(),
        "local_observed",
    );

    let readiness = storage
        .catalog_readiness(game.id())
        .expect("authority before absence");
    storage
        .mark_installation_absent(&game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("mark absence");
    let absent_readiness = storage
        .catalog_readiness(game.id())
        .expect("absent readiness");
    let error = storage
        .collect_absent_installation(&game, AuthorityCas::new(absent_readiness.authority_epoch()))
        .expect_err("ambiguous HostBinary receipts must preserve all rows");
    if mixed_locations {
        assert!(
            error
                .to_string()
                .contains("disagree about installation-root containment")
        );
    } else {
        assert!(
            error
                .to_string()
                .contains("no recognized durable Proxy host receipt")
        );
    }
    assert_eq!(
        storage
            .get_installed_addon(game.id())
            .expect("owner retained"),
        Some(persisted_addon)
    );
    assert_eq!(
        count_for_game(&storage, "components", game.id().as_str()),
        1
    );
    assert_eq!(
        count_for_artifact(&storage, "artifact:proxy-host-conflict"),
        1
    );
    assert!(
        storage
            .is_installation_absent(game.id())
            .expect("absence persists")
    );
    assert_eq!(
        storage
            .catalog_readiness(game.id())
            .expect("readiness preserved"),
        absent_readiness
    );
}

#[test]
fn ambiguous_external_owner_path_blocks_collection_without_losing_catalog_rows() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = sample_game(
        "manual:ambiguous-owner",
        "C:/Games/Ambiguous",
        Launcher::Manual,
    );
    storage.upsert_game(&game).expect("game");
    let addon = InstalledAddon::new(
        game.id().clone(),
        AddonKind::Luma,
        PathRef::new("D:/Mods/Luma-Game.addon").expect("external primary"),
    )
    .with_created_file(PathRef::new("D:/Mods/../escaped.dll").expect("ambiguous external receipt"));
    storage
        .upsert_installed_addon(&addon)
        .expect("owner before collection");
    let persisted_addon = storage
        .get_installed_addon(game.id())
        .expect("persisted owner before collection")
        .expect("owner row");
    storage
        .replace_components_for_game(
            game.id(),
            &[duplicate_component(&game, "C:/Games/Ambiguous/stale.dll")],
        )
        .expect("local component");
    seed_artifact(
        &storage,
        "artifact:ambiguous",
        game.id().as_str(),
        "local_observed",
    );

    let readiness = storage
        .catalog_readiness(game.id())
        .expect("authority before absence");
    storage
        .mark_installation_absent(&game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("mark absent durably");
    let absent_readiness = storage
        .catalog_readiness(game.id())
        .expect("absence readiness");
    let error = storage
        .collect_absent_installation(&game, AuthorityCas::new(absent_readiness.authority_epoch()))
        .expect_err("ambiguous ownership must preserve the receipt");
    assert!(
        error
            .to_string()
            .contains("cannot collect absent installation")
    );
    assert_eq!(
        storage
            .get_installed_addon(game.id())
            .expect("owner retained"),
        Some(persisted_addon)
    );
    assert_eq!(
        count_for_game(&storage, "components", game.id().as_str()),
        1
    );
    assert_eq!(count_for_artifact(&storage, "artifact:ambiguous"), 1);
    assert!(
        storage
            .is_installation_absent(game.id())
            .expect("absence persists")
    );
    assert_eq!(
        storage
            .catalog_readiness(game.id())
            .expect("readiness preserved"),
        absent_readiness
    );
}

#[test]
fn invalid_unicode_drive_receipt_returns_conflict_without_poisoning_storage() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let game = sample_game(
        "manual:unicode-owner",
        "C:/Games/UnicodeOwner",
        Launcher::Manual,
    );
    storage.upsert_game(&game).expect("game");
    let addon = InstalledAddon::new(
        game.id().clone(),
        AddonKind::Luma,
        PathRef::new("Cé:/Mods/Luma.addon").expect("legacy receipt text"),
    );
    storage
        .upsert_installed_addon(&addon)
        .expect("legacy owner");
    let persisted_addon = storage
        .get_installed_addon(game.id())
        .expect("persisted owner")
        .expect("owner exists");
    storage
        .replace_components_for_game(
            game.id(),
            &[duplicate_component(
                &game,
                "C:/Games/UnicodeOwner/stale.dll",
            )],
        )
        .expect("local component");
    let readiness = storage.catalog_readiness(game.id()).expect("authority");
    storage
        .mark_installation_absent(&game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("mark absence");
    let absent_readiness = storage
        .catalog_readiness(game.id())
        .expect("absent authority");
    let error = storage
        .collect_absent_installation(&game, AuthorityCas::new(absent_readiness.authority_epoch()))
        .expect_err("invalid receipt must be a recoverable collection conflict");
    assert!(error.to_string().contains("ambiguous or non-absolute path"));
    assert_eq!(
        storage
            .get_installed_addon(game.id())
            .expect("owner retained"),
        Some(persisted_addon)
    );
    assert_eq!(
        count_for_game(&storage, "components", game.id().as_str()),
        1
    );
    assert!(
        storage
            .is_installation_absent(game.id())
            .expect("absence persists")
    );
    assert_eq!(
        storage
            .catalog_readiness(game.id())
            .expect("authority retained"),
        absent_readiness
    );
    assert_eq!(
        storage.list_games().expect("database remains usable"),
        vec![game]
    );
}

fn stable_journal(path: &str) -> EngineConfigJournal {
    EngineConfigJournal {
        stable: Some(EngineConfigReceipt {
            schema_version: 1,
            path: path.to_owned(),
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

fn seed_engine_journal(storage: &SqliteStorage, game_id: &str, journal: &EngineConfigJournal) {
    let raw = serde_json::to_string(journal).expect("journal serialization");
    storage
        .connection
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO game_engine_config_journals
                (game_id, addon_kind, journal_json, created_at, updated_at)
             VALUES (?1, 'renodx', ?2, 1, 1)",
            rusqlite::params![game_id, raw],
        )
        .expect("seed standalone journal");
}

fn count_for_game(storage: &SqliteStorage, table: &str, game_id: &str) -> i64 {
    storage
        .connection
        .lock()
        .expect("connection")
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE game_id = ?1"),
            [game_id],
            |row| row.get(0),
        )
        .expect("count")
}

fn count_for_artifact(storage: &SqliteStorage, id: &str) -> i64 {
    storage
        .connection
        .lock()
        .expect("connection")
        .query_row(
            "SELECT COUNT(*) FROM library_artifacts WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .expect("artifact count")
}
