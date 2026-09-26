use std::assert_matches;
use std::fs;

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    Architecture, GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};
use tempfile::{TempDir, tempdir};

use crate::addons::reshade::types::ReshadeChannel;
use crate::{Context, ServiceError};

fn steam_fixture(suffix: &str) -> SafetyFixture {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new(format!("steam:install-safety-{suffix}")).expect("game id");
    let external_id = game_id.as_str().strip_prefix("steam:").expect("Steam id");
    let identity = GameIdentity::new(
        game_id.clone(),
        "RenoDX Compatibility Test",
        Launcher::Steam,
    )
    .expect("identity")
    .with_external_id(external_id)
    .expect("external id");
    let game = GameInstallation::new(
        identity,
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

fn unsupported_settings_manifest(game_id: &GameId) -> crate::addons::renodx::types::RenoDxManifest {
    let mut title = crate::addons::renodx::test_support::title(
        "future-config-title",
        "future-config-title",
        Architecture::X64,
        crate::addons::renodx::types::Status::Working,
        vec![crate::addons::renodx::test_support::rule(
            crate::addons::renodx::types::MatchKind::SteamAppid,
            game_id.as_str().strip_prefix("steam:").expect("Steam id"),
            100,
        )],
    );
    title.has_unsupported_settings = true;
    crate::addons::renodx::test_support::manifest(vec![title])
}

#[test]
fn addon_arch_invariant_rejects_a_bitness_mismatch() {
    assert!(super::local_file::ensure_addon_arch(Architecture::X64, Architecture::X64).is_ok());
    let error = super::local_file::ensure_addon_arch(Architecture::X86, Architecture::X64)
        .expect_err("a 32-bit add-on for a 64-bit host must be rejected");
    assert_matches!(error, ServiceError::InvalidInput(_));
}

#[test]
fn every_install_path_rejects_an_explicit_unavailable_stable_channel() {
    let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    reshade_sources.stable = None;

    let error = super::phase::ensure_requested_channel(&reshade_sources, ReshadeChannel::Stable)
        .expect_err("Stable must not silently remap to Nightly");

    assert_matches!(error, ServiceError::InvalidInput(_));
}

#[test]
fn inactive_catalog_and_file_install_snapshots_reject_unsupported_settings_without_mutation() {
    let fixture = steam_fixture("unsupported-settings-install");
    let manifest = unsupported_settings_manifest(&fixture.game_id);
    let ini = fixture.game_dir.path().join("ReShade.ini");
    fs::write(&ini, b"sentinel ini").expect("sentinel INI");

    let catalog = super::phase::resolve_catalog_install_snapshot(
        &fixture.context,
        &manifest,
        &fixture.game_id,
        ReshadeChannel::Stable,
    );
    assert!(matches!(catalog, Err(ServiceError::InvalidInput(_))));

    let file = super::phase::resolve_file_install_snapshot(
        &fixture.context,
        &manifest,
        &fixture.game_id,
        ReshadeChannel::Stable,
        Architecture::X64,
    );
    assert!(matches!(file, Err(ServiceError::InvalidInput(_))));

    assert_eq!(fs::read(&ini).expect("INI remains"), b"sentinel ini");
    assert!(
        fixture
            .context
            .storage()
            .get_installed_addon(&fixture.game_id)
            .expect("record")
            .is_none()
    );
    assert!(
        fixture
            .context
            .storage()
            .pending_file_mutations_for_game(&fixture.game_id)
            .expect("receipts")
            .is_empty()
    );
}

#[test]
fn inactive_install_snapshot_rejects_profile_and_config_drift() {
    use crate::addons::matching::MatchConfidence;
    use crate::addons::renodx::matcher::ResolvedInstall;
    use crate::addons::renodx::types::{RenoDxConfig, RenoDxConfigKey, RenoDxConfigSetting};
    use crate::addons::reshade::proxy::HostKind;

    let plan = || ResolvedInstall {
        slug: "test".to_owned(),
        addon_url: "https://example.test/test.addon64".to_owned(),
        arch: Architecture::X64,
        host_kind: HostKind::Proxy,
        proxy_dll_name: "dxgi.dll".to_owned(),
        confidence: MatchConfidence::Verified,
        generic_profile: None,
        profile_id: Some("unity".to_owned()),
        processing_path: Default::default(),
        renodx_config: Some(RenoDxConfig {
            settings: vec![RenoDxConfigSetting {
                key: RenoDxConfigKey::ForcePipelineCloning,
                value: 1,
            }],
        }),
        guidance: Vec::new(),
        launch: None,
    };
    let snapshot = |plan| super::phase::CatalogInstallSnapshot {
        plan,
        target_dir: std::path::PathBuf::from("C:/Games/Test"),
        channel: ReshadeChannel::Stable,
        writes_host: false,
        registered_exe_path: None,
    };
    let before = snapshot(plan());

    let mut changed_profile = plan();
    changed_profile.profile_id = Some("ue_extended".to_owned());
    assert!(
        super::phase::ensure_catalog_install_snapshot_matches(&before, &snapshot(changed_profile))
            .is_err()
    );

    let mut changed_config = plan();
    changed_config
        .renodx_config
        .as_mut()
        .expect("config")
        .settings[0]
        .value = 0;
    assert!(
        super::phase::ensure_catalog_install_snapshot_matches(&before, &snapshot(changed_config))
            .is_err()
    );
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
    let game_id = GameId::new(format!("manual:install-safety-{suffix}")).expect("game id");
    let game = GameInstallation::new(
        GameIdentity::new(game_id.clone(), "Safety Test Game", Launcher::Manual).expect("identity"),
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

async fn assert_install_barrier_rejects(
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
    let error = super::commit::authorize_install_commit(
        &fixture.context,
        crate::addons::mutation_features::RENODX_INSTALL,
        guards,
        &safety,
        |_| {
            commit_called = true;
            Ok(())
        },
    )
    .expect_err("invalid safety must reject the install commit");

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

#[tokio::test]
async fn install_commit_barrier_rejects_stale_game_context_before_first_write() {
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

    assert_install_barrier_rejects(&fixture, safety, |error| {
        matches!(error, ServiceError::SafetyContextStale { .. })
    })
    .await;
}

#[tokio::test]
async fn install_commit_barrier_rejects_another_game_scope_before_first_write() {
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

    assert_install_barrier_rejects(&fixture, safety, |error| {
        matches!(error, ServiceError::SafetyContextScopeMismatch { .. })
    })
    .await;
}
