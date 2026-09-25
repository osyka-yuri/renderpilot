use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_application::{ComponentRepository, GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, ComponentFile, ComponentId, ComponentKind, GameId, GameIdentity, GameInstallation,
    GameRuntime, InstalledAddon, Launcher, LibraryComponent, LibraryTechnology, PathRef, Platform,
    RenoDxInstallState, Swappability, TrackedSource, TrackedSourceRole,
};
use renderpilot_storage_sqlite::BeginFileMutationPreparation;
use tempfile::{TempDir, tempdir};

use super::support::{DlssSnapshot, ensure_snapshot_matches, install_ini_operation};
use super::*;
use crate::addons::records;
use crate::addons::renodx::dlss_fix::DlssFixRequest;
use crate::addons::renodx::dlss_fix_binding;
use crate::addons::reshade::scan as reshade;
use crate::file_mutation::{
    MutationScope, RetryableFileMutationV2, RetryableFileOperation, RetryableFilePlan,
    V2DiskObservation, observe,
};
use crate::game_mutation_lock;

fn seed_preparing_v2_row(
    context: &Context,
    game_id: &GameId,
    id: &str,
    feature: &str,
    root: &Path,
) -> PathBuf {
    let transaction_dir = context.file_mutation_root().join(id);
    fs::create_dir_all(&transaction_dir).expect("transaction dir");
    let manifest = serde_json::json!({
        "format_version": 2,
        "roots": [root.to_string_lossy().into_owned()],
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
            feature: feature.to_owned(),
            subject_id: Some(game_id.as_str().to_owned()),
            initial_manifest_json: manifest,
        })
        .expect("preparing v2 row");
    transaction_dir
}

fn snapshot(addon_dir: &Path, game_root: &Path) -> DlssSnapshot {
    let record = InstalledAddon::new(
        GameId::new("manual:dlss-snapshot").expect("game id"),
        AddonKind::RenoDx,
        PathRef::new(
            addon_dir
                .join("renodx-game.addon64")
                .to_string_lossy()
                .into_owned(),
        )
        .expect("add-on path"),
    );
    let binding = dlss_fix_binding::resolve(&record);
    let ini_path = game_root.join(reshade::RESHADE_INI_FILE_NAME);
    DlssSnapshot {
        record,
        binding,
        request: None,
        ini_observation: observe(&ini_path),
        ini_path,
    }
}

struct PartialDlssFixture {
    _db_root: TempDir,
    _game_root: TempDir,
    context: Context,
    game_id: GameId,
    target: PathBuf,
    before_record: InstalledAddon,
}

fn partial_dlss_fixture(suffix: &str) -> PartialDlssFixture {
    let db_root = tempdir().expect("db root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new(format!("manual:dlss-safety-{suffix}")).expect("game id");
    let addon = game_root.path().join("renodx-game.addon64");
    let target = game_root.path().join("renodx-dlssfix.addon64");
    fs::write(&addon, b"main addon").expect("main addon");
    fs::write(&target, b"existing companion").expect("companion");

    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "DLSS Safety Test", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.path().to_string_lossy()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");
    let component = |suffix: &str, technology: LibraryTechnology, file_name: &str| {
        let path = game_root.path().join(file_name);
        fs::write(&path, b"component").expect("component file");
        LibraryComponent::new(
            ComponentId::new(format!("component:dlss-safety-{suffix}")).expect("component id"),
            game_id.clone(),
            ComponentKind::NativeLibrary,
            technology,
            Swappability::Swappable,
        )
        .with_file(ComponentFile::new(
            PathRef::new(path.to_string_lossy()).expect("component path"),
        ))
    };
    context
        .storage()
        .replace_components_for_game(
            &game_id,
            &[
                component(
                    "sr",
                    LibraryTechnology::DlssSuperResolution,
                    "nvngx_dlss.dll",
                ),
                component(
                    "fg",
                    LibraryTechnology::DlssFrameGeneration,
                    "nvngx_dlssg.dll",
                ),
                component(
                    "streamline",
                    LibraryTechnology::NvidiaStreamline,
                    "sl.interposer.dll",
                ),
            ],
        )
        .expect("components");
    let record = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(addon.to_string_lossy()).expect("add-on path"),
    )
    .with_tracked_source(TrackedSource::new(
        TrackedSourceRole::DlssFix,
        "https://example.test/dlss",
        None,
        "existing-source",
    ));
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");
    let before_record = records::record_of_kind(&context, &game_id, AddonKind::RenoDx)
        .expect("record query")
        .expect("record remains");

    PartialDlssFixture {
        _db_root: db_root,
        _game_root: game_root,
        context,
        game_id,
        target,
        before_record,
    }
}

