use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_application::GameRepository;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};
use renderpilot_storage_sqlite::{
    BeginFileMutationPreparation, CatalogReadiness, GameMutationCommit, InstalledAddonMutation,
};

use super::manifest::{
    FileBeforeSnapshot, FileMutationManifest, MANIFEST_FORMAT_VERSION, serialize_manifest,
};
use super::*;
use crate::Context;
use crate::ServiceError;

fn scope(root: &Path) -> MutationScope {
    MutationScope::single(root).expect("scope")
}

fn assert_no_v2_staged_residue(root: &Path) {
    let staged = fs::read_dir(root)
        .expect("read game root")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".renderpilot-v2-") && name.ends_with(".staged"))
        .collect::<Vec<_>>();
    assert!(staged.is_empty(), "staged residue: {staged:?}");
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

fn commit_empty_mutation(
    context: &Context,
    game_id: &GameId,
    mutation_id: &str,
) -> Result<(), ServiceError> {
    context
        .storage()
        .commit_game_mutation(GameMutationCommit {
            game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: InstalledAddonMutation::Keep,
            mutation_id: Some(mutation_id),
        })
        .map_err(ServiceError::from)
}

fn prepare_raw_v2_row(
    context: &Context,
    game_id: &GameId,
    id: &str,
    transaction_dir: &Path,
    scope_root: &Path,
) {
    let manifest = serde_json::json!({
        "format_version": 2,
        "roots": [scope_root.to_string_lossy().into_owned()],
        "transaction_dir": transaction_dir.to_string_lossy().into_owned(),
        "operations": [],
        "snapshots": [],
    })
    .to_string();
    context
        .storage()
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: id.to_owned(),
            game_id: game_id.clone(),
            feature: renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE.to_owned(),
            subject_id: None,
            initial_manifest_json: manifest.clone(),
        })
        .expect("begin raw v2 row");
    context
        .storage()
        .finish_preparing_file_mutation(id, &manifest)
        .expect("finish raw v2 row");
}

fn transaction_dir_for_pending_row(context: &Context, id: &str) -> PathBuf {
    let row = context
        .storage()
        .get_pending_file_mutation(id)
        .expect("read pending row")
        .expect("pending row");
    let manifest: serde_json::Value =
        serde_json::from_str(&row.manifest_json).expect("manifest JSON");
    PathBuf::from(
        manifest["transaction_dir"]
            .as_str()
            .expect("transaction directory"),
    )
}

#[cfg(windows)]
fn unreachable_game_root() -> PathBuf {
    for drive in (b'D'..=b'Z').map(char::from) {
        let volume = PathBuf::from(format!("{drive}:\\"));
        if !volume.exists() {
            return volume.join("renderpilot-unreachable-game");
        }
    }
    panic!("the test host has no unavailable drive letter")
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
#[test]
fn v2_prepared_recovery_never_touches_a_foreign_file_created_after_crash() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-crash").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"renderpilot".to_vec(),
                expected: V2DiskObservation::Absent,
            }],
        },
    )
    .expect("prepared");
    drop(mutation); // crash before first target write
    fs::write(&target, b"foreign").expect("foreign create during downtime");

    recover_pending(&context, &guard).expect("cleanup-only recovery");
    assert_eq!(fs::read(&target).expect("foreign target"), b"foreign");
}

#[test]
fn v2_sync_failure_restores_only_its_exact_postimage() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-rollback").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    fs::write(&target, b"before").expect("seed");
    let expected = observe(&target);
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"after".to_vec(),
                expected,
            }],
        },
    )
    .expect("prepared");
    let error = mutation
        .commit_or_rollback(&context, |_| Err::<(), _>(crate::failed("database failed")))
        .expect_err("failure");
    assert!(error.to_string().contains("database failed"));
    assert_eq!(fs::read(&target).expect("restored"), b"before");
}

#[test]
fn v2_sync_rollback_preserves_a_foreign_postwrite_replacement() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-conflict").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    fs::write(&target, b"before").expect("seed");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"renderpilot".to_vec(),
                expected: observe(&target),
            }],
        },
    )
    .expect("prepared");
    let error = mutation
        .commit_or_rollback(&context, |_| {
            fs::write(&target, b"foreign").expect("external edit");
            Err::<(), _>(crate::failed("database failed"))
        })
        .expect_err("rollback conflict");
    assert!(error.to_string().contains("rollback"));
    assert_eq!(fs::read(&target).expect("foreign survives"), b"foreign");
}

