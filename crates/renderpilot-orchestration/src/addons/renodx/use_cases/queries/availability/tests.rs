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
use renderpilot_application::{GameRepository, InstalledAddonRepository, ProxyTopologyRepository};
use renderpilot_domain::{ExeGraphicsInfo, GraphicsApi, Launcher};
#[cfg(windows)]
use renderpilot_domain::{
    FileReceipt, GameIdentity, GameInstallation, GameProxyTopology, GameRuntime, InstalledAddon,
    InstalledAddonHostKind, PathRef, Platform, ProxyImplementation, ProxyLink, ProxyRootPrestate,
    Sha256Hash,
};
#[cfg(windows)]
use tempfile::tempdir;

fn directx_facts() -> MatchFacts {
    MatchFacts {
        launcher: Launcher::Steam,
        external_id: Some("1091500".to_owned()),
        exe_file_name: Some("game.exe".to_owned()),
        engine: None,
        unreal_version: None,
        graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X64))
            .with_graphics_dlls(vec!["dxgi.dll".to_owned()]),
        unreal_detection: None,
        target_platform: None,
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

#[cfg(windows)]
fn seed_availability_game(
    context: &Context,
    game_id: &renderpilot_domain::GameId,
    appid: &str,
    game_root: &std::path::Path,
    exe_path: &std::path::Path,
) {
    let identity = GameIdentity::new(game_id.clone(), "Cyberpunk 2077", Launcher::Steam)
        .expect("identity")
        .with_external_id(appid)
        .expect("external id");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(game_root.to_string_lossy().replace('\\', "/")).expect("install path"),
    )
    .with_executable_candidate(
        PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
    );
    context.storage().upsert_game(&game).expect("seed game");
}

