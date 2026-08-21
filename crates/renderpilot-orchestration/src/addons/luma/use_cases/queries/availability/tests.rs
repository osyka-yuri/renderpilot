use std::path::Path;
use std::time::Duration;

use super::*;

use crate::addons::luma::test_support::{
    MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports, manifest, reshade_sources, rule,
    sample_dgvoodoo_requirement, title,
};
use crate::addons::luma::types::Status;
use crate::addons::matching::MatchKind;
use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::InstalledAddon;
use renderpilot_domain::{
    Architecture, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};
use tempfile::tempdir;

fn seed_game(context: &Context, game_id: &GameId, appid: &str, game_dir: &Path, exe_path: &Path) {
    let identity = GameIdentity::new(game_id.clone(), "Dishonored 2", Launcher::Steam)
        .expect("identity")
        .with_external_id(appid)
        .expect("external id");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_dir.to_string_lossy().replace('\\', "/")).expect("install path"),
    )
    .with_executable_candidate(
        PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
    );
    context.storage().upsert_game(&game).expect("seed game");
}

fn curated_manifest(appid: &str) -> LumaManifest {
    let mut m = manifest(vec![title(
        "dishonored-2",
        "Luma-Dishonored_2.zip",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, appid, 100)],
    )]);
    m.min_reshade_version = "6.7.0".to_owned();
    m
}

fn reshade_host_bytes() -> Vec<u8> {
    build_pe_with_exports(
        MACHINE_AMD64,
        PE32_PLUS_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
            "ReShadeUnregisterEvent",
        ],
    )
}

#[tokio::test]
async fn availability_loader_serializes_with_other_game_operations() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403651").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    seed_game(&context, &game_id, "403651", game_dir.path(), &exe_path);
    let manifest = curated_manifest("403651");

    let guard = game_mutation_lock::lock(&game_id).await;
    let reshade_sources = reshade_sources();
    let mut query = Box::pin(load_availability(
        &context,
        &manifest,
        &reshade_sources,
        &game_id,
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(25), query.as_mut())
            .await
            .is_err(),
        "availability must wait while another operation owns the game lock"
    );

    drop(guard);
    let report = tokio::time::timeout(Duration::from_secs(2), query)
        .await
        .expect("availability resumes after the lock is released")
        .expect("availability");
    assert!(matches!(
        report.outcome,
        AvailabilityOutcome::Installable { .. }
    ));
}

#[test]
fn availability_reports_installable_for_a_curated_match_with_no_host() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403640").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    seed_game(&context, &game_id, "403640", game_dir.path(), &exe_path);

    let report = availability(
        &context,
        &curated_manifest("403640"),
        &reshade_sources(),
        &game_id,
    )
    .expect("availability");

    assert!(matches!(
        report.outcome,
        AvailabilityOutcome::Installable { .. }
    ));
    assert!(report.actions.install.is_some());
    assert_eq!(report.state, LumaInstallState::NotInstalled);
}

#[test]
fn availability_surfaces_external_requirement_for_an_installable_match() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403647").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    seed_game(&context, &game_id, "403647", game_dir.path(), &exe_path);

    let mut manifest = curated_manifest("403647");
    manifest.titles[0].external_requirement = Some(sample_dgvoodoo_requirement());
    let report =
        availability(&context, &manifest, &reshade_sources(), &game_id).expect("availability");

    match report.outcome {
        AvailabilityOutcome::Installable {
            external_requirement,
            ..
        } => assert!(matches!(
            external_requirement,
            Some(ManagedDependencySummary::Dgvoodoo2 { version }) if version == "2.87.3"
        )),
        other => panic!("expected installable, got {other:?}"),
    }
}

/// Unmanaged RenoDX beside a nested shipping exe must block Luma availability
/// the same way install does.
#[test]
fn availability_blocks_on_unmanaged_renodx_beside_nested_exe() {
    let db_dir = tempdir().expect("db dir");
    let install_root = tempdir().expect("install root");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403643").expect("game id");
    let target_dir = install_root
        .path()
        .join("Game")
        .join("Binaries")
        .join("Win64");
    std::fs::create_dir_all(&target_dir).expect("nested target");
    let exe_path = target_dir.join("Game-Win64-Shipping.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    std::fs::write(target_dir.join("renodx-nested.addon64"), b"renodx").expect("renodx");
    seed_game(&context, &game_id, "403643", install_root.path(), &exe_path);

    let report = availability(
        &context,
        &curated_manifest("403643"),
        &reshade_sources(),
        &game_id,
    )
    .expect("availability");

    match report.outcome {
        AvailabilityOutcome::BlockedByOtherAddon {
            other_kind,
            unmanaged,
        } => {
            assert_eq!(other_kind, AddonKind::RenoDx);
            assert!(unmanaged, "nested unmanaged peer must be detected");
        }
        other => panic!("expected BlockedByOtherAddon for nested peer, got {other:?}"),
    }
}

#[test]
fn availability_reports_blocked_by_other_addon_when_a_renodx_record_exists() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403641").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    seed_game(&context, &game_id, "403641", game_dir.path(), &exe_path);

    let renodx_record = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(
            game_dir
                .path()
                .join("renodx-test.addon64")
                .to_string_lossy()
                .replace('\\', "/"),
        )
        .expect("path"),
    );
    context
        .storage()
        .upsert_installed_addon(&renodx_record)
        .expect("seed renodx record");

    let report = availability(
        &context,
        &curated_manifest("403641"),
        &reshade_sources(),
        &game_id,
    )
    .expect("availability");

    match report.outcome {
        AvailabilityOutcome::BlockedByOtherAddon {
            other_kind,
            unmanaged,
        } => {
            assert_eq!(other_kind, AddonKind::RenoDx);
            assert!(!unmanaged);
        }
        other => panic!("expected BlockedByOtherAddon, got {other:?}"),
    }
    assert_eq!(report.state, LumaInstallState::NotInstalled);
}