#[test]
fn v2_prepared_recovery_keeps_applied_postimages_and_a_retry_converges() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-after-write").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    let mut mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"payload".to_vec(),
                expected: V2DiskObservation::Absent,
            }],
        },
    )
    .expect("prepared");
    mutation.apply().expect("first target write");
    drop(mutation); // crash after a write but before persistence/cleanup

    recover_pending(&context, &guard).expect("cleanup-only recovery");
    assert_eq!(fs::read(&target).expect("postimage remains"), b"payload");

    RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"payload".to_vec(),
                expected: observe(&target),
            }],
        },
    )
    .expect("retry prepared")
    .commit_or_rollback(&context, |mutation_id| {
        commit_empty_mutation(&context, &game_id, mutation_id)
    })
    .expect("idempotent retry");
    assert_eq!(
        fs::read(target).expect("unchanged retry payload"),
        b"payload"
    );
}

#[test]
fn v2_delete_rolls_back_only_while_the_target_stays_absent() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-delete").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    fs::write(&target, b"before-delete").expect("seed");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UNINSTALL,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Delete {
                path: target.clone(),
                expected: observe(&target),
            }],
        },
    )
    .expect("prepared");
    let error = mutation
        .commit_or_rollback(&context, |_| Err::<(), _>(crate::failed("database failed")))
        .expect_err("rollback after delete");
    assert!(error.to_string().contains("database failed"));
    assert_eq!(fs::read(target).expect("restored delete"), b"before-delete");
}

#[test]
fn v2_token_drift_aborts_before_a_write_and_preserves_the_foreign_file() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-token-drift").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    fs::write(&target, b"before").expect("seed");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"renderpilot".to_vec(),
                expected: observe(&target),
            }],
        },
    )
    .expect("prepared");
    fs::write(&target, b"foreign").expect("external change before final token");

    let error = mutation
        .commit_or_rollback(&context, |_| Ok::<(), ServiceError>(()))
        .expect_err("token drift");
    assert!(error.to_string().contains("changed before apply"));
    assert_eq!(fs::read(target).expect("foreign survives"), b"foreign");
}

#[test]
fn v2_corrupted_prepared_payload_aborts_before_target_write() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-corrupted-payload").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    fs::write(&target, b"before").expect("seed");
    let mut mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"after".to_vec(),
                expected: observe(&target),
            }],
        },
    )
    .expect("prepared");
    let mutation_id = mutation.id().to_owned();
    fs::write(
        transaction_dir_for_pending_row(&context, &mutation_id).join("0.payload"),
        b"corrupted-payload",
    )
    .expect("corrupt immutable payload");

    let error = mutation.apply().expect_err("corrupted payload must fail");
    assert!(error.to_string().contains("payload digest"));
    assert_eq!(fs::read(&target).expect("live target"), b"before");
    assert_eq!(
        context
            .storage()
            .get_pending_file_mutation(&mutation_id)
            .expect("row")
            .expect("prepared row")
            .state,
        renderpilot_storage_sqlite::PendingFileMutationState::Prepared
    );
}

#[test]
fn v2_corrupted_preimage_refuses_restore_and_retains_pending_fence() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-corrupted-preimage").expect("id");
    store_game(&context, game_id.clone(), root.path());
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    fs::write(&target, b"before").expect("seed");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"after".to_vec(),
                expected: observe(&target),
            }],
        },
    )
    .expect("prepared");
    let mutation_id = mutation.id().to_owned();
    fs::write(
        transaction_dir_for_pending_row(&context, &mutation_id).join("0.before"),
        b"corrupted-preimage",
    )
    .expect("corrupt immutable preimage");

    let error = mutation
        .commit_or_rollback(&context, |_| Err::<(), _>(crate::failed("database failed")))
        .expect_err("corrupted preimage must block restore");
    assert!(error.to_string().contains("preimage snapshot"));
    assert_eq!(fs::read(&target).expect("postimage retained"), b"after");
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
}

#[test]
fn v2_absent_write_failure_never_exposes_a_partial_live_payload() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-absent-publish-failure").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"complete-payload-that-must-never-be-partial".to_vec(),
                expected: V2DiskObservation::Absent,
            }],
        },
    )
    .expect("prepared");
    super::retryable_v2::fail_next_absent_publish_for_test(&target);

    let error = mutation
        .commit_or_rollback(&context, |_| Ok::<(), ServiceError>(()))
        .expect_err("injected publish failure");
    assert!(error.to_string().contains("publish failure"));
    assert_eq!(observe(&target), V2DiskObservation::Absent);
    assert!(!target.exists(), "the empty reservation must be cleaned");
}