#[cfg(windows)]
fn seed_external_luma_owner(
    context: &Context,
    game_id: &renderpilot_domain::GameId,
    external_dir: &std::path::Path,
) -> InstalledAddon {
    let addon_path = external_dir.join("Luma-Cyberpunk.addon");
    std::fs::write(&addon_path, b"external Luma payload").expect("write external payload");
    let addon_path = PathRef::new(addon_path.to_string_lossy().replace('\\', "/"))
        .expect("external add-on path");
    let record = InstalledAddon::new(game_id.clone(), AddonKind::Luma, addon_path.clone())
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_created_file(addon_path);
    context
        .storage()
        .upsert_installed_addon(&record)
        .expect("seed external Luma owner");
    record
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
fn manual_install_uses_proxy_for_directx_or_inconclusive_and_layer_for_vulkan() {
    let manifest = manifest(Vec::new());
    let resolution = RenoDxResolution::Incompatible {
        reason: IncompatibilityReason::ArchUnknown,
    };
    let mut facts = directx_facts();

    for (apis, expected_host) in [
        (
            vec![GraphicsApi::D3D12],
            crate::addons::reshade::proxy::HostKind::Proxy,
        ),
        (Vec::new(), crate::addons::reshade::proxy::HostKind::Proxy),
        (
            vec![GraphicsApi::Vulkan],
            crate::addons::reshade::proxy::HostKind::Vulkan,
        ),
    ] {
        facts.graphics = ExeGraphicsInfo::new(apis, Some(Architecture::X64));
        let offer = manual_file_install(&manifest, &facts, &resolution).expect("manual offer");
        assert_eq!(offer.host_kind, expected_host);
    }

    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
    assert!(manual_file_install(&manifest, &facts, &resolution).is_none());
}

#[test]
fn manual_install_is_not_offered_for_unsupported_settings() {
    let report = manual_file_install(
        &manifest(Vec::new()),
        &directx_facts(),
        &RenoDxResolution::UnsupportedSettings,
    );

    assert!(report.is_none());
}

#[test]
#[cfg(windows)]
fn retained_shared_vulkan_owner_requires_shared_install_scope_without_active_payload() {
    let db_dir = tempdir().expect("db dir");
    let install_dir = tempdir().expect("install dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091598").expect("game id");
    let addon_path = install_dir.path().join("missing-renodx.addon64");
    let exe_path = install_dir.path().join("Game.exe");
    context
        .storage()
        .upsert_installed_addon(
            &InstalledAddon::new(
                game_id.clone(),
                AddonKind::RenoDx,
                PathRef::new(addon_path.to_string_lossy().replace('\\', "/")).expect("addon path"),
            )
            .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
            .with_registered_exe_path(
                PathRef::new(exe_path.to_string_lossy().replace('\\', "/"))
                    .expect("registered executable"),
            ),
        )
        .expect("seed external owner");

    assert!(!addon_path.exists(), "the local payload is absent");
    assert!(
        crate::addons::records::active_record_of_kind(&context, &game_id, AddonKind::RenoDx,)
            .expect("active record query")
            .is_none(),
        "the retained host receipt must not be a local installed flag"
    );
    assert!(install_requires_shared_vulkan(&context, &game_id).expect("install scope"));
}

#[test]
#[cfg(windows)]
fn engine_config_report_uses_only_matching_standalone_owner_without_addon_record() {
    use renderpilot_domain::{EngineConfigJournal, EngineConfigTransition};

    fn report_status(
        owner_kind: AddonKind,
        pending: bool,
    ) -> crate::addons::engine_config::service::EngineConfigStatus {
        let temp = tempdir().expect("temp");
        let context = Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game_id =
            GameId::new(format!("manual:engine-owner-{owner_kind:?}-{pending}")).expect("game id");
        let owner_target = temp.path().join("external").join("Engine.ini");
        let report_target = temp.path().join("game-config").join("Engine.ini");
        std::fs::create_dir_all(owner_target.parent().expect("owner parent"))
            .expect("owner directory");
        std::fs::create_dir_all(report_target.parent().expect("report parent"))
            .expect("report directory");

        let guidance = engine_ini_guidance();
        let recipe = guidance
            .engine_ini
            .as_ref()
            .expect("typed Engine.ini recipe")
            .to_engine_config_recipe(&guidance.id)
            .expect("runtime recipe");
        let recipes = EngineIniRecipeSet::from_recipes([&recipe]).expect("recipe set");
        crate::addons::engine_config::service::apply(
            context.storage(),
            &game_id,
            owner_kind,
            &owner_target,
            &recipes,
            "availability-owner-apply",
        )
        .expect("persist standalone owner");

        if pending {
            let owner = context
                .storage()
                .engine_config_journal_owner(&game_id)
                .expect("read canonical owner")
                .expect("owner");
            let stable = owner.journal.stable.clone().expect("stable receipt");
            let pending_journal = EngineConfigJournal {
                stable: Some(stable.clone()),
                pending: Some(EngineConfigTransition {
                    operation_id: "availability-owner-pending".to_owned(),
                    stage_name: ".renderpilot-engine-availability-owner-pending.stage".to_owned(),
                    prior: Some(stable.clone()),
                    after: None,
                    before_digest: stable.after_digest.clone(),
                    after_digest: stable.after_digest,
                }),
            };
            assert!(
                context
                    .storage()
                    .compare_and_swap_engine_config_journal(
                        &game_id,
                        owner_kind,
                        Some(&owner.raw_token),
                        Some(&pending_journal),
                    )
                    .expect("save pending owner")
            );
        }

        assert!(
            context
                .storage()
                .get_installed_addon(&game_id)
                .expect("local add-on record")
                .is_none()
        );
        let report = super::engine_config_report(
            &context,
            &game_id,
            AddonKind::RenoDx,
            &crate::addons::engine_config::EngineIniResolution::ReadyToCreate(report_target),
            &[guidance],
        )
        .expect("Engine.ini availability");
        report.status
    }

    assert_eq!(
        report_status(AddonKind::RenoDx, false),
        crate::addons::engine_config::service::EngineConfigStatus::NeedsRepair,
        "a stable RenoDX journal is visible without a local InstalledAddon row"
    );
    assert_eq!(
        report_status(AddonKind::RenoDx, true),
        crate::addons::engine_config::service::EngineConfigStatus::RecoveryRequired,
        "a Pending RenoDX journal remains visible without a local row"
    );
    assert_eq!(
        report_status(AddonKind::Luma, false),
        crate::addons::engine_config::service::EngineConfigStatus::Ready,
        "a Luma owner is not interpreted as RenoDX ownership"
    );
}

#[cfg(windows)]
fn engine_ini_guidance() -> crate::addons::renodx::types::RenoDxGuidance {
    use crate::addons::renodx::types::{
        RenoDxEngineIniEntry, RenoDxEngineIniRecipe, RenoDxEngineIniSection, RenoDxGuidanceKind,
    };

    crate::addons::renodx::types::RenoDxGuidance {
        id: "availability.engine.owner".to_owned(),
        kind: RenoDxGuidanceKind::EngineIni,
        message_id: "availability.engine.owner".to_owned(),
        fallback_text: "Apply the reviewed Engine.ini setting.".to_owned(),
        code: Some("[SystemSettings]\nr.AvailabilityOwner=1".to_owned()),
        settings: Vec::new(),
        engine_ini: Some(RenoDxEngineIniRecipe {
            schema_version: 1,
            revision: 1,
            sections: vec![RenoDxEngineIniSection {
                name: "SystemSettings".to_owned(),
                entries: vec![RenoDxEngineIniEntry {
                    key: "r.AvailabilityOwner".to_owned(),
                    value: "1".to_owned(),
                }],
            }],
        }),
        url: None,
        condition: None,
    }
}

#[test]
#[cfg(windows)]
fn availability_reports_the_framework_install_sentinel() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:1091599").expect("game id");
    let exe_path = game_dir.path().join("Game.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");

    let identity = GameIdentity::new(game_id.clone(), "RenoDX Sentinel", Launcher::Steam)
        .expect("identity")
        .with_external_id("1091599")
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
        "sentinel",
        "sentinel",
        Architecture::X64,
        crate::addons::renodx::types::Status::Working,
        vec![rule(
            crate::addons::renodx::types::MatchKind::SteamAppid,
            "1091599",
            100,
        )],
    )]);
    let reshade_sources = crate::addons::renodx::test_support::reshade_sources();
    let marker = game_dir.path().join("renderpilot-renodx-install.lock");
    std::fs::write(&marker, b"").expect("write sentinel");

    let torn =
        availability(&context, &manifest, &reshade_sources, &game_id).expect("torn availability");
    assert!(torn.install_torn);

    std::fs::remove_file(marker).expect("remove sentinel");
    let clean =
        availability(&context, &manifest, &reshade_sources, &game_id).expect("clean availability");
    assert!(!clean.install_torn);
    assert!(
        !clean.has_persisted_record,
        "there is no persisted RenoDX record"
    );
}