fn assert_no_partial_dlss_writes(fixture: &PartialDlssFixture, error: &ServiceError) {
    assert_eq!(
        fs::read(&fixture.target).expect("target bytes"),
        b"existing companion"
    );
    assert_eq!(
        records::record_of_kind(&fixture.context, &fixture.game_id, AddonKind::RenoDx)
            .expect("record query")
            .expect("record remains"),
        fixture.before_record,
        "safety rejection must precede projection persistence"
    );
    assert!(
        fixture
            .context
            .storage()
            .pending_file_mutations_for_game(&fixture.game_id)
            .expect("pending rows")
            .is_empty(),
        "safety rejection must precede durable mutation preparation"
    );
    assert!(matches!(
        error,
        ServiceError::SafetyContextMissing { .. }
            | ServiceError::SafetyContextStale { .. }
            | ServiceError::SafetyContextScopeMismatch { .. }
    ));
}

#[tokio::test]
async fn install_rejects_missing_game_safety_before_partial_projection_write() {
    let fixture = partial_dlss_fixture("install-missing");
    let error = crate::FileSafetyAuthority::new()
        .game_permit(fixture.game_id.clone(), None)
        .expect_err("missing game safety context must reject install");
    assert!(matches!(&error, ServiceError::SafetyContextMissing { .. }));
    assert_no_partial_dlss_writes(&fixture, &error);
}

#[tokio::test]
async fn install_rejects_stale_game_safety_before_partial_projection_write() {
    let fixture = partial_dlss_fixture("install-stale");
    let authority = crate::FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&fixture.context, &fixture.game_id)
        .expect("fresh game safety assessment");
    let safety = authority
        .game_permit(fixture.game_id.clone(), Some(&assessment.context_token))
        .expect("permit");
    fs::write(
        fixture
            .target
            .parent()
            .expect("game root")
            .join("EasyAntiCheat"),
        b"detected marker",
    )
    .expect("marker");

    let error = install_dlss_fix(&fixture.context, &fixture.game_id, safety, None)
        .await
        .expect_err("stale game safety context must reject install");
    assert!(matches!(&error, ServiceError::SafetyContextStale { .. }));
    assert_no_partial_dlss_writes(&fixture, &error);
}

#[tokio::test]
async fn install_rejects_scope_mismatched_game_safety_before_partial_projection_write() {
    let fixture = partial_dlss_fixture("install-scope");
    let other_root = tempdir().expect("other game root");
    let other_id = GameId::new("manual:dlss-install-safety-other").expect("other id");
    let other_game = GameInstallation::new(
        GameIdentity::new(other_id, "Other DLSS Safety Test", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(other_root.path().to_string_lossy()).expect("game path"),
    );
    fixture
        .context
        .storage()
        .upsert_game(&other_game)
        .expect("other game");
    let authority = crate::FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&fixture.context, other_game.id())
        .expect("other game safety assessment");
    let safety = authority
        .game_permit(fixture.game_id.clone(), Some(&assessment.context_token))
        .expect("well-formed permit");

    let error = install_dlss_fix(&fixture.context, &fixture.game_id, safety, None)
        .await
        .expect_err("a permit for another game must reject install");
    assert!(matches!(
        &error,
        ServiceError::SafetyContextScopeMismatch { .. }
    ));
    assert_no_partial_dlss_writes(&fixture, &error);
}

