//! Sentinel finish semantics and set-diff apply regressions.

use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, GameId, GameIdentity, GameInstallation, GameRuntime, InstalledAddon, Launcher,
    ManagedFileMode, PathRef, Platform, TrackedSource, TrackedSourceRole,
};
use tempfile::tempdir;

use super::apply::apply_set_diff_with_mutation;
use super::dgvoodoo;
use super::host::{self, PreparedHostUpdate};
use super::prepare::PreparedFullUpdate;
use super::rollback::UpdateFailure;
use super::test_fixtures::{
    mark_gshade_custom_build, path_ref, payload, payload_file, resolved_target,
};
use super::update;
use crate::Context;
use crate::ServiceError;
use crate::addons::durable;
use crate::addons::engine;
use crate::addons::luma::errors as luma_errors;
use crate::addons::records;
use crate::file_mutation::{DurableFileTransaction, MutationScope};

/// Sentinel-only finish contract used by pure marker tests. Production update
/// routes through [`durable::finish_sentinel_mutation`] with a real mutation.
fn finish_local_sentinel(
    sentinel: engine::OperationSentinel,
    result: Result<(), UpdateFailure>,
) -> Result<(), ServiceError> {
    match result {
        Ok(()) => {
            sentinel.finish_committed();
            Ok(())
        }
        Err(failure) => {
            if failure.rollback_complete {
                sentinel.finish_rolled_back();
            }
            Err(failure.error)
        }
    }
}

fn prepare_update_mutation(
    context: &Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_dir: &Path,
    paths: impl IntoIterator<Item = PathBuf>,
) -> DurableFileTransaction {
    let scope = MutationScope::single(game_dir).expect("scope");
    DurableFileTransaction::prepare(
        context,
        guard,
        &scope,
        crate::addons::mutation_features::LUMA_UPDATE,
        Some(guard.game_id().as_str()),
        paths,
    )
    .expect("prepare mutation")
}

fn seed_safety_game(context: &Context, game_id: &GameId, game_root: &Path) {
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "Luma Safety Test", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.to_string_lossy()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");
}

fn game_safety(context: &Context, game_id: &GameId) -> crate::GameSafetyPermit {
    let authority = crate::FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(context, game_id)
        .expect("assessment");
    authority
        .game_permit(game_id.clone(), Some(&assessment.context_token))
        .expect("permit")
}

#[test]
fn update_safety_boundary_rejects_missing_stale_and_scope_mismatched_permits_before_writes() {
    let db_root = tempdir().expect("db root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:luma-update-safety").expect("game id");
    seed_safety_game(&context, &game_id, game_root.path());
    let target = game_root.path().join("Luma-Game.addon");

    let authority = crate::FileSafetyAuthority::new();
    let missing = authority
        .game_permit(game_id.clone(), None)
        .expect_err("missing permit must reject before update writes");
    assert!(matches!(missing, ServiceError::SafetyContextMissing { .. }));
    assert!(!target.exists());
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );
    let assessment = authority
        .issue_game_assessment(&context, &game_id)
        .expect("assessment");
    let permit = authority
        .game_permit(game_id.clone(), Some(&assessment.context_token))
        .expect("permit");
    fs::write(game_root.path().join("EasyAntiCheat"), b"detected marker").expect("marker");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("lock");
    let stale = authority
        .authorize_game_commit(
            &context,
            crate::addons::mutation_features::LUMA_UPDATE,
            &guard,
            &permit,
            || -> Result<(), ServiceError> { panic!("stale permit entered commit") },
        )
        .expect_err("stale permit must reject before update writes");
    assert!(matches!(stale, ServiceError::SafetyContextStale { .. }));
    assert!(!target.exists());
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );
    drop(guard);

    let other_root = tempdir().expect("other game root");
    let other_id = GameId::new("manual:luma-update-other-safety").expect("other game id");
    seed_safety_game(&context, &other_id, other_root.path());
    let other_assessment = authority
        .issue_game_assessment(&context, &other_id)
        .expect("other assessment");
    let mismatched = authority
        .game_permit(game_id.clone(), Some(&other_assessment.context_token))
        .expect("well-formed permit");
    let guard = crate::game_mutation_lock::try_lock(&game_id).expect("lock");
    let scope = authority
        .authorize_game_commit(
            &context,
            crate::addons::mutation_features::LUMA_UPDATE,
            &guard,
            &mismatched,
            || -> Result<(), ServiceError> { panic!("mismatched permit entered commit") },
        )
        .expect_err("scope-mismatched permit must reject before update writes");
    assert!(matches!(
        scope,
        ServiceError::SafetyContextScopeMismatch { .. }
    ));
    assert!(!target.exists());
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );
}

#[test]
fn rolled_back_update_keeps_a_marker_that_predated_the_attempt() {
    let dir = tempdir().expect("tempdir");
    engine::write_sentinel(&engine::sentinel_path(dir.path(), AddonKind::Luma))
        .expect("seed sentinel");
    let sentinel = engine::OperationSentinel::begin(dir.path(), AddonKind::Luma)
        .expect("transaction sentinel");

    let result = finish_local_sentinel(
        sentinel,
        Err(UpdateFailure {
            error: luma_errors::invalid("boom".to_owned()),
            rollback_complete: true,
        }),
    );

    assert!(result.is_err());
    assert!(engine::is_install_torn(dir.path(), AddonKind::Luma));
}