#[test]
fn v2_reservation_flush_failure_removes_the_owned_empty_target_and_all_artifacts() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-reservation-flush-failure").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"complete-payload".to_vec(),
                expected: V2DiskObservation::Absent,
            }],
        },
    )
    .expect("prepared");
    super::retryable_v2::fail_next_reservation_flush_for_test(&target);

    let error = mutation
        .commit_or_rollback(&context, |_| Ok::<(), ServiceError>(()))
        .expect_err("injected reservation flush failure");
    assert!(error.to_string().contains("reservation flush failure"));
    assert_eq!(observe(&target), V2DiskObservation::Absent);
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty(),
        "failed reservation must not leave a pending transaction"
    );
    assert_no_v2_staged_residue(root.path());
}

#[test]
fn v2_reservation_drift_preserves_a_foreign_nonempty_target() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-reservation-drift").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"renderpilot-payload".to_vec(),
                expected: V2DiskObservation::Absent,
            }],
        },
    )
    .expect("prepared");
    super::retryable_v2::drift_next_absent_reservation_for_test(&target);

    let error = mutation
        .commit_or_rollback(&context, |_| Ok::<(), ServiceError>(()))
        .expect_err("injected reservation drift failure");
    assert!(error.to_string().contains("reservation drift failure"));
    assert_eq!(
        fs::read(&target).expect("foreign reservation remains"),
        b"foreign-reservation"
    );
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty(),
        "foreign drift must not leave a pending transaction"
    );
    assert_no_v2_staged_residue(root.path());
}

#[test]
fn v2_rollback_continues_after_a_restore_error_and_reverses_other_applied_operations() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-rollback-all").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let first = root.path().join("first.addon64");
    let second = root.path().join("second.addon64");
    fs::write(&first, b"first-before").expect("first seed");
    fs::write(&second, b"second-before").expect("second seed");
    let mutation = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        None,
        &RetryableFilePlan {
            operations: vec![
                RetryableFileOperation::Write {
                    path: first.clone(),
                    bytes: b"first-after".to_vec(),
                    expected: observe(&first),
                },
                RetryableFileOperation::Write {
                    path: second.clone(),
                    bytes: b"second-after".to_vec(),
                    expected: observe(&second),
                },
            ],
        },
    )
    .expect("prepared");
    super::retryable_v2::fail_next_restore_snapshot_for_test(&second);
    let error = mutation
        .commit_or_rollback(&context, |_| Err::<(), _>(crate::failed("database failed")))
        .expect_err("rollback restore error");

    assert!(error.to_string().contains("rollback"));
    assert!(error.to_string().contains("restore snapshot failure"));
    assert_eq!(
        fs::read(&first).expect("first safe reversal"),
        b"first-before"
    );
    assert_eq!(
        fs::read(&second).expect("failed restore keeps postimage"),
        b"second-after"
    );
}

#[test]
fn v2_preimage_mismatch_aborts_before_manifest_publication() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-preimage-mismatch").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    fs::write(&target, b"before").expect("seed");
    super::retryable_v2::corrupt_next_preimage_snapshot_for_test(&target);

    let result = RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope(root.path()),
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        None,
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"after".to_vec(),
                expected: observe(&target),
            }],
        },
    );
    let error = match result {
        Ok(_) => panic!("mismatched preimage must fail preparation"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("preimage snapshot"));
    assert_eq!(fs::read(&target).expect("live file untouched"), b"before");
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn legacy_prepared_dlss_features_clean_rows_without_restoring_live_files() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:legacy-dlss").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    for feature in [
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UNINSTALL,
        renderpilot_domain::mutation_features::RENODX_UPDATE,
    ] {
        let addon = root.path().join("renodx-game.addon64");
        let companion = root.path().join("renodx-dlssfix.addon64");
        let ini = root.path().join("ReShade.ini");
        fs::write(&addon, b"before-addon").expect("addon");
        fs::write(&companion, b"before-companion").expect("companion");
        fs::write(&ini, b"before-ini").expect("ini");
        let mutation = DurableFileTransaction::prepare(
            &context,
            &guard,
            &scope(root.path()),
            feature,
            None,
            [addon.clone(), companion.clone(), ini.clone()],
        )
        .expect("legacy prepared");
        drop(mutation);
        fs::write(&addon, b"foreign-addon").expect("edit");
        fs::write(&companion, b"foreign-companion").expect("edit");
        fs::write(&ini, b"foreign-ini").expect("edit");

        recover_pending(&context, &guard).expect("non-destructive legacy cleanup");
        assert_eq!(fs::read(&addon).unwrap(), b"foreign-addon");
        assert_eq!(fs::read(&companion).unwrap(), b"foreign-companion");
        assert_eq!(fs::read(&ini).unwrap(), b"foreign-ini");
    }
}

