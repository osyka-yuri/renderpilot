use std::fs;

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, GameIdentity, GameInstallation, GameRuntime, InstalledAddon, Launcher, PathRef,
    Platform, TrackedSource, TrackedSourceRole,
};
use tempfile::{TempDir, tempdir};

use super::*;
use crate::addons::renodx::use_cases::commands::switch_reshade_channel::record::replace_or_append_host_source;
use crate::addons::renodx::use_cases::reshade_update::recorded_reshade_channel;

fn record_with_sources(sources: Vec<TrackedSource>) -> InstalledAddon {
    let addon = PathRef::new(r"C:\Games\Test\renodx-test.addon64").expect("path");
    InstalledAddon::from_parts(
        GameId::new("steam:42").expect("id"),
        AddonKind::RenoDx,
        addon.clone(),
        None,
        vec![addon],
        Vec::new(),
        sources,
    )
    .expect("record")
}

fn source(role: TrackedSourceRole, url: &str, digest: &str) -> TrackedSource {
    TrackedSource::new(role, url, None, digest)
}

struct SameChannelFixture {
    _db_root: TempDir,
    game_root: TempDir,
    context: Context,
    game_id: GameId,
    before_record: InstalledAddon,
}

fn same_channel_fixture(suffix: &str) -> SameChannelFixture {
    let db_root = tempdir().expect("db root");
    let game_root = tempdir().expect("game root");
    let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new(format!("manual:channel-safety-{suffix}")).expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "Channel Safety Test", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.path().to_string_lossy()).expect("game path"),
    );
    context.storage().upsert_game(&game).expect("game");

    let addon_path = game_root.path().join("renodx-game.addon64");
    fs::write(&addon_path, b"addon").expect("add-on");
    let record = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(addon_path.to_string_lossy()).expect("add-on path"),
    )
    .with_tracked_source(source(
        TrackedSourceRole::HostBinary,
        "https://reshade.me/downloads/ReShade_Setup.exe",
        "host-digest",
    ))
    .with_reshade_channel("stable");
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("record");
    let before_record = records::record_of_kind(&context, &game_id, AddonKind::RenoDx)
        .expect("record query")
        .expect("record remains");

    SameChannelFixture {
        _db_root: db_root,
        game_root,
        context,
        game_id,
        before_record,
    }
}

async fn switch_same_channel(
    fixture: &SameChannelFixture,
    safety: crate::GameMutationSafetyPermits,
) -> Result<RenoDxInstallState, ServiceError> {
    let manifest = crate::addons::renodx::test_support::manifest(Vec::new());
    let reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    switch_reshade_channel(SwitchChannelRequest {
        context: &fixture.context,
        manifest: &manifest,
        reshade_sources: &reshade_sources,
        game_id: &fixture.game_id,
        target_channel: ReshadeChannel::Stable,
        safety,
        progress: None,
    })
    .await
}

fn assert_record_unchanged(fixture: &SameChannelFixture) {
    assert_eq!(
        records::record_of_kind(&fixture.context, &fixture.game_id, AddonKind::RenoDx)
            .expect("record query")
            .expect("record remains"),
        fixture.before_record,
        "safety rejection must precede the metadata heal"
    );
}

#[test]
fn host_source_replacement_appends_for_legacy_records_without_host_source() {
    let record = record_with_sources(vec![source(
        TrackedSourceRole::AddonPayload,
        "https://example/renodx.addon64",
        "addon-digest",
    )]);
    let host = source(
        TrackedSourceRole::HostBinary,
        "https://reshade.me/downloads/ReShade_Setup.exe",
        "host-digest",
    );

    let sources = replace_or_append_host_source(&record, host);

    assert_eq!(sources.len(), 2);
    assert_eq!(
        sources
            .iter()
            .filter(|source| source.role() == TrackedSourceRole::HostBinary)
            .count(),
        1
    );
}

#[test]
fn host_source_replacement_replaces_existing_host_source() {
    let record = record_with_sources(vec![
        source(
            TrackedSourceRole::AddonPayload,
            "https://example/renodx.addon64",
            "addon-digest",
        ),
        source(
            TrackedSourceRole::HostBinary,
            "https://old.example/ReShade.exe",
            "old-host-digest",
        ),
    ]);
    let host = source(
        TrackedSourceRole::HostBinary,
        "https://reshade.me/downloads/ReShade_Setup.exe",
        "new-host-digest",
    );

    let sources = replace_or_append_host_source(&record, host);

    assert_eq!(sources.len(), 2);
    let host = sources
        .iter()
        .find(|source| source.role() == TrackedSourceRole::HostBinary)
        .expect("host source");
    assert_eq!(host.digest(), "new-host-digest");
}

#[test]
fn proxy_switch_record_updates_top_level_channel() {
    let record = record_with_sources(vec![source(
        TrackedSourceRole::HostBinary,
        "https://reshade.me/downloads/ReShade_Setup.exe",
        "old-host-digest",
    )])
    .with_reshade_channel("stable");
    let host = source(
        TrackedSourceRole::HostBinary,
        "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip",
        "new-host-digest",
    )
    .with_channel("nightly");

    let updated = rebuild_proxy_switch_record(&record, host, None, ReshadeChannel::Nightly)
        .expect("switch record");

    assert_eq!(updated.reshade_channel(), Some("nightly"));
    assert_eq!(
        recorded_reshade_channel(&updated),
        Some(ReshadeChannel::Nightly)
    );
}

#[tokio::test]
async fn same_channel_heal_rejects_stale_safety_before_persisting_metadata() {
    let fixture = same_channel_fixture("stale");
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
    fs::create_dir(fixture.game_root.path().join("EasyAntiCheat")).expect("anti-cheat marker");

    let error = switch_same_channel(&fixture, safety)
        .await
        .expect_err("stale context must reject the metadata heal");

    assert!(matches!(error, ServiceError::SafetyContextStale { .. }));
    assert_record_unchanged(&fixture);
}

#[tokio::test]
async fn same_channel_heal_rejects_another_game_scope_before_persisting_metadata() {
    let fixture = same_channel_fixture("scope");
    let other_root = tempdir().expect("other game root");
    let other_game_id = GameId::new("manual:channel-safety-other").expect("other game id");
    let other_game = GameInstallation::new(
        GameIdentity::new(other_game_id.clone(), "Other Game", Launcher::Manual).expect("identity"),
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
        .issue_game_assessment(&fixture.context, &other_game_id)
        .expect("assessment");
    let safety = authority
        .game_mutation_permits(
            fixture.game_id.clone(),
            Some(&assessment.context_token),
            None,
        )
        .expect("well-formed permits");

    let error = switch_same_channel(&fixture, safety)
        .await
        .expect_err("another game scope must reject the metadata heal");

    assert!(matches!(
        error,
        ServiceError::SafetyContextScopeMismatch { .. }
    ));
    assert_record_unchanged(&fixture);
}

#[test]
fn every_switch_path_rejects_an_explicit_unavailable_stable_channel() {
    let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    reshade_sources.stable = None;

    let error = ensure_target_channel(&reshade_sources, ReshadeChannel::Stable)
        .expect_err("Stable must not silently remap to Nightly");

    assert!(error.to_string().contains("stable"));
}
