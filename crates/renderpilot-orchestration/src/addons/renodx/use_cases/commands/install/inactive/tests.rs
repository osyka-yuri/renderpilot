use std::assert_matches;
use std::fs;
use std::path::PathBuf;

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, Architecture, GameId, GameIdentity, GameInstallation, GameRuntime, InstalledAddon,
    InstalledAddonHostKind, Launcher, PathRef, Platform,
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
        install_root: std::path::PathBuf::from("C:/Games"),
        target_dir: std::path::PathBuf::from("C:/Games/Test"),
        channel: ReshadeChannel::Stable,
        writes_host: false,
        registered_exe_path: None,
        previous_owner: None,
        external_owner: None,
        receipt_release_paths: Vec::new(),
        receipt_release_roots: Vec::new(),
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

#[test]
fn inactive_install_snapshot_rejects_shared_host_owner_drift_after_prepare() {
    use super::phase::ExistingInstallOwner;
    use crate::addons::matching::MatchConfidence;
    use crate::addons::renodx::matcher::ResolvedInstall;
    use crate::addons::reshade::proxy::HostKind;

    let plan = ResolvedInstall {
        slug: "test".to_owned(),
        addon_url: "https://example.test/test.addon64".to_owned(),
        arch: Architecture::X64,
        host_kind: HostKind::Proxy,
        proxy_dll_name: "dxgi.dll".to_owned(),
        confidence: MatchConfidence::Verified,
        generic_profile: None,
        profile_id: None,
        processing_path: Default::default(),
        renodx_config: None,
        guidance: Vec::new(),
        launch: None,
    };
    let snapshot =
        |install_root: &str, registered_exe_path: &str| super::phase::CatalogInstallSnapshot {
            plan: plan.clone(),
            install_root: std::path::PathBuf::from(install_root),
            target_dir: std::path::PathBuf::from("C:/Games/Test"),
            channel: ReshadeChannel::Stable,
            writes_host: false,
            registered_exe_path: None,
            previous_owner: Some(ExistingInstallOwner {
                host_kind: Some(InstalledAddonHostKind::SharedVulkanLayer),
                registered_exe_path: Some(std::path::PathBuf::from(registered_exe_path)),
            }),
            external_owner: None,
            receipt_release_paths: Vec::new(),
            receipt_release_roots: Vec::new(),
        };
    let before = snapshot("C:/Games", "C:/Games/Test/game.exe");
    let equivalent = snapshot("c:/games", "c:/games/test/GAME.EXE");
    let drifted = snapshot("C:/Games", "C:/Games/Test/other.exe");
    let root_drifted = snapshot("C:/Other", "C:/Games/Test/game.exe");

    assert!(super::phase::ensure_catalog_install_snapshot_matches(&before, &equivalent).is_ok());
    assert!(super::phase::ensure_catalog_install_snapshot_matches(&before, &drifted).is_err());
    assert!(super::phase::ensure_catalog_install_snapshot_matches(&before, &root_drifted).is_err());
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
async fn retained_shared_binding_owner_requires_shared_permit_before_proxy_reinstall_writes() {
    use crate::addons::renodx::install::PreparedInstall;
    use crate::addons::renodx::types::RenoDxProcessingPath;
    use crate::addons::reshade::proxy::HostKind;

    let fixture = safety_fixture("shared-owner-proxy-noop");
    let addon_path = fixture.game_dir.path().join("renodx-test.addon64");
    let exe_path = fixture.game_dir.path().join("game.exe");
    let target_record = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(addon_path.to_string_lossy()).expect("target add-on path"),
    )
    .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
    .with_registered_exe_path(
        PathRef::new(exe_path.to_string_lossy()).expect("registered executable path"),
    );
    fixture
        .context
        .storage()
        .upsert_installed_addon(&target_record)
        .expect("target shared owner");
    let target_record = fixture
        .context
        .storage()
        .get_installed_addon(&fixture.game_id)
        .expect("reload persisted shared owner")
        .expect("shared owner exists");

    let other_id = GameId::new("manual:shared-owner-proxy-noop-other").expect("other id");
    let other_record = InstalledAddon::new(
        other_id,
        AddonKind::RenoDx,
        PathRef::new(r"C:\Games\Other\renodx-test.addon64").expect("other add-on path"),
    )
    .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
    .with_registered_exe_path(
        PathRef::new(exe_path.to_string_lossy()).expect("other registered executable path"),
    );
    fixture
        .context
        .storage()
        .upsert_installed_addon(&other_record)
        .expect("other game's shared owner");

    let shared_change = crate::addons::renodx::use_cases::commands::shared_vulkan_layer::
        PreparedInstallChange::ReconcileApps {
            old_exe_path: Some(exe_path),
            new_exe_path: None,
            source: None,
            layer_dir: fixture.game_dir.path().join("unused-shared-layer"),
        };
    let prepared = PreparedInstall {
        game_id: fixture.game_id.clone(),
        host_kind: HostKind::Proxy,
        proxy_dll_name: "dxgi.dll".to_owned(),
        addon_file_name: "renodx-reinstall.addon64".to_owned(),
        addon_source_url: "https://example.test/renodx-reinstall.addon64".to_owned(),
        source_digest: "source-digest".to_owned(),
        source_etag: None,
        source_last_modified: None,
        addon_bytes: b"addon".to_vec(),
        reshade_dll_bytes: b"proxy-host".to_vec(),
        reshade_source_url: "https://example.test/reshade.zip".to_owned(),
        reshade_source_etag: None,
        reshade_last_modified: None,
        reshade_digest: "host-digest".to_owned(),
        reshade_channel: Some(ReshadeChannel::Stable),
        processing_path: RenoDxProcessingPath::Upgrade,
        renodx_config: None,
        ini_tweaks: crate::addons::renodx::types::renodx_ini_defaults(),
    };
    let targets = crate::addons::renodx::mutation_targets::install_targets(
        fixture.game_dir.path(),
        &prepared,
    )
    .expect("proxy install targets");
    let authority = crate::FileSafetyAuthority::new();
    let game_assessment = authority
        .issue_game_assessment(&fixture.context, &fixture.game_id)
        .expect("game assessment");
    let safety = authority
        .game_mutation_permits(
            fixture.game_id.clone(),
            Some(&game_assessment.context_token),
            None,
        )
        .expect("game permit without shared scope");
    let guards = crate::mutation_boundary::enter_mutation_boundary_async(
        &fixture.context,
        &fixture.game_id,
        true,
    )
    .await
    .expect("combined game/shared boundary");
    assert!(
        shared_change
            .resolve_locked_inactive_plan(&fixture.context, &fixture.game_id)
            .expect("other owner keeps app registration")
            .is_none()
    );

    let error =
        super::commit::authorize_combined_install(super::commit::CombinedRenoDxInstallRequest {
            context: &fixture.context,
            feature: crate::addons::mutation_features::RENODX_INSTALL,
            guards,
            safety: &safety,
            game_id: &fixture.game_id,
            game_dir: fixture.game_dir.path(),
            prepared: &prepared,
            registered_exe_path: None,
            shared_change,
            source_last_modified: None,
            source_mtime: None,
            targets,
            expected_owner: None,
            receipt_release: None,
        })
        .expect_err("the shared owner still requires its existing shared permit");

    assert_eq!(
        error,
        ServiceError::SafetyContextMissing {
            scope: crate::SafetyScope::SharedVulkan,
        }
    );
    assert!(
        !fixture
            .game_dir
            .path()
            .join("renodx-reinstall.addon64")
            .exists()
    );
    assert!(!fixture.game_dir.path().join("dxgi.dll").exists());
    assert_eq!(
        fixture
            .context
            .storage()
            .get_installed_addon(&fixture.game_id)
            .expect("stored target owner")
            .expect("target owner remains"),
        target_record
    );
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

fn prepared_proxy_reinstall(game_id: &GameId) -> crate::addons::renodx::install::PreparedInstall {
    crate::addons::renodx::install::PreparedInstall {
        game_id: game_id.clone(),
        host_kind: crate::addons::reshade::proxy::HostKind::Proxy,
        proxy_dll_name: "dxgi.dll".to_owned(),
        addon_file_name: "renodx-new.addon64".to_owned(),
        addon_source_url: "https://example.test/renodx-new.addon64".to_owned(),
        source_digest: "new-addon-digest".to_owned(),
        source_etag: None,
        source_last_modified: None,
        addon_bytes: b"new-addon-bytes".to_vec(),
        reshade_dll_bytes: b"new-proxy-host".to_vec(),
        reshade_source_url: "https://example.test/reshade.zip".to_owned(),
        reshade_source_etag: None,
        reshade_last_modified: None,
        reshade_digest: "proxy-host-digest".to_owned(),
        reshade_channel: Some(ReshadeChannel::Stable),
        processing_path: crate::addons::renodx::types::RenoDxProcessingPath::Upgrade,
        renodx_config: None,
        ini_tweaks: crate::addons::renodx::types::renodx_ini_defaults(),
    }
}

fn external_reno_owner(
    game_id: &GameId,
    addon: &std::path::Path,
    host: &std::path::Path,
) -> InstalledAddon {
    InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(addon.to_string_lossy()).expect("old external add-on path"),
    )
    .with_host_kind(InstalledAddonHostKind::Proxy)
    .with_backed_up_file(PathRef::new(host.to_string_lossy()).expect("backed-up host path"))
}

