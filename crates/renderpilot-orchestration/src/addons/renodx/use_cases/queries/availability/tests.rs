#[cfg(windows)]
use std::assert_matches;

use super::*;
use crate::addons::matching::{IncompatibilityReason, MatchFacts};
#[cfg(windows)]
use crate::addons::records;
use crate::addons::renodx::test_support::manifest;
#[cfg(windows)]
use crate::addons::renodx::test_support::{
    MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports, rule, title,
};
#[cfg(windows)]
use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{ExeGraphicsInfo, GraphicsApi, Launcher};
#[cfg(windows)]
use renderpilot_domain::{
    GameIdentity, GameInstallation, GameRuntime, InstalledAddon, PathRef, Platform,
};
use tempfile::tempdir;

fn directx_facts() -> MatchFacts {
    MatchFacts {
        launcher: Launcher::Steam,
        external_id: Some("1091500".to_owned()),
        exe_file_name: Some("game.exe".to_owned()),
        exe_sha256: None,
        engine: None,
        graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X64))
            .with_graphics_dlls(vec!["dxgi.dll".to_owned()]),
    }
}

#[cfg(windows)]
fn full_reshade_host_bytes() -> Vec<u8> {
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

#[test]
fn manual_install_is_not_offered_for_unmatched_games() {
    let report = manual_file_install(
        &manifest(Vec::new()),
        &directx_facts(),
        &RenoDxResolution::NoMatch,
    );

    assert!(report.is_none());
}

#[test]
fn manual_install_can_be_offered_for_matched_incompatible_directx_games() {
    let report = manual_file_install(
        &manifest(Vec::new()),
        &directx_facts(),
        &RenoDxResolution::Incompatible {
            reason: IncompatibilityReason::ArchUnknown,
        },
    );

    assert!(report.is_some());
}

#[tokio::test]
#[cfg(windows)]
async fn availability_auto_adopts_proxy_install_after_db_loss() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091500").expect("game id");
    let exe_path = game_dir.path().join("Game.exe");

    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    std::fs::write(game_dir.path().join("dxgi.dll"), full_reshade_host_bytes())
        .expect("write host");
    std::fs::write(game_dir.path().join("renodx-cp2077.addon64"), b"addon").expect("write addon");
    std::fs::write(
        game_dir.path().join("ReShade.ini"),
        "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
    )
    .expect("write ini");

    let identity = GameIdentity::new(game_id.clone(), "Cyberpunk 2077", Launcher::Steam)
        .expect("identity")
        .with_external_id("1091500")
        .expect("external id");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_dir.path().to_string_lossy().replace('\\', "/")).expect("install path"),
    )
    .with_executable_candidate(
        PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
    );
    context.storage().upsert_game(&game).expect("seed game");

    let manifest = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        crate::addons::renodx::types::Status::Working,
        vec![rule(
            crate::addons::renodx::types::MatchKind::SteamAppid,
            "1091500",
            100,
        )],
    )]);
    let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    reshade_sources.stable = None;

    let preview =
        availability(&context, &manifest, &reshade_sources, &game_id).expect("pure availability");
    assert_eq!(preview.state, RenoDxInstallState::NotInstalled);
    assert!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("get before reconciliation")
            .is_none(),
        "the pure query must not persist an adopted record"
    );

    let report = load_availability(&context, &manifest, &reshade_sources, &game_id)
        .await
        .expect("availability");

    assert_matches!(report.state, RenoDxInstallState::Installed { .. });
    assert!(report.actions.install.is_none());
    assert!(report.actions.use_existing.is_some());
    let record = records::record_of_kind(&context, &game_id, AddonKind::RenoDx)
        .expect("read adopted record")
        .expect("adopted record");
    assert!(record.installed_at().is_some());
    assert_eq!(record.addon_version(), None);
}