#[test]
fn fully_rolled_back_update_clears_a_marker_created_for_the_attempt() {
    let dir = tempdir().expect("tempdir");
    let sentinel = engine::OperationSentinel::begin(dir.path(), AddonKind::Luma)
        .expect("transaction sentinel");

    let result = finish_local_sentinel(
        sentinel,
        Err(UpdateFailure {
            error: luma_errors::invalid("boom".to_owned()),
            rollback_complete: true,
        }),
    );

    assert!(result.is_err());
    assert!(!engine::is_install_torn(dir.path(), AddonKind::Luma));
}

#[test]
fn incomplete_rollback_keeps_the_transaction_marker() {
    let dir = tempdir().expect("tempdir");
    let sentinel = engine::OperationSentinel::begin(dir.path(), AddonKind::Luma)
        .expect("transaction sentinel");

    let result = finish_local_sentinel(
        sentinel,
        Err(UpdateFailure {
            error: luma_errors::invalid("boom".to_owned()),
            rollback_complete: false,
        }),
    );

    assert!(result.is_err());
    assert!(engine::is_install_torn(dir.path(), AddonKind::Luma));
}

#[test]
fn committed_update_clears_a_marker_that_predated_the_attempt() {
    let dir = tempdir().expect("tempdir");
    engine::write_sentinel(&engine::sentinel_path(dir.path(), AddonKind::Luma))
        .expect("seed sentinel");
    let sentinel = engine::OperationSentinel::begin(dir.path(), AddonKind::Luma)
        .expect("transaction sentinel");

    finish_local_sentinel(sentinel, Ok(())).expect("commit");

    assert!(!engine::is_install_torn(dir.path(), AddonKind::Luma));
}

#[tokio::test]
async fn prepare_failure_never_opens_the_update_sentinel() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403652").expect("id");
    let addon_path = game_dir.path().join("Luma-Game.addon");
    std::fs::write(&addon_path, b"old-addon").expect("write addon");
    seed_safety_game(&context, &game_id, game_dir.path());
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, path_ref(&addon_path));
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("persist record");

    let manifest = crate::addons::luma::test_support::manifest(Vec::new());
    let reshade_sources = crate::addons::luma::test_support::reshade_sources();
    let result = update(super::UpdateRequest {
        context: &context,
        manifest: &manifest,
        reshade_sources: &reshade_sources,
        game_id: &game_id,
        force_full: false,
        safety: game_safety(&context, &game_id),
        progress: None,
    })
    .await;

    result.expect_err("prepare must fail before a live target or payload can be resolved");
    assert!(
        !engine::is_install_torn(game_dir.path(), AddonKind::Luma),
        "prepare performs no disk mutation and must not create a marker"
    );
}

#[test]
fn persistence_failure_rolls_back_every_set_diff_phase_and_clears_a_new_marker() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403653").expect("id");
    let addon_path = game_dir.path().join("Luma-Game.addon");
    let changed_path = game_dir.path().join("Luma/Global/Changed.hlsl");
    let removed_path = game_dir.path().join("Luma/Global/Removed.hlsl");
    let added_path = game_dir.path().join("Luma/Global/Added.hlsl");
    std::fs::create_dir_all(changed_path.parent().expect("parent")).expect("payload dir");
    std::fs::write(&addon_path, b"old-addon").expect("addon");
    std::fs::write(&changed_path, b"old-changed").expect("changed");
    std::fs::write(&removed_path, b"old-removed").expect("removed");
    std::fs::write(&added_path, b"user-owned-addition-slot").expect("addition slot");

    let record = InstalledAddon::from_parts(
        game_id.clone(),
        AddonKind::Luma,
        path_ref(&addon_path),
        None,
        vec![
            path_ref(&addon_path),
            path_ref(&changed_path),
            path_ref(&removed_path),
        ],
        Vec::new(),
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/old.zip",
            None,
            "old-digest",
        )],
    )
    .expect("record");
    let foreign_path = game_dir.path().join("RenoDX.addon");
    let foreign = InstalledAddon::new(game_id.clone(), AddonKind::RenoDx, path_ref(&foreign_path));
    context
        .storage()
        .upsert_installed_addon(&foreign)
        .expect("persist conflicting record");

    let prepared = PreparedFullUpdate {
        target: resolved_target(game_dir.path()),
        payload: payload(vec![
            payload_file("Luma-Game.addon", b"new-addon"),
            payload_file("Luma/Global/Changed.hlsl", b"new-changed"),
            payload_file("Luma/Global/Added.hlsl", b"luma-added"),
        ]),
        host: PreparedHostUpdate::unchanged(&record),
        dgvoodoo: dgvoodoo::PreparedDgVoodooUpdate::unchanged(&record),
        dependency_paths: Vec::new(),
    };
    let guard = crate::game_mutation_lock::blocking_lock(&game_id);
    let mutation = prepare_update_mutation(
        &context,
        &guard,
        game_dir.path(),
        [
            addon_path.clone(),
            changed_path.clone(),
            removed_path.clone(),
            added_path.clone(),
            engine::sentinel_path(game_dir.path(), AddonKind::Luma),
        ],
    );
    let sentinel = engine::OperationSentinel::begin(game_dir.path(), AddonKind::Luma)
        .expect("transaction sentinel");
    let failure = apply_set_diff_with_mutation(
        &context,
        &record,
        game_dir.path(),
        prepared,
        None,
        mutation.id(),
    )
    .expect_err("foreign row forces persistence failure");
    assert!(failure.rollback_complete);

    let rollback_complete = failure.rollback_complete;
    assert!(
        durable::finish_sentinel_mutation(
            &context,
            sentinel,
            mutation,
            Err(failure.error),
            rollback_complete,
            "Luma update",
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&addon_path).expect("addon"), b"old-addon");
    assert_eq!(
        std::fs::read(&changed_path).expect("changed"),
        b"old-changed"
    );
    assert_eq!(
        std::fs::read(&removed_path).expect("removed"),
        b"old-removed"
    );
    assert_eq!(
        std::fs::read(&added_path).expect("addition slot"),
        b"user-owned-addition-slot"
    );
    assert!(!game_dir.path().join("Luma/Global/Added.hlsl.bak").exists());
    assert!(!engine::is_install_torn(game_dir.path(), AddonKind::Luma));
}

