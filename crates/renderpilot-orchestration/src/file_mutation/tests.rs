use std::fs;
use std::path::Path;

use renderpilot_application::GameRepository;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};
use renderpilot_storage_sqlite::{
    BeginFileMutationPreparation, CatalogReadiness, GameMutationCommit, InstalledAddonMutation,
};

use super::manifest::{FileMutationManifest, MANIFEST_FORMAT_VERSION, serialize_manifest};
use super::*;
use crate::Context;
use crate::ServiceError;

fn scope(root: &Path) -> MutationScope {
    MutationScope::single(root).expect("scope")
}

fn store_game(context: &Context, game_id: GameId, root: &Path) {
    let game = GameInstallation::new(
        GameIdentity::new(game_id, "File mutation test", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(root.to_string_lossy().replace('\\', "/")).expect("root"),
    );
    context.storage().upsert_game(&game).expect("store game");
}

#[test]
fn prepared_recovery_restores_existing_and_removes_created_files() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:test").expect("id");
    store_game(&context, game_id.clone(), root.path());
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let existing = root.path().join("original.dll");
    let created = root.path().join("created.dll");
    fs::write(&existing, b"before").expect("seed");

    let mutation = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test",
        None,
        [existing.clone(), created.clone()],
    )
    .expect("prepare");
    assert_eq!(
        context
            .storage()
            .catalog_readiness(&game_id)
            .expect("readiness"),
        CatalogReadiness::Invalidated {
            authority_epoch: 1,
            reason: "prepared_file_mutation".to_owned(),
            mutation_token: Some(mutation.id().to_owned()),
        },
        "the prepared durable marker must invalidate before the caller's first filesystem write"
    );
    fs::write(&existing, b"after").expect("mutate");
    fs::write(&created, b"new").expect("create");
    drop(mutation);

    recover_pending(&context, &guard).expect("recover");
    assert_eq!(fs::read(existing).expect("read"), b"before");
    assert!(!created.exists());
    recover_pending(&context, &guard).expect("idempotent recovery");
}

#[test]
fn transaction_accepts_an_explicit_external_addon_root() {
    let game = tempfile::tempdir().expect("game");
    let addon = tempfile::tempdir().expect("addon");
    let context = Context::open_at(game.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:split").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let game_file = game.path().join("dxgi.dll");
    let addon_file = addon.path().join("nvngx_dlss.dll");

    let mutation = DurableFileTransaction::prepare(
        &context,
        &guard,
        &MutationScope::new([game.path().to_path_buf(), addon.path().to_path_buf()])
            .expect("scope"),
        "test",
        None,
        [game_file.clone(), addon_file.clone()],
    )
    .expect("prepare split roots");
    fs::write(&game_file, b"host").expect("host");
    fs::write(&addon_file, b"payload").expect("payload");
    mutation.rollback(context.storage()).expect("rollback");

    assert!(!game_file.exists());
    assert!(!addon_file.exists());
}

#[test]
fn pre_catalog_prepared_recovery_restores_without_creating_catalog_authority() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:pre-catalog-recovery").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let live = root.path().join("addon.dll");
    fs::write(&live, b"before").expect("seed");

    let mutation = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test",
        None,
        [live.clone()],
    )
    .expect("prepare without catalog");
    fs::write(&live, b"after").expect("mutate");
    drop(mutation);

    recover_pending(&context, &guard).expect("recover without catalog");
    assert_eq!(fs::read(live).expect("read"), b"before");
    assert!(context.storage().catalog_readiness(&game_id).is_err());
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn preparing_recovery_never_restores_partial_snapshots() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:preparing").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let id = "preparing-crash";
    let transaction_dir = context.file_mutation_root().join(id);
    fs::create_dir_all(&transaction_dir).expect("transaction dir");
    context
        .storage()
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: id.to_owned(),
            game_id: game_id.clone(),
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: serialize_manifest(&FileMutationManifest {
                format_version: MANIFEST_FORMAT_VERSION,
                roots: vec![
                    std::fs::canonicalize(root.path())
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                ],
                transaction_dir: transaction_dir.to_string_lossy().into_owned(),
                snapshots: Vec::new(),
            })
            .unwrap(),
        })
        .expect("row");

    recover_pending(&context, &guard).expect("recover");

    assert!(!transaction_dir.exists());
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn committed_recovery_only_cleans_snapshots() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:committed-cleanup").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let live = root.path().join("nvngx_dlss.dll");
    fs::write(&live, b"before").expect("seed");

    let mutation = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test",
        None,
        [live.clone()],
    )
    .expect("prepare");
    fs::write(&live, b"committed").expect("mutate");
    context
        .storage()
        .commit_game_mutation(GameMutationCommit {
            game_id: &game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: InstalledAddonMutation::Keep,
            mutation_id: Some(mutation.id()),
        })
        .expect("commit row");
    drop(mutation);

    recover_pending(&context, &guard).expect("recover");
    assert_eq!(fs::read(live).unwrap(), b"committed");
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn missing_snapshot_stops_before_row_cleanup_and_retains_prepared_invalidation() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:missing-snapshot").expect("id");
    store_game(&context, game_id.clone(), root.path());
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let live = root.path().join("nvngx_dlss.dll");
    fs::write(&live, b"before").expect("seed");

    let mutation = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test",
        None,
        [live.clone()],
    )
    .expect("prepare");
    let mutation_id = mutation.id().to_owned();
    let row = context
        .storage()
        .get_pending_file_mutation(&mutation_id)
        .expect("row")
        .expect("prepared row");
    let manifest = super::manifest::deserialize_manifest(&row).expect("manifest");
    let snapshot = manifest.snapshots[0]
        .snapshot
        .as_ref()
        .expect("before snapshot");
    fs::remove_file(snapshot).expect("remove snapshot");
    fs::write(&live, b"after").expect("mutate");
    drop(mutation);

    recover_pending(&context, &guard).expect_err("missing snapshot must stop recovery");
    assert_eq!(
        context
            .storage()
            .get_pending_file_mutation(&mutation_id)
            .expect("row")
            .expect("prepared row")
            .state,
        renderpilot_storage_sqlite::PendingFileMutationState::Prepared
    );
    assert_eq!(
        context
            .storage()
            .catalog_readiness(&game_id)
            .expect("readiness"),
        CatalogReadiness::Invalidated {
            authority_epoch: 1,
            reason: "prepared_file_mutation".to_owned(),
            mutation_token: Some(mutation_id),
        }
    );
    assert_eq!(fs::read(live).expect("live bytes"), b"after");
}