#[cfg(windows)]
#[test]
fn v2_cleanup_recovery_tolerates_an_unreachable_game_root() {
    let root = tempfile::tempdir().expect("data root");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v2-unreachable-root").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let id = "v2-unreachable-root";
    let transaction_dir = context.file_mutation_root().join(id);
    fs::create_dir_all(&transaction_dir).expect("transaction directory");
    fs::write(transaction_dir.join("artifact"), b"cleanup-only").expect("artifact");
    prepare_raw_v2_row(
        &context,
        &game_id,
        id,
        &transaction_dir,
        &unreachable_game_root(),
    );

    recover_pending(&context, &guard).expect("cleanup-only V2 recovery");
    assert!(!transaction_dir.exists());
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );
}

#[cfg(windows)]
#[test]
fn legacy_cleanup_recovery_tolerates_an_unreachable_game_root() {
    let root = tempfile::tempdir().expect("data root");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:legacy-unreachable-root").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let id = "legacy-unreachable-root";
    let transaction_dir = context.file_mutation_root().join(id);
    fs::create_dir_all(&transaction_dir).expect("transaction directory");
    let manifest = serialize_manifest(&FileMutationManifest {
        format_version: MANIFEST_FORMAT_VERSION,
        roots: vec![unreachable_game_root().to_string_lossy().into_owned()],
        transaction_dir: transaction_dir.to_string_lossy().into_owned(),
        snapshots: Vec::new(),
    })
    .expect("manifest");
    context
        .storage()
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: id.to_owned(),
            game_id: game_id.clone(),
            feature: renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE.to_owned(),
            subject_id: None,
            initial_manifest_json: manifest.clone(),
        })
        .expect("begin");
    context
        .storage()
        .finish_preparing_file_mutation(id, &manifest)
        .expect("prepare");

    recover_pending(&context, &guard).expect("cleanup-only legacy recovery");
    assert!(!transaction_dir.exists());
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn ordinary_v1_prepared_recovery_requires_a_valid_live_scope() {
    let root = tempfile::tempdir().expect("data root");
    let external = tempfile::tempdir().expect("external root");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v1-invalid-scope").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let id = "v1-invalid-scope";
    let transaction_dir = context.file_mutation_root().join(id);
    fs::create_dir_all(&transaction_dir).expect("transaction directory");
    let manifest = serialize_manifest(&FileMutationManifest {
        format_version: MANIFEST_FORMAT_VERSION,
        roots: vec![root.path().to_string_lossy().into_owned()],
        transaction_dir: transaction_dir.to_string_lossy().into_owned(),
        snapshots: vec![FileBeforeSnapshot {
            path: external
                .path()
                .join("outside.dll")
                .to_string_lossy()
                .into_owned(),
            snapshot: None,
        }],
    })
    .expect("manifest");
    context
        .storage()
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: id.to_owned(),
            game_id,
            feature: "ordinary_v1_restore".to_owned(),
            subject_id: None,
            initial_manifest_json: manifest.clone(),
        })
        .expect("begin");
    context
        .storage()
        .finish_preparing_file_mutation(id, &manifest)
        .expect("prepare");

    let error = recover_pending(&context, &guard).expect_err("invalid scope must stop restore");
    assert!(error.to_string().contains("outside authorized roots"));
    assert_eq!(
        context
            .storage()
            .get_pending_file_mutation(id)
            .expect("row")
            .expect("prepared row")
            .state,
        renderpilot_storage_sqlite::PendingFileMutationState::Prepared
    );
    assert!(transaction_dir.exists());
}

#[test]
fn legacy_dlss_near_prefix_uses_normal_v1_restore_policy() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:legacy-dlss-near-prefix").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let target = root.path().join("renodx-dlssfix.addon64");
    fs::write(&target, b"before").expect("seed");
    let mutation = DurableFileTransaction::prepare(
        &context,
        &guard,
        &scope(root.path()),
        "renodx_dlss_fix_install_extra",
        None,
        [target.clone()],
    )
    .expect("legacy prepared");
    fs::write(&target, b"after").expect("mutate");
    drop(mutation);

    recover_pending(&context, &guard).expect("ordinary v1 recovery");
    assert_eq!(fs::read(target).expect("restored"), b"before");
}

