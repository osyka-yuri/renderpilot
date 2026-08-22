use std::fs;

use renderpilot_application::GameRepository;
use renderpilot_domain::{
    AddonKind, GameId, GameIdentity, GameInstallation, GameRuntime, InstalledAddon, Launcher,
    PathRef, Platform, TrackedSource, TrackedSourceRole,
};
use tempfile::{TempDir, tempdir};

use super::commit::authorize_update_commit;
use super::prepare::prepare_update_artifacts;
use super::snapshot::{UpdateSnapshot, ensure_update_snapshot_matches};
use crate::addons::reshade::types::ReshadeChannel;
use crate::{Context, ServiceError};

fn record() -> InstalledAddon {
    InstalledAddon::new(
        GameId::new("steam:1091500").expect("game id"),
        AddonKind::RenoDx,
        PathRef::new("C:/Games/Test/renodx-test.addon64").expect("add-on path"),
    )
    .with_addon_version("1")
}

fn snapshot(record: InstalledAddon, channel: Option<ReshadeChannel>) -> UpdateSnapshot {
    UpdateSnapshot {
        record,
        shared_vulkan_channel: channel,
        addon: None,
        host: None,
        host_target: None,
    }
}

struct SafetyFixture {
    _db_dir: TempDir,
    game_dir: TempDir,
    context: Context,
    game_id: GameId,
}

fn safety_fixture(suffix: &str) -> SafetyFixture {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new(format!("manual:update-safety-{suffix}")).expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "Update Safety Test", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_dir.path().to_string_lossy()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");

    SafetyFixture {
        _db_dir: db_dir,
        game_dir,
        context,
        game_id,
    }
}

async fn assert_update_barrier_rejects(
    fixture: &SafetyFixture,
    safety: crate::GameMutationSafetyPermits,
    expected: fn(&ServiceError) -> bool,
) {
    let guards = crate::mutation_boundary::enter_mutation_boundary_async(
        &fixture.context,
        &fixture.game_id,
        false,
    )
    .await
    .expect("game boundary");
    let mut commit_called = false;
    let error = authorize_update_commit(&fixture.context, guards, &safety, |_| {
        commit_called = true;
        Ok(())
    })
    .expect_err("invalid safety must reject the update commit");

    assert!(expected(&error), "unexpected error: {error:?}");
    assert!(!commit_called, "safety rejection must precede first write");
    assert!(
        fixture
            .context
            .storage()
            .pending_file_mutations_for_game(&fixture.game_id)
            .expect("pending mutations")
            .is_empty()
    );
}

#[test]
fn update_snapshot_rejects_any_install_record_drift() {
    let prepared = snapshot(record(), None);
    let current = snapshot(record().with_addon_version("2"), None);

    assert!(ensure_update_snapshot_matches(&prepared, &current).is_err());
}

#[test]
fn update_snapshot_rejects_shared_vulkan_channel_drift() {
    let prepared = snapshot(record(), Some(ReshadeChannel::Stable));
    let current = snapshot(record(), Some(ReshadeChannel::Nightly));

    assert!(ensure_update_snapshot_matches(&prepared, &current).is_err());
}

#[tokio::test]
async fn update_commit_barrier_rejects_stale_game_context_before_first_write() {
    let fixture = safety_fixture("stale");
    let authority = crate::FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&fixture.context, &fixture.game_id)
        .expect("assessment");
    let safety = authority
        .game_mutation_permits(
            fixture.game_id.clone(),
            Some(&assessment.context_token),
            None,
        )
        .expect("permits");
    fs::create_dir(fixture.game_dir.path().join("EasyAntiCheat")).expect("anti-cheat marker");

    assert_update_barrier_rejects(&fixture, safety, |error| {
        matches!(error, ServiceError::SafetyContextStale { .. })
    })
    .await;
}

#[tokio::test]
async fn update_commit_barrier_rejects_another_game_scope_before_first_write() {
    let fixture = safety_fixture("scope");
    let other = safety_fixture("other");
    fixture
        .context
        .storage()
        .upsert_game(
            &other
                .context
                .storage()
                .require_game(&other.game_id)
                .expect("other game"),
        )
        .expect("copy other game");
    let authority = crate::FileSafetyAuthority::new();
    let assessment = authority
        .issue_game_assessment(&fixture.context, &other.game_id)
        .expect("assessment");
    let safety = authority
        .game_mutation_permits(
            fixture.game_id.clone(),
            Some(&assessment.context_token),
            None,
        )
        .expect("well-formed permits");

    assert_update_barrier_rejects(&fixture, safety, |error| {
        matches!(error, ServiceError::SafetyContextScopeMismatch { .. })
    })
    .await;
}

#[tokio::test]
async fn generic_prepare_preserves_the_dlss_projection_without_network_or_writes() {
    let dlss = TrackedSource::new(
        TrackedSourceRole::DlssFix,
        "https://example.test/renodx-dlssfix.addon64",
        Some("etag".to_owned()),
        "live-digest",
    )
    .with_last_modified(Some("Mon, 01 Jan 2024 00:00:00 GMT".to_owned()))
    .with_advisory();
    let prepared = prepare_update_artifacts(
        &snapshot(record().with_tracked_source(dlss.clone()), None),
        None,
    )
    .await
    .expect("generic prepare does not fetch DLSS");

    assert_eq!(prepared.refreshed_sources, vec![dlss]);
    assert!(prepared.replacements.is_empty());
    assert!(prepared.host_install.is_none());
}