#[test]
fn commit_or_rollback_cleans_on_success_and_restores_on_failure() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:commit-or-rollback").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let live = root.path().join("payload.dll");
    fs::write(&live, b"before").expect("seed");

    let mutation = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test_ok",
        Some(game_id.as_str()),
        [live.clone()],
    )
    .expect("prepare");
    let mutation_id = mutation.id().to_owned();
    fs::write(&live, b"after").expect("mutate");
    mutation
        .commit_or_rollback(
            context.storage(),
            || {
                context
                    .storage()
                    .commit_game_mutation(GameMutationCommit {
                        game_id: &game_id,
                        component_set: None,
                        baseline_mutations: &[],
                        addon: InstalledAddonMutation::Keep,
                        mutation_id: Some(&mutation_id),
                    })
                    .map_err(ServiceError::from)?;
                Ok::<(), ServiceError>(())
            },
            |_| {},
            || {},
        )
        .expect("commit");
    assert_eq!(fs::read(&live).unwrap(), b"after");
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .unwrap()
            .is_empty()
    );

    let mutation = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test_err",
        Some(game_id.as_str()),
        [live.clone()],
    )
    .expect("prepare err path");
    fs::write(&live, b"broken").expect("mutate");
    let error = mutation
        .commit_or_rollback(
            context.storage(),
            || Err::<(), _>(crate::failed("apply failed")),
            |_| {},
            || {},
        )
        .expect_err("work failure");
    assert!(error.to_string().contains("apply failed"));
    assert_eq!(fs::read(&live).unwrap(), b"after");
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn prepare_rejects_paths_outside_scope() {
    let root = tempfile::tempdir().expect("game");
    let outside = tempfile::tempdir().expect("outside");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:scope").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let foreign = outside.path().join("foreign.dll");
    fs::write(&foreign, b"x").expect("seed");

    let error = match DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test",
        None,
        [foreign],
    ) {
        Ok(_) => panic!("outside scope should fail"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("outside authorized roots"));
}

#[test]
fn prepare_rejects_non_file_paths() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:non-file").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let nested = root.path().join("subdir");
    fs::create_dir_all(&nested).expect("dir");

    let error = match DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "test",
        None,
        [nested],
    ) {
        Ok(_) => panic!("directory should fail"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("non-file path"));
}

#[test]
fn orphan_sweep_preserves_claimed_directories_and_removes_unclaimed_ones() {
    let root = tempfile::tempdir().expect("root");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:orphan-sweep").expect("id");
    let guard = crate::game_mutation_lock::blocking_lock(&game_id);
    let claimed = context.file_mutation_root().join("claimed");
    let orphan = context.file_mutation_root().join("orphan");
    fs::create_dir_all(&claimed).expect("claimed");
    fs::create_dir_all(&orphan).expect("orphan");
    context
        .storage()
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: "claimed".to_owned(),
            game_id: GameId::new("manual:other-game").expect("other"),
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: serialize_manifest(&FileMutationManifest {
                format_version: MANIFEST_FORMAT_VERSION,
                roots: vec![
                    std::fs::canonicalize(root.path())
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                ],
                transaction_dir: claimed.to_string_lossy().into_owned(),
                snapshots: Vec::new(),
            })
            .expect("manifest"),
        })
        .expect("row");

    recover_pending(&context, &guard).expect("sweep");

    assert!(claimed.exists());
    assert!(!orphan.exists());
}