#[tokio::test]
async fn apply_set_diff_records_a_shadowed_dlss_as_a_managed_binding() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    mark_gshade_custom_build(game_dir.path());

    let game_id = GameId::new(format!(
        "manual:luma-set-diff-dlss-{}",
        ulid::Ulid::generate()
    ))
    .expect("id");
    let addon_path = game_dir.path().join("Luma-Game.addon");
    std::fs::write(&addon_path, b"old-addon").expect("write addon");
    let record = InstalledAddon::from_parts(
        game_id.clone(),
        AddonKind::Luma,
        path_ref(&addon_path),
        None,
        vec![path_ref(&addon_path)],
        Vec::new(),
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/old.zip",
            None,
            "old-digest",
        )],
    )
    .expect("record");

    let game_owned_dlss = game_dir.path().join("nvngx_dlss.dll");
    let original_dlss = crate::addons::luma::test_support::build_nvidia_dlss_pe([2, 5, 0, 0]);
    let bundled_dlss = crate::addons::luma::test_support::build_nvidia_dlss_pe([3, 7, 0, 0]);
    std::fs::write(&game_owned_dlss, &original_dlss).expect("write game dlss");

    let fresh = payload(vec![
        payload_file("Luma-Game.addon", b"new-addon"),
        payload_file("nvngx_dlss.dll", &bundled_dlss),
    ]);
    let target = resolved_target(game_dir.path());
    let manifest = crate::addons::luma::test_support::manifest(Vec::new());
    let host = host::prepare_host_update_if_needed(
        &manifest,
        &crate::addons::luma::test_support::reshade_sources(),
        &target,
        &record,
        None,
    )
    .await
    .expect("prepare host");
    let dgvoodoo = dgvoodoo::prepare_if_needed(&target, &record, None, false)
        .await
        .expect("prepare dependency");

    let guard = crate::game_mutation_lock::lock(&game_id).await;
    let mutation = prepare_update_mutation(
        &context,
        &guard,
        game_dir.path(),
        [
            addon_path.clone(),
            game_owned_dlss.clone(),
            game_dir.path().join("nvngx_dlss.dll.bak"),
            engine::sentinel_path(game_dir.path(), AddonKind::Luma),
        ],
    );
    apply_set_diff_with_mutation(
        &context,
        &record,
        game_dir.path(),
        PreparedFullUpdate {
            target,
            payload: fresh,
            host,
            dgvoodoo,
            dependency_paths: Vec::new(),
        },
        None,
        mutation.id(),
    )
    .expect("update applies");
    mutation
        .cleanup_committed(context.storage())
        .expect("cleanup committed mutation");

    assert_eq!(std::fs::read(&game_owned_dlss).unwrap(), bundled_dlss);
    let bak = game_dir.path().join("nvngx_dlss.dll.bak");
    assert_eq!(std::fs::read(&bak).unwrap(), original_dlss);

    let persisted = records::record_of_kind(&context, record.game_id(), AddonKind::Luma)
        .expect("get")
        .expect("present");
    assert!(
        persisted.backed_up_files().iter().all(|path| {
            !Path::new(path.as_str())
                .file_name()
                .is_some_and(|name| name == "nvngx_dlss.dll")
        }),
        "DLSS must not leak back into the generic backup list"
    );
    let managed = persisted.managed_files().first().expect("managed DLSS");
    assert_eq!(managed.mode(), ManagedFileMode::Owned);
    assert_eq!(Path::new(managed.path().as_str()), game_owned_dlss);
}