async fn run_external_reno_reinstall(
    fixture: &SafetyFixture,
    old_record: &InstalledAddon,
    expected_record: &InstalledAddon,
) -> Result<InstalledAddon, ServiceError> {
    let prepared = prepared_proxy_reinstall(&fixture.game_id);
    let release =
        crate::addons::renodx::install::PreparedRenoDxUninstall::prepare_receipt_only(old_record)?;
    let mut targets = crate::addons::renodx::mutation_targets::install_targets(
        fixture.game_dir.path(),
        &prepared,
    )?;
    let owner = crate::addons::external_proxy_owner::InactiveExternalProxyOwner {
        record: old_record.clone(),
        unmanaged_paths: vec![PathBuf::from(old_record.addon_file().as_str())],
    };
    crate::addons::external_proxy_owner::augment_external_owner_targets(
        &mut targets,
        &owner,
        release.affected_paths(),
    );

    let authority = crate::FileSafetyAuthority::new();
    let assessment = authority.issue_game_assessment(&fixture.context, &fixture.game_id)?;
    let safety = authority.game_mutation_permits(
        fixture.game_id.clone(),
        Some(&assessment.context_token),
        None,
    )?;
    let guards = crate::mutation_boundary::enter_mutation_boundary_async(
        &fixture.context,
        &fixture.game_id,
        false,
    )
    .await?;

    super::commit::authorize_install_commit(
        &fixture.context,
        crate::addons::mutation_features::RENODX_INSTALL,
        guards,
        &safety,
        |guard| {
            crate::addons::durable::run_replace_expected_install_mutation(
                crate::addons::durable::TargetsMutation {
                    context: &fixture.context,
                    guard,
                    targets,
                    feature: crate::addons::mutation_features::RENODX_INSTALL,
                    game_id: &fixture.game_id,
                },
                expected_record,
                None,
                &[],
                || release.apply(),
                || {
                    let (record, commit) = crate::addons::renodx::install::install(
                        fixture.game_dir.path(),
                        &prepared,
                    )?;
                    let record = super::phase::annotate_install_record(
                        record,
                        prepared.host_kind,
                        ReshadeChannel::Stable,
                        None,
                    )?;
                    Ok((record, commit))
                },
                |_| {},
            )
        },
    )
}

