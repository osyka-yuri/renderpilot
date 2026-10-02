use renderpilot_application::{ComponentRepository, GameRepository};
use renderpilot_domain::{
    ComponentId, ComponentKind, ComponentRollbackBaseline, GameId, GameIdentity, GameInstallation,
    GameRuntime, Launcher, LibraryTechnology, PathRef, Platform, RootAuthority, Swappability,
};
use renderpilot_storage_sqlite::{AuthorityCas, SqliteStorage};

use super::{CatalogScanChange, PersistScanRequest, persist_scan_result};
use crate::catalog::scan::{reconcile::CatalogInstallIndex, xiph_test_support::split_component};

#[test]
fn reactivation_publication_replaces_stale_xiph_identity_after_prior_collection_failure() {
    let temp = tempfile::tempdir().expect("temp");
    let root = temp.path().join("Game");
    std::fs::create_dir_all(&root).expect("game root");
    let game_id = GameId::new("manual:reactivated-xiph").expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "Game", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(root.to_string_lossy().replace('\\', "/")).expect("root"),
    )
    .with_root_authority(RootAuthority::UserConfirmed);
    let storage = SqliteStorage::in_memory().expect("storage");
    storage.upsert_game(&game).expect("game");

    // Model a prior install whose first metadata collection did not run.
    // Its old component ID and immutable rollback baseline overlap the
    // fresh split closure; active-install scans must still respect them.
    let candidate = split_component(&game);
    let stale_id = ComponentId::new("component:old-xiph-lineage").expect("old id");
    let stale_component = renderpilot_domain::LibraryComponent::new(
        stale_id.clone(),
        game_id.clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::XiphVorbis,
        Swappability::BundleOnly,
    )
    .with_file(candidate.files()[0].clone());
    storage
        .replace_components_for_game(&game_id, std::slice::from_ref(&stale_component))
        .expect("stale component");
    storage
        .recover_component_rollback_baseline(
            &game_id,
            &stale_id,
            &ComponentRollbackBaseline::new(vec![candidate.files()[0].clone()]),
        )
        .expect("stale baseline");

    let before_absence = storage.catalog_readiness(&game_id).expect("readiness");
    storage
        .mark_installation_absent(&game, AuthorityCas::new(before_absence.authority_epoch()))
        .expect("durable absence");
    let readiness = storage
        .catalog_readiness(&game_id)
        .expect("absent readiness");
    let catalog_index = CatalogInstallIndex::load(&storage).expect("absent index");

    let result = persist_scan_result(
        &storage,
        PersistScanRequest {
            game,
            libraries: Vec::new(),
            components: std::slice::from_ref(&candidate),
            initial_readiness: readiness.clone(),
            authority: AuthorityCas::new(readiness.authority_epoch()),
            prune_empty_operations: false,
            root_correction_recovery_bundle_path: None,
            prefetched_catalog_index: Some(&catalog_index),
            consolidation_candidates: &[],
            reactivating_from_absence: true,
        },
    )
    .expect("fresh complete reactivation scan");

    assert_eq!(result.change, CatalogScanChange::Added);
    assert!(
        !storage
            .is_installation_absent(&game_id)
            .expect("active state")
    );
    assert!(
        storage
            .find_active_game(&game_id)
            .expect("active row")
            .is_some()
    );
    assert_eq!(
        storage
            .list_components_for_game(&game_id)
            .expect("current components"),
        vec![candidate]
    );
    assert!(
        !storage
            .has_component_backups_for_game(&game_id)
            .expect("stale baseline cleared")
    );
}