#[tokio::test]
async fn availability_loader_auto_adopts_luma_install_after_db_loss() {
    // Mirrors the RenoDX adoption test. After a DB loss (or record wipe),
    // availability must discover stray Luma files, create an adopted record
    // with the discovered created_files, and report Installed state so that
    // uninstall becomes available and the user is not forced to delete by hand.
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403642").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    let addon_path = game_dir.path().join("Luma-Dishonored_2.addon");
    std::fs::write(&addon_path, b"x").expect("addon");
    std::fs::write(game_dir.path().join("dxgi.dll"), reshade_host_bytes()).expect("ReShade host");
    std::fs::create_dir_all(game_dir.path().join("Luma").join("Global")).expect("luma dir");
    std::fs::write(
        game_dir
            .path()
            .join("Luma")
            .join("Global")
            .join("Copy_PS.hlsl"),
        b"ps",
    )
    .expect("shader");

    seed_game(&context, &game_id, "403642", game_dir.path(), &exe_path);

    let manifest = curated_manifest("403642");
    let preview =
        availability(&context, &manifest, &reshade_sources(), &game_id).expect("pure availability");
    assert!(matches!(
        preview.outcome,
        AvailabilityOutcome::UnmanagedPresent
    ));
    assert!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("get before reconciliation")
            .is_none(),
        "the pure query must not persist an adopted record"
    );

    let report = load_availability(&context, &manifest, &reshade_sources(), &game_id)
        .await
        .expect("reconciled availability");

    // Recovery stays local-only, but the exact manifest payload gives it a
    // checkable advisory content identity. It must not look untracked in
    // the UI merely because the database row was lost.
    assert!(matches!(report.state, LumaInstallState::Installed { .. }));
    // Outcome is no longer UnmanagedPresent thanks to adoption.
    assert!(!matches!(
        report.outcome,
        AvailabilityOutcome::UnmanagedPresent
    ));

    let stored = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("get")
        .expect("adopted record must exist");
    assert_eq!(stored.kind(), AddonKind::Luma);
    assert!(stored.has_addon_source());
    assert_eq!(stored.reshade_channel(), None);
    // Must contain the marker + at least the shader we wrote under Luma/
    // (this is what makes uninstall() able to perform a clean recovery).
    let created: Vec<_> = stored
        .created_files()
        .iter()
        .map(|p| p.as_str().to_owned())
        .collect();
    assert!(
        created
            .iter()
            .any(|p| p.ends_with("Luma-Dishonored_2.addon"))
    );
    assert!(created.iter().any(
        |p| p.contains("Luma/Global/Copy_PS.hlsl") || p.contains("Luma\\Global\\Copy_PS.hlsl")
    ));
    assert!(
        !created.iter().any(|path| path.ends_with("dxgi.dll")),
        "an unreadable-version host is not repaired or claimed during local-only recovery"
    );
}

#[tokio::test]
async fn availability_loader_adopts_the_manifest_named_payload_after_db_loss() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403649").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    let addon_path = game_dir.path().join("Luma-Dishonored 2.addon");
    std::fs::write(&addon_path, b"addon").expect("addon");
    std::fs::create_dir_all(game_dir.path().join("Luma").join("Global")).expect("luma dir");
    std::fs::write(
        game_dir
            .path()
            .join("Luma")
            .join("Global")
            .join("Copy_PS.hlsl"),
        b"ps",
    )
    .expect("shader");
    seed_game(&context, &game_id, "403649", game_dir.path(), &exe_path);

    let mut manifest = curated_manifest("403649");
    manifest.titles[0].addon_file = "Luma-Dishonored 2.addon".to_owned();
    let report = load_availability(&context, &manifest, &reshade_sources(), &game_id)
        .await
        .expect("availability");

    assert!(matches!(report.state, LumaInstallState::Installed { .. }));
    let stored = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("get")
        .expect("adopted record");
    assert!(
        stored
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("Luma-Dishonored 2.addon")),
        "adoption must use the explicit manifest identity, never the ZIP stem"
    );
    assert!(stored.has_addon_source());
}