#[tokio::test]
async fn external_proxy_reinstall_releases_old_receipts_and_publishes_new_owner_atomically() {
    let fixture = safety_fixture("external-proxy-reinstall-success");
    let external = tempdir().expect("external AddonPath");
    let old_addon = external.path().join("renodx-old.addon64");
    let old_host = external.path().join("dxgi.dll");
    let old_host_backup = crate::fs::backup_path(&old_host).expect("old host backup");
    let neighbor_ini = external.path().join("ReShade.ini");
    let current_ini = fixture.game_dir.path().join("ReShade.ini");
    let unrelated_ini = fixture.game_dir.path().join("Unrelated.ini");
    fs::write(&old_addon, b"old external addon").expect("old add-on");
    fs::write(&old_host, b"old external host").expect("old host");
    fs::write(&old_host_backup, b"prior host baseline").expect("host baseline");
    fs::write(&neighbor_ini, b"[GENERAL]\r\nExternal=keep\r\n").expect("neighbor INI");
    fs::write(&current_ini, b"[GENERAL]\r\nCurrent=keep\r\n").expect("current INI");
    fs::write(&unrelated_ini, b"[USER]\r\nKeep=exact\r\n").expect("unrelated INI");

    let old_record = external_reno_owner(&fixture.game_id, &old_addon, &old_host);
    fixture
        .context
        .storage()
        .upsert_installed_addon(&old_record)
        .expect("store old external owner");
    let old_record = fixture
        .context
        .storage()
        .get_installed_addon(&fixture.game_id)
        .expect("reload persisted old owner")
        .expect("old owner exists");
    let result = run_external_reno_reinstall(&fixture, &old_record, &old_record)
        .await
        .expect("atomic external replacement");

    assert!(!old_addon.exists(), "the old external payload is released");
    assert_eq!(
        fs::read(&old_host).expect("restored external host baseline"),
        b"prior host baseline"
    );
    assert!(
        !old_host_backup.exists(),
        "the consumed old backup sidecar is part of the release"
    );
    assert_eq!(
        fs::read(&neighbor_ini).expect("neighbor INI remains"),
        b"[GENERAL]\r\nExternal=keep\r\n"
    );
    assert_eq!(
        fs::read(&unrelated_ini).expect("unrelated INI remains"),
        b"[USER]\r\nKeep=exact\r\n"
    );
    assert_eq!(
        fs::read(fixture.game_dir.path().join("renodx-new.addon64")).expect("new payload"),
        b"new-addon-bytes"
    );
    assert_eq!(
        fs::read(fixture.game_dir.path().join("dxgi.dll")).expect("new host"),
        b"new-proxy-host"
    );
    assert!(result.host_kind() == Some(InstalledAddonHostKind::Proxy));
    let stored_new_owner = fixture
        .context
        .storage()
        .get_installed_addon(&fixture.game_id)
        .expect("stored owner")
        .expect("replacement owner");
    assert!(
        stored_new_owner.eq_ignoring_persistence_timestamps(&result),
        "publication preserves the complete returned owner and attaches persistence timestamps"
    );
    assert!(stored_new_owner.installed_at().is_some());
    assert!(stored_new_owner.updated_at().is_some());
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
async fn failed_external_proxy_reinstall_restores_old_receipts_and_keeps_old_owner() {
    let fixture = safety_fixture("external-proxy-reinstall-cas-failure");
    let external = tempdir().expect("external AddonPath");
    let old_addon = external.path().join("renodx-old.addon64");
    let old_host = external.path().join("dxgi.dll");
    let old_host_backup = crate::fs::backup_path(&old_host).expect("old host backup");
    let neighbor_ini = external.path().join("ReShade.ini");
    let current_ini = fixture.game_dir.path().join("ReShade.ini");
    let old_host_bytes = b"old external host";
    let backup_bytes = b"prior host baseline";
    let neighbor_ini_bytes = b"[GENERAL]\r\nExternal=keep\r\n";
    let current_ini_bytes = b"[GENERAL]\r\nCurrent=keep\r\n";
    fs::write(&old_addon, b"old external addon").expect("old add-on");
    fs::write(&old_host, old_host_bytes).expect("old host");
    fs::write(&old_host_backup, backup_bytes).expect("host baseline");
    fs::write(&neighbor_ini, neighbor_ini_bytes).expect("neighbor INI");
    fs::write(&current_ini, current_ini_bytes).expect("current INI");

    let old_record = external_reno_owner(&fixture.game_id, &old_addon, &old_host);
    fixture
        .context
        .storage()
        .upsert_installed_addon(&old_record)
        .expect("store old external owner");
    let old_record = fixture
        .context
        .storage()
        .get_installed_addon(&fixture.game_id)
        .expect("reload persisted old owner")
        .expect("old owner exists");
    let wrong_expected = InstalledAddon::new(
        fixture.game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(external.path().join("different.addon64").to_string_lossy())
            .expect("wrong owner path"),
    )
    .with_host_kind(InstalledAddonHostKind::Proxy);

    let error = run_external_reno_reinstall(&fixture, &old_record, &wrong_expected)
        .await
        .expect_err("the final exact-owner CAS must fail");
    assert!(
        !matches!(error, ServiceError::SafetyContextMissing { .. }),
        "the injected failure must reach the durable catalog publication"
    );
    assert_eq!(
        fs::read(&old_addon).expect("old add-on restored"),
        b"old external addon"
    );
    assert_eq!(
        fs::read(&old_host).expect("old host restored"),
        old_host_bytes
    );
    assert_eq!(
        fs::read(&old_host_backup).expect("backup restored"),
        backup_bytes
    );
    assert_eq!(
        fs::read(&neighbor_ini).expect("external neighbor unchanged"),
        neighbor_ini_bytes
    );
    assert_eq!(
        fs::read(&current_ini).expect("current INI restored"),
        current_ini_bytes
    );
    assert!(
        !fixture.game_dir.path().join("renodx-new.addon64").exists(),
        "new payload writes roll back"
    );
    assert!(!fixture.game_dir.path().join("dxgi.dll").exists());
    assert_eq!(
        fixture
            .context
            .storage()
            .get_installed_addon(&fixture.game_id)
            .expect("stored owner")
            .expect("old owner retained"),
        old_record
    );
    assert!(
        fixture
            .context
            .storage()
            .pending_file_mutations_for_game(&fixture.game_id)
            .expect("pending mutations")
            .is_empty()
    );
}