#[test]
#[cfg(windows)]
fn availability_reports_blocked_by_other_addon_when_a_luma_record_exists() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091510").expect("game id");
    let exe_path = game_dir.path().join("Game.exe");

    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");

    let identity = GameIdentity::new(game_id.clone(), "Cyberpunk 2077", Launcher::Steam)
        .expect("identity")
        .with_external_id("1091510")
        .expect("external id");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_dir.path().to_string_lossy().replace('\\', "/")).expect("install path"),
    )
    .with_executable_candidate(
        PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
    );
    context.storage().upsert_game(&game).expect("seed game");

    // Luma is already managing this game.
    let luma_record = InstalledAddon::new(
        game_id.clone(),
        renderpilot_domain::AddonKind::Luma,
        PathRef::new(
            game_dir
                .path()
                .join("Luma-Game.addon")
                .to_string_lossy()
                .replace('\\', "/"),
        )
        .expect("luma addon path"),
    );
    context
        .storage()
        .upsert_installed_addon(&luma_record)
        .expect("seed luma record");

    let manifest = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        crate::addons::renodx::types::Status::Working,
        vec![rule(
            crate::addons::renodx::types::MatchKind::SteamAppid,
            "1091510",
            100,
        )],
    )]);
    let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    reshade_sources.stable = None;

    let report =
        availability(&context, &manifest, &reshade_sources, &game_id).expect("availability");

    match report.outcome {
        AvailabilityOutcome::BlockedByOtherAddon {
            other_kind,
            unmanaged,
        } => {
            assert_eq!(other_kind, renderpilot_domain::AddonKind::Luma);
            assert!(!unmanaged, "a tracked DB record is not an unmanaged block");
        }
        other => panic!("expected BlockedByOtherAddon, got {other:?}"),
    }
    assert!(report.manual_install.is_none());
    assert!(matches!(report.state, RenoDxInstallState::NotInstalled));

    // Orphan adoption must not have run: no RenoDX record was created, and the
    // Luma record is untouched.
    let still_present = records::foreign_record(&context, &game_id, AddonKind::RenoDx)
        .expect("get")
        .expect("luma record survives");
    assert_eq!(still_present.kind(), renderpilot_domain::AddonKind::Luma);
}

#[test]
#[cfg(windows)]
fn availability_reports_blocked_by_other_addon_for_unmanaged_luma_files() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091511").expect("game id");
    let exe_path = game_dir.path().join("Game.exe");

    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    // No DB record for either tool — just an unmanaged Luma add-on on disk.
    std::fs::write(game_dir.path().join("Luma-Game.addon"), b"luma-addon")
        .expect("write luma addon");

    let identity = GameIdentity::new(game_id.clone(), "Cyberpunk 2077", Launcher::Steam)
        .expect("identity")
        .with_external_id("1091511")
        .expect("external id");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_dir.path().to_string_lossy().replace('\\', "/")).expect("install path"),
    )
    .with_executable_candidate(
        PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
    );
    context.storage().upsert_game(&game).expect("seed game");

    let manifest = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        crate::addons::renodx::types::Status::Working,
        vec![rule(
            crate::addons::renodx::types::MatchKind::SteamAppid,
            "1091511",
            100,
        )],
    )]);
    let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    reshade_sources.stable = None;

    let report =
        availability(&context, &manifest, &reshade_sources, &game_id).expect("availability");

    match report.outcome {
        AvailabilityOutcome::BlockedByOtherAddon {
            other_kind,
            unmanaged,
        } => {
            assert_eq!(other_kind, renderpilot_domain::AddonKind::Luma);
            assert!(
                unmanaged,
                "an on-disk-only Luma install is an unmanaged block"
            );
        }
        other => panic!("expected BlockedByOtherAddon, got {other:?}"),
    }
    // No RenoDX record must have been created either.
    // Raw repository read on purpose: asserts no record of ANY kind was created.
    assert!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("get")
            .is_none()
    );
}

/// Unmanaged peer files next to a nested shipping exe must block availability
/// the same way install does — not only when they sit at the library root.
#[test]
#[cfg(windows)]
fn availability_blocks_on_unmanaged_peer_beside_nested_exe() {
    let db_dir = tempdir().expect("db dir");
    let install_root = tempdir().expect("install root");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091512").expect("game id");
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
    // Peer files only beside the exe — library root stays clean.
    std::fs::write(target_dir.join("Luma-Game.addon"), b"luma-addon").expect("write luma");

    let identity = GameIdentity::new(game_id.clone(), "Nested Game", Launcher::Steam)
        .expect("identity")
        .with_external_id("1091512")
        .expect("external id");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(install_root.path().to_string_lossy().replace('\\', "/"))
            .expect("install path"),
    )
    .with_executable_candidate(
        PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
    );
    context.storage().upsert_game(&game).expect("seed game");

    let manifest = manifest(vec![title(
        "nested-game",
        "nested",
        Architecture::X64,
        crate::addons::renodx::types::Status::Working,
        vec![rule(
            crate::addons::renodx::types::MatchKind::SteamAppid,
            "1091512",
            100,
        )],
    )]);
    let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    reshade_sources.stable = None;

    let report =
        availability(&context, &manifest, &reshade_sources, &game_id).expect("availability");
    match report.outcome {
        AvailabilityOutcome::BlockedByOtherAddon {
            other_kind,
            unmanaged,
        } => {
            assert_eq!(other_kind, renderpilot_domain::AddonKind::Luma);
            assert!(unmanaged, "nested unmanaged peer must be detected");
        }
        other => panic!("expected BlockedByOtherAddon for nested peer, got {other:?}"),
    }
}