#[test]
fn phase_three_revalidation_rejects_record_target_and_ini_drift() {
    let addon_dir = tempdir().expect("add-on dir");
    let game_root = tempdir().expect("game root");
    let before = snapshot(addon_dir.path(), game_root.path());

    let mut record_changed = before.clone();
    record_changed.record = record_changed.record.with_addon_version("foreign");
    assert!(ensure_snapshot_matches(&before, &record_changed).is_err());

    let mut target_changed = before.clone();
    target_changed.binding.observation = V2DiskObservation::Regular {
        digest: "foreign".to_owned(),
    };
    assert!(ensure_snapshot_matches(&before, &target_changed).is_err());

    let mut ini_changed = before.clone();
    ini_changed.ini_observation = V2DiskObservation::Regular {
        digest: "foreign".to_owned(),
    };
    assert!(ensure_snapshot_matches(&before, &ini_changed).is_err());
}

#[test]
fn install_ini_targets_the_active_game_root_not_the_addon_parent() {
    let addon_dir = tempdir().expect("split add-on dir");
    let game_root = tempdir().expect("game root");
    let active_ini = game_root.path().join(reshade::RESHADE_INI_FILE_NAME);
    fs::write(&active_ini, "[ADDON]\nDisabledAddons=Example\n").expect("active ini");
    let snapshot = snapshot(addon_dir.path(), game_root.path());
    let operations = install_ini_operation(
        &snapshot,
        &DlssFixRequest {
            dlss_path: r"C:\\Game\\nvngx_dlss.dll".to_owned(),
            streamline_path: r"C:\\Game\\sl.interposer.dll".to_owned(),
        },
    )
    .expect("ini operation");

    let [RetryableFileOperation::Write { path, .. }] = operations.as_slice() else {
        panic!("expected exactly one active INI write")
    };
    assert_eq!(path, &active_ini);
    assert!(
        !addon_dir
            .path()
            .join(reshade::RESHADE_INI_FILE_NAME)
            .exists()
    );
}

#[tokio::test]
async fn update_rejects_missing_game_safety_before_partial_projection_write() {
    let fixture = partial_dlss_fixture("missing");
    let error = crate::FileSafetyAuthority::new()
        .game_permit(fixture.game_id.clone(), None)
        .expect_err("missing game safety context must reject update");
    assert!(matches!(&error, ServiceError::SafetyContextMissing { .. }));
    assert_no_partial_dlss_writes(&fixture, &error);
}

#[tokio::test]
async fn update_rejects_stale_game_safety_before_partial_projection_write() {
    let fixture = partial_dlss_fixture("stale");
    let authority = crate::FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&fixture.context, &fixture.game_id)
        .expect("fresh game safety assessment");
    let safety = authority
        .game_permit(fixture.game_id.clone(), Some(&assessment.context_token))
        .expect("permit");
    fs::write(
        fixture
            .target
            .parent()
            .expect("game root")
            .join("EasyAntiCheat"),
        b"detected marker",
    )
    .expect("marker");

    let error = update_dlss_fix(&fixture.context, &fixture.game_id, safety, None)
        .await
        .expect_err("stale game safety context must reject update");
    assert!(matches!(&error, ServiceError::SafetyContextStale { .. }));
    assert_no_partial_dlss_writes(&fixture, &error);
}

#[tokio::test]
async fn update_rejects_scope_mismatched_game_safety_before_partial_projection_write() {
    let fixture = partial_dlss_fixture("scope");
    let other_root = tempdir().expect("other game root");
    let other_id = GameId::new("manual:dlss-safety-other").expect("other id");
    let other_game = GameInstallation::new(
        GameIdentity::new(other_id, "Other DLSS Safety Test", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(other_root.path().to_string_lossy()).expect("game path"),
    );
    fixture
        .context
        .storage()
        .upsert_game(&other_game)
        .expect("other game");
    let authority = crate::FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&fixture.context, other_game.id())
        .expect("other game safety assessment");
    let safety = authority
        .game_permit(fixture.game_id.clone(), Some(&assessment.context_token))
        .expect("well-formed permit");

    let error = update_dlss_fix(&fixture.context, &fixture.game_id, safety, None)
        .await
        .expect_err("a permit for another game must reject update");
    assert!(matches!(
        &error,
        ServiceError::SafetyContextScopeMismatch { .. }
    ));
    assert_no_partial_dlss_writes(&fixture, &error);
}