#[test]
#[cfg(windows)]
fn availability_reports_architecture_incompatible_persisted_record_without_promoting_install_state()
{
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = GameId::new("steam:49520").expect("game id");
    let exe_path = game_dir.path().join("Borderlands2.exe");
    let host_path = game_dir.path().join("d3d9.dll");
    let addon_path = game_dir.path().join("renodx-borderlands2.addon64");

    // Borderlands 2 is a 32-bit game, while the old local receipt points to
    // its 64-bit RenoDX add-on and 64-bit ReShade host.
    std::fs::write(&exe_path, build_pe_with_exports(0x014c, 0x010b, &[])).expect("write x86 exe");
    std::fs::write(&host_path, full_reshade_host_bytes()).expect("write x64 ReShade host");
    std::fs::write(&addon_path, b"persisted x64 RenoDX add-on").expect("write add-on");
    seed_availability_game(&context, &game_id, "49520", game_dir.path(), &exe_path);

    let host_path_ref =
        PathRef::new(host_path.to_string_lossy().replace('\\', "/")).expect("host path");
    let addon_record = InstalledAddon::new(
        game_id.clone(),
        AddonKind::RenoDx,
        PathRef::new(addon_path.to_string_lossy().replace('\\', "/")).expect("add-on path"),
    )
    .with_host_kind(InstalledAddonHostKind::Proxy)
    .with_created_file(host_path_ref);
    context
        .storage()
        .upsert_installed_addon(&addon_record)
        .expect("persist inactive local owner");

    assert!(
        records::active_record_of_kind(&context, &game_id, AddonKind::RenoDx)
            .expect("active record query")
            .is_none(),
        "the x64 host cannot be active for this x86 game"
    );

    let report = availability(
        &context,
        &manifest(Vec::new()),
        &crate::addons::renodx::test_support::reshade_sources(),
        &game_id,
    )
    .expect("availability");

    assert_eq!(report.state, RenoDxInstallState::NotInstalled);
    assert!(
        report.has_persisted_record,
        "the RenoDX record is persisted"
    );
    assert!(!report.install_requires_shared_vulkan);
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
    assert!(
        report.has_persisted_record,
        "an active install also has a persisted record"
    );
    assert!(report.actions.install.is_none());
    assert!(report.actions.use_existing.is_some());
    let record = records::record_of_kind(&context, &game_id, AddonKind::RenoDx)
        .expect("read adopted record")
        .expect("adopted record");
    assert!(record.installed_at().is_some());
    assert_eq!(record.addon_version(), None);
}

#[tokio::test]
#[cfg(windows)]
async fn availability_load_allows_install_with_an_inactive_external_luma_owner() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let external_dir = tempdir().expect("external AddonPath");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = renderpilot_domain::GameId::new("steam:1091513").expect("game id");
    let exe_path = game_dir.path().join("Game.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    seed_availability_game(&context, &game_id, "1091500", game_dir.path(), &exe_path);
    let owner = seed_external_luma_owner(&context, &game_id, external_dir.path());
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

    let report = load_availability(&context, &manifest, &reshade_sources, &game_id)
        .await
        .expect("availability");

    assert!(matches!(
        report.outcome,
        AvailabilityOutcome::Installable { .. }
    ));
    assert_eq!(report.state, RenoDxInstallState::NotInstalled);
    assert!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("read external owner")
            .is_some_and(|stored| stored.eq_ignoring_persistence_timestamps(&owner)),
        "availability leaves the raw external-owner receipt unchanged"
    );
}