#[test]
fn unknown_manifest_version_fails_closed_before_resolution() {
    let root = tempfile::tempdir().expect("game");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:unknown-file-mutation-version").expect("id");
    store_game(&context, game_id.clone(), root.path());
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let manifest = r#"{"format_version":99,"snapshots":[]}"#;
    context
        .storage()
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: "unknown-version".to_owned(),
            game_id: game_id.clone(),
            feature: "test".to_owned(),
            subject_id: None,
            initial_manifest_json: manifest.to_owned(),
        })
        .expect("begin");
    context
        .storage()
        .finish_preparing_file_mutation("unknown-version", manifest)
        .expect("finish");

    let error = recover_pending(&context, &guard).expect_err("unsupported version must stop");
    assert!(error.to_string().contains("unsupported"));
    assert_eq!(
        context
            .storage()
            .get_pending_file_mutation("unknown-version")
            .expect("row lookup")
            .expect("row retained")
            .state,
        renderpilot_storage_sqlite::PendingFileMutationState::Prepared
    );
    assert!(matches!(
        context
            .storage()
            .catalog_readiness(&game_id)
            .expect("readiness"),
        CatalogReadiness::Invalidated { .. }
    ));
}

#[test]
fn v2_recovery_rejects_root_and_sibling_transaction_directories() {
    for (suffix, target_root) in [("root", true), ("sibling", false)] {
        let root = tempfile::tempdir().expect("game");
        let context = Context::open_at(root.path().join("catalog.db")).expect("context");
        let game_id = GameId::new(format!("manual:v2-owner-{suffix}")).expect("id");
        let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
        let victim = context.file_mutation_root().join("victim");
        fs::create_dir_all(&victim).expect("victim directory");
        let sentinel = victim.join("sentinel.payload");
        fs::write(&sentinel, b"keep").expect("sentinel");
        let declared = if target_root {
            context.file_mutation_root().to_path_buf()
        } else {
            victim.clone()
        };
        let row_id = format!("attacker-{suffix}");
        prepare_raw_v2_row(&context, &game_id, &row_id, &declared, root.path());

        let error = recover_pending(&context, &guard).expect_err("foreign directory must fail");
        assert!(error.to_string().contains("durable row id"));
        assert_eq!(fs::read(&sentinel).expect("victim retained"), b"keep");
        assert!(
            context
                .storage()
                .get_pending_file_mutation(&row_id)
                .expect("row lookup")
                .is_some(),
            "malformed row must remain for diagnosis"
        );
    }
}

#[cfg(windows)]
#[test]
fn v1_recovery_cleanup_uses_the_validated_directory_not_an_external_alias() {
    use std::os::windows::fs::symlink_dir;

    let root = tempfile::tempdir().expect("game");
    let aliases = tempfile::tempdir().expect("aliases");
    let context = Context::open_at(root.path().join("catalog.db")).expect("context");
    let game_id = GameId::new("manual:v1-cleanup-alias").expect("id");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("guard");
    let row_id = "owned-v1-directory";
    let owned = context.file_mutation_root().join(row_id);
    fs::create_dir_all(&owned).expect("owned transaction directory");
    fs::write(owned.join("snapshot"), b"owned").expect("owned artifact");
    let alias = aliases.path().join("external-alias");
    if symlink_dir(&owned, &alias).is_err() {
        // Windows may deny symlink creation outside Developer Mode. The
        // canonical-path plumbing is still covered on every platform.
        return;
    }
    let manifest = serde_json::json!({
        "format_version": 1,
        "roots": [root.path().to_string_lossy().into_owned()],
        "transaction_dir": alias.to_string_lossy().into_owned(),
        "snapshots": [],
    })
    .to_string();
    context
        .storage()
        .begin_file_mutation_preparation(&BeginFileMutationPreparation {
            id: row_id.to_owned(),
            game_id,
            feature: renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL.to_owned(),
            subject_id: None,
            initial_manifest_json: manifest.clone(),
        })
        .expect("begin v1 row");
    context
        .storage()
        .finish_preparing_file_mutation(row_id, &manifest)
        .expect("finish v1 row");

    recover_pending(&context, &guard).expect("recover through validated directory");
    assert!(!owned.exists(), "owned app-private directory is cleaned");
    assert!(
        fs::symlink_metadata(&alias)
            .expect("external alias remains")
            .file_type()
            .is_symlink(),
        "cleanup must not delete the untrusted alias path"
    );
}