#[tokio::test]
#[cfg(windows)]
async fn availability_auto_adopts_proxy_install_with_dlss_fix_companion() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    // A distinct appid from the other adoption tests in this module — the
    // per-game `game_mutation_lock` is a global `static`, so tests sharing one ID
    // would contend for the same lock when `cargo test` runs them in parallel.
    let game_id = GameId::new("steam:1091501").expect("game id");
    let exe_path = game_dir.path().join("Game.exe");

    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    std::fs::write(game_dir.path().join("dxgi.dll"), full_reshade_host_bytes())
        .expect("write host");
    std::fs::write(game_dir.path().join("renodx-cp2077.addon64"), b"addon").expect("write addon");
    // The DLSS-Fix companion, co-located with the main addon.
    std::fs::write(game_dir.path().join("renodx-dlssfix.addon64"), b"dlssfix")
        .expect("write dlssfix");
    std::fs::write(
        game_dir.path().join("ReShade.ini"),
        "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
    )
    .expect("write ini");

    let identity = GameIdentity::new(game_id.clone(), "Cyberpunk 2077", Launcher::Steam)
        .expect("identity")
        .with_external_id("1091501")
        .expect("external id");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_dir.path().to_string_lossy().replace('\\', "/")).expect("install path"),
    )
    .with_executable_candidate(
        PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
    );
    context.storage().upsert_game(&game).expect("seed game");

    let manifest = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        crate::addons::renodx::types::Status::Working,
        vec![rule(
            crate::addons::renodx::types::MatchKind::SteamAppid,
            "1091501",
            100,
        )],
    )]);
    let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    reshade_sources.stable = None;

    let report = load_availability(&context, &manifest, &reshade_sources, &game_id)
        .await
        .expect("availability");

    assert_matches!(report.state, RenoDxInstallState::Installed { .. });

    let record = records::record_of_kind(&context, &game_id, AddonKind::RenoDx)
        .expect("read adopted record")
        .expect("adopted record");

    // Symptom 1 fixed: the adopted addon-file path (and its digest) come from
    // the real main addon, not the DLSS-Fix file.
    assert_eq!(
        record.addon_file().file_name(),
        Some("renodx-cp2077.addon64")
    );
    let addon_source = record
        .tracked_sources()
        .iter()
        .find(|s| s.role() == renderpilot_domain::TrackedSourceRole::AddonPayload)
        .expect("addon source recorded");
    let real_addon_digest =
        renderpilot_detection::sha256_file(&game_dir.path().join("renodx-cp2077.addon64"))
            .expect("hash real addon")
            .to_string();
    assert_eq!(addon_source.digest(), real_addon_digest);

    // Symptom 2 fixed: the central ownership projection sees DLSS-Fix evidence.
    assert!(crate::addons::renodx::dlss_fix_binding::resolve(&record).has_evidence);
}

#[tokio::test]
#[cfg(windows)]
async fn availability_does_not_adopt_a_stray_addon_file_under_the_wrong_name() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091502").expect("game id");
    let exe_path = game_dir.path().join("Game.exe");

    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    std::fs::write(game_dir.path().join("dxgi.dll"), full_reshade_host_bytes())
        .expect("write host");
    // No renodx-cp2077.addon64 (the resolved slug's exact expected name).
    // Only an unrelated add-on file sits in the folder — must NOT be
    // mistaken for this game's add-on.
    std::fs::write(game_dir.path().join("renodx-othertitle.addon64"), b"addon")
        .expect("write stray addon");
    std::fs::write(
        game_dir.path().join("ReShade.ini"),
        "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
    )
    .expect("write ini");

    let identity = GameIdentity::new(game_id.clone(), "Cyberpunk 2077", Launcher::Steam)
        .expect("identity")
        .with_external_id("1091502")
        .expect("external id");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_dir.path().to_string_lossy().replace('\\', "/")).expect("install path"),
    )
    .with_executable_candidate(
        PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
    );
    context.storage().upsert_game(&game).expect("seed game");

    let manifest = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        crate::addons::renodx::types::Status::Working,
        vec![rule(
            crate::addons::renodx::types::MatchKind::SteamAppid,
            "1091502",
            100,
        )],
    )]);
    let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    reshade_sources.stable = None;

    let report = load_availability(&context, &manifest, &reshade_sources, &game_id)
        .await
        .expect("availability");

    assert_eq!(report.state, RenoDxInstallState::NotInstalled);
    // Raw repository read on purpose: asserts no record of ANY kind was created.
    assert!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("read record")
            .is_none()
    );
}