#[tokio::test]
#[cfg(windows)]
async fn availability_keeps_unrelated_unmanaged_luma_files_blocking_an_external_owner() {
    let db_dir = tempdir().expect("db dir");
    let game_dir = tempdir().expect("game dir");
    let external_dir = tempdir().expect("external AddonPath");
    let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
    let game_id = renderpilot_domain::GameId::new("steam:1091514").expect("game id");
    let exe_path = game_dir.path().join("Game.exe");
    std::fs::write(
        &exe_path,
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
    )
    .expect("write exe");
    seed_availability_game(&context, &game_id, "1091500", game_dir.path(), &exe_path);
    let owner = seed_external_luma_owner(&context, &game_id, external_dir.path());
    std::fs::write(game_dir.path().join("Luma-untracked.addon"), b"unowned")
        .expect("write unrelated peer file");
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

    let report = load_availability(&context, &manifest, &reshade_sources, &game_id)
        .await
        .expect("availability");

    assert!(matches!(
        report.outcome,
        AvailabilityOutcome::BlockedByOtherAddon {
            other_kind: AddonKind::Luma,
            unmanaged: true,
        }
    ));
    assert_eq!(report.state, RenoDxInstallState::NotInstalled);
    assert!(
        context
            .storage()
            .get_installed_addon(&game_id)
            .expect("read external owner")
            .is_some_and(|stored| stored.eq_ignoring_persistence_timestamps(&owner)),
        "the blocked peer receipt remains unchanged"
    );
}

#[tokio::test]
#[cfg(windows)]
async fn availability_does_not_adopt_orphan_over_existing_proxy_topology() {
    for (game_id_text, with_external_owner) in [("steam:1091502", false), ("steam:1091515", true)] {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new(game_id_text).expect("game id");
        let exe_path = game_dir.path().join("Game.exe");

        std::fs::write(
            &exe_path,
            build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
        )
        .expect("write exe");
        std::fs::write(game_dir.path().join("dxgi.dll"), full_reshade_host_bytes())
            .expect("write host");
        std::fs::write(game_dir.path().join("renodx-cp2077.addon64"), b"addon")
            .expect("write addon");
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
            PathRef::new(game_dir.path().to_string_lossy().replace('\\', "/"))
                .expect("install path"),
        )
        .with_executable_candidate(
            PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
        );
        context.storage().upsert_game(&game).expect("seed game");

        let external_owner = if with_external_owner {
            let external_dir = tempdir().expect("external AddonPath");
            Some(seed_external_luma_owner(
                &context,
                &game_id,
                external_dir.path(),
            ))
        } else {
            None
        };

        let root_slot = PathRef::new(
            game_dir
                .path()
                .join("dxgi.dll")
                .to_string_lossy()
                .replace('\\', "/"),
        )
        .expect("root slot");
        let topology = GameProxyTopology {
            id: "topology:availability-aggregate-owner".to_owned(),
            game_id: game_id.clone(),
            root_slot: root_slot.clone(),
            outer: ProxyLink {
                implementation: ProxyImplementation::OptiScaler,
                path: root_slot,
                receipt: FileReceipt::owned(
                    "optiscaler:availability",
                    Sha256Hash::new("a".repeat(64)).expect("digest"),
                )
                .expect("receipt"),
            },
            downstream: None,
            downstream_origin: None,
            root_prestate: ProxyRootPrestate::Absent,
        };
        let connection = rusqlite::Connection::open(db_dir.path().join("catalog.sqlite"))
            .expect("fixture connection");
        connection
        .execute(
            "INSERT INTO game_proxy_topologies (game_id, id, topology_json) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                game_id.as_str(),
                topology.id.as_str(),
                serde_json::to_string(&topology).expect("topology json")
            ],
        )
        .expect("topology");

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
            .expect("availability remains queryable");
        assert!(matches!(report.state, RenoDxInstallState::NotInstalled));
        let raw_owner = context
            .storage()
            .get_installed_addon(&game_id)
            .expect("get record");
        if let Some(external_owner) = external_owner {
            assert!(matches!(
                report.outcome,
                AvailabilityOutcome::BlockedByOtherAddon {
                    other_kind: AddonKind::Luma,
                    unmanaged: false,
                }
            ));
            assert!(
                raw_owner.is_some_and(|stored| {
                    stored.eq_ignoring_persistence_timestamps(&external_owner)
                }),
                "the external peer receipt remains authoritative with a proxy topology"
            );
        } else {
            assert!(raw_owner.is_none(), "the original fixture has no raw owner");
        }
        assert!(
            records::record_of_kind(&context, &game_id, AddonKind::RenoDx)
                .expect("get RenoDX record")
                .is_none(),
            "the local orphan is not adopted over the topology"
        );
        assert_eq!(
            context
                .storage()
                .get_proxy_topology(&game_id)
                .expect("get topology"),
            Some(topology)
        );
    }
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