#[tokio::test]
async fn availability_loader_never_adopts_an_asset_derived_payload_name() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403650").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    std::fs::write(game_dir.path().join("Luma-Dishonored_2.addon"), b"addon")
        .expect("asset-derived addon");
    seed_game(&context, &game_id, "403650", game_dir.path(), &exe_path);

    let mut manifest = curated_manifest("403650");
    manifest.titles[0].addon_file = "Luma-Dishonored 2.addon".to_owned();
    let report = load_availability(&context, &manifest, &reshade_sources(), &game_id)
        .await
        .expect("availability");

    assert!(matches!(
        report.outcome,
        AvailabilityOutcome::UnmanagedPresent
    ));
    assert!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("get")
            .is_none(),
        "recovery must not guess from a ZIP-derived or otherwise similar sibling"
    );
}

#[tokio::test]
async fn availability_loader_db_loss_with_user_effects_adopts_only_luma_payload() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403648").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    std::fs::write(game_dir.path().join("Luma-Dishonored_2.addon"), b"addon").expect("addon");
    std::fs::write(game_dir.path().join("dxgi.dll"), reshade_host_bytes()).expect("ReShade host");
    std::fs::create_dir_all(game_dir.path().join("reshade-shaders").join("Shaders"))
        .expect("effect directory");
    std::fs::write(
        game_dir
            .path()
            .join("reshade-shaders")
            .join("Shaders")
            .join("User.fx"),
        b"technique User {}",
    )
    .expect("user effect");
    seed_game(&context, &game_id, "403648", game_dir.path(), &exe_path);

    let report = load_availability(
        &context,
        &curated_manifest("403648"),
        &reshade_sources(),
        &game_id,
    )
    .await
    .expect("availability");
    assert!(matches!(report.state, LumaInstallState::Installed { .. }));

    let stored = context
        .storage()
        .get_installed_addon(&game_id)
        .expect("get")
        .expect("adopted record");
    assert!(
        stored
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("Luma-Dishonored_2.addon"))
    );
    assert!(
        !stored
            .created_files()
            .iter()
            .any(|path| path.as_str().ends_with("dxgi.dll")),
        "user effects keep the existing host outside Luma ownership"
    );
}

#[test]
fn availability_reports_installable_not_unmanaged_when_the_debris_is_also_torn() {
    // P1.2: the same on-disk debris that alone would read as
    // `UnmanagedPresent` must instead resolve to a normal `Installable`
    // outcome (plus `install_torn: true`) once the crash-safety sentinel
    // shows it is abandoned state from an interrupted install — the
    // install command auto-recovers it (see `luma::install::recover_torn_install`),
    // so availability should not steer the user toward a manual cleanup.
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403646").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    std::fs::write(
        game_dir.path().join("Luma-Dishonored_2.addon"),
        b"half-written",
    )
    .expect("addon debris");
    std::fs::write(game_dir.path().join("renderpilot-luma-install.lock"), b"")
        .expect("write sentinel");
    seed_game(&context, &game_id, "403646", game_dir.path(), &exe_path);

    let report = availability(
        &context,
        &curated_manifest("403646"),
        &reshade_sources(),
        &game_id,
    )
    .expect("availability");

    assert!(report.install_torn);
    assert!(matches!(
        report.outcome,
        AvailabilityOutcome::Installable { .. }
    ));
}

#[test]
fn availability_reports_incompatible_on_a_curated_arch_mismatch() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403643").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    seed_game(&context, &game_id, "403643", game_dir.path(), &exe_path);

    // The detected exe is 64-bit (per `build_pe_with_exports(MACHINE_AMD64, ..)`
    // above), but the curated title declares X86 — a hard arch-gate mismatch.
    let mut manifest = curated_manifest("403643");
    manifest.titles[0].arch = Architecture::X86;

    let report =
        availability(&context, &manifest, &reshade_sources(), &game_id).expect("availability");
    assert!(matches!(
        report.outcome,
        AvailabilityOutcome::Incompatible { .. }
    ));
}

#[test]
fn availability_reports_install_torn_when_the_crash_sentinel_is_present() {
    // B.5: a prior install/rollback that didn't complete cleanly leaves the
    // engine's crash-safety sentinel behind (see `engine::is_install_torn`).
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403644").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    seed_game(&context, &game_id, "403644", game_dir.path(), &exe_path);
    std::fs::write(game_dir.path().join("renderpilot-luma-install.lock"), b"")
        .expect("write sentinel");

    let report = availability(
        &context,
        &curated_manifest("403644"),
        &reshade_sources(),
        &game_id,
    )
    .expect("availability");

    assert!(report.install_torn);
}

#[test]
fn availability_reports_install_not_torn_for_a_clean_folder() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:403645").expect("game id");
    let exe_path = game_dir.path().join("Dishonored2.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    seed_game(&context, &game_id, "403645", game_dir.path(), &exe_path);

    let report = availability(
        &context,
        &curated_manifest("403645"),
        &reshade_sources(),
        &game_id,
    )
    .expect("availability");

    assert!(!report.install_torn);
}