#[test]
fn retry_recovery_cleans_pending_v2_without_host_or_network_work() {
    let db_root = tempdir().expect("db root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:dlss-retry-recovery").expect("game id");
    let addon = game_root.path().join("renodx-game.addon64");
    fs::write(&addon, b"addon").expect("addon");
    let record = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(addon.to_string_lossy().into_owned()).expect("add-on path"),
    );
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");

    let guard = game_mutation_lock::try_lock(&game_id).expect("test lock");
    let target = game_root.path().join("renodx-dlssfix.addon64");
    let scope = MutationScope::new([game_root.path().to_path_buf()]).expect("scope");
    RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope,
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_INSTALL,
        Some(game_id.as_str()),
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: target.clone(),
                bytes: b"payload".to_vec(),
                expected: V2DiskObservation::Absent,
            }],
        },
    )
    .expect("prepared v2 row");
    drop(guard);

    let state = retry_dlss_fix_recovery(&context, &game_id).expect("recovery retry");
    assert!(matches!(state, RenoDxInstallState::Installed { .. }));
    assert!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .is_empty()
    );
    assert!(
        !target.exists(),
        "recovery must not apply the pending payload"
    );
}

#[test]
fn retry_recovery_rejects_an_unrelated_pending_mutation() {
    let db_root = tempdir().expect("db root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:dlss-retry-unrelated").expect("game id");
    let guard = game_mutation_lock::try_lock(&game_id).expect("test lock");
    let scope = MutationScope::new([game_root.path().to_path_buf()]).expect("scope");
    RetryableFileMutationV2::prepare(
        &context,
        &guard,
        &scope,
        renderpilot_domain::mutation_features::RENODX_UPDATE,
        Some(game_id.as_str()),
        &RetryableFilePlan {
            operations: vec![RetryableFileOperation::Write {
                path: game_root.path().join("pending-generic.addon64"),
                bytes: b"payload".to_vec(),
                expected: V2DiskObservation::Absent,
            }],
        },
    )
    .expect("prepared generic row");
    drop(guard);

    assert!(retry_dlss_fix_recovery(&context, &game_id).is_err());
    assert_eq!(
        context
            .storage()
            .pending_file_mutations_for_game(&game_id)
            .expect("pending rows")
            .len(),
        1,
        "an unrelated recovery row must remain untouched"
    );
}

#[test]
fn retry_recovery_handles_only_dlss_rows_when_unrelated_work_is_pending() {
    let db_root = tempdir().expect("db root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("manual:dlss-retry-mixed").expect("game id");
    let dlss_id = "dlss-retry-mixed-exact";
    let unrelated_id = "dlss-retry-mixed-unrelated";
    let dlss_dir = seed_preparing_v2_row(
        &context,
        &game_id,
        dlss_id,
        renderpilot_domain::mutation_features::RENODX_DLSS_FIX_UPDATE,
        game_root.path(),
    );
    let unrelated_dir = seed_preparing_v2_row(
        &context,
        &game_id,
        unrelated_id,
        renderpilot_domain::mutation_features::RENODX_UPDATE,
        game_root.path(),
    );
    let unrelated_orphan_dir = context.file_mutation_root().join("unrelated-orphan");
    fs::create_dir_all(&unrelated_orphan_dir).expect("unrelated orphan dir");

    let state = retry_dlss_fix_recovery(&context, &game_id).expect("DLSS recovery retry");
    assert!(matches!(state, RenoDxInstallState::NotInstalled));
    let rows = context
        .storage()
        .pending_file_mutations_for_game(&game_id)
        .expect("pending rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, unrelated_id);
    assert_eq!(
        rows[0].feature,
        renderpilot_domain::mutation_features::RENODX_UPDATE
    );
    assert!(!dlss_dir.exists(), "selected recovery must be cleaned");
    assert!(
        unrelated_dir.exists(),
        "unrelated recovery artifacts must remain untouched"
    );
    assert!(
        unrelated_orphan_dir.exists(),
        "feature-scoped recovery must not run the global orphan sweep"
    );
}
