use super::*;
use crate::addons::luma::test_support::{manifest, rule, sample_dgvoodoo_requirement, title};
use crate::addons::luma::types::{LumaEngine, LumaProfile, Status};
use crate::addons::matching::{Engine, MatchFacts, MatchKind};
use renderpilot_domain::{ExeGraphicsInfo, Launcher};

fn facts() -> MatchFacts {
    MatchFacts {
        launcher: Launcher::Steam,
        external_id: Some("403640".to_owned()),
        exe_file_name: Some("Dishonored2.exe".to_owned()),
        exe_sha256: None,
        engine: None,
        graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X64))
            .with_graphics_dlls(vec!["dxgi.dll".to_owned()]),
    }
}

fn one_title_manifest() -> LumaManifest {
    manifest(vec![title(
        "dishonored-2",
        "Luma-Dishonored_2.zip",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "403640", 100)],
    )])
}

#[test]
fn installs_a_verified_steam_match() {
    match resolve(&one_title_manifest(), &facts()) {
        LumaResolution::Installable(plan) => {
            assert_eq!(plan.asset, "Luma-Dishonored_2.zip");
            assert_eq!(plan.proxy_dll_name, "dxgi.dll");
            assert_eq!(plan.confidence, MatchConfidence::Verified);
            assert!(!plan.profile.is_engine());
        }
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn no_match_returns_no_match() {
    let m = manifest(vec![title(
        "other",
        "Luma-Other.zip",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "42", 100)],
    )]);
    assert!(matches!(resolve(&m, &facts()), LumaResolution::NoMatch));
}

#[test]
fn a_detected_engine_never_falls_back_to_a_generic_title_unlike_renodx() {
    // E.18: Luma has no engine-fallback concept at all (see the module doc)
    // — a detected engine with no matching per-game/Generic-Mod title must
    // stay `NoMatch`, unlike RenoDX's own engine-generic resolver.
    let m = manifest(vec![title(
        "unrelated-game",
        "Luma-Unrelated.zip",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "42", 100)],
    )]);
    let mut f = facts();
    f.engine = Some(Engine::Unreal);
    f.external_id = Some("999999".to_owned());

    assert!(matches!(resolve(&m, &f), LumaResolution::NoMatch));
}

#[test]
fn confirmed_d3d11_installs_and_inconclusive_defaults_to_installable() {
    let m = one_title_manifest();
    let mut f = facts();
    f.graphics = ExeGraphicsInfo::new(Vec::new(), Some(Architecture::X64));
    match resolve(&m, &f) {
        LumaResolution::Installable(plan) => assert_eq!(plan.proxy_dll_name, "dxgi.dll"),
        other => panic!("expected installable on inconclusive read, got {other:?}"),
    }
}

#[test]
fn confirmed_vulkan_is_incompatible_even_when_curated() {
    let m = one_title_manifest();
    let mut f = facts();
    f.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
    assert!(matches!(
        resolve(&m, &f),
        LumaResolution::Incompatible {
            reason: IncompatibilityReason::ApiUnsupported { .. },
        }
    ));
}

#[test]
fn confirmed_opengl_is_incompatible() {
    let m = one_title_manifest();
    let mut f = facts();
    f.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
    assert!(matches!(
        resolve(&m, &f),
        LumaResolution::Incompatible {
            reason: IncompatibilityReason::ApiUnsupported { .. },
        }
    ));
}

#[test]
fn confirmed_d3d12_only_is_incompatible_unlike_renodx() {
    // Luma is DX11-specific: a confirmed *other* DirectX version is declined,
    // even though the very same detection would install fine under RenoDX.
    let m = one_title_manifest();
    let mut f = facts();
    f.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64));
    assert!(matches!(
        resolve(&m, &f),
        LumaResolution::Incompatible {
            reason: IncompatibilityReason::ApiNotAllowed { .. },
        }
    ));
}

#[test]
fn generic_ue_d3d12_is_installable_so_the_manual_dx11_callout_can_be_shown() {
    let mut t = title(
        "generic-ue",
        "Luma-Unreal_Engine.zip",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "403640", 100)],
    );
    t.profile = LumaProfile::Engine {
        engine: LumaEngine::Unreal,
    };
    let m = manifest(vec![t]);
    let mut f = facts();
    f.engine = Some(Engine::Unreal);
    f.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64));

    assert!(matches!(resolve(&m, &f), LumaResolution::Installable(_)));
}

#[test]
fn confirmed_d3d9_is_incompatible_without_an_explicit_external_requirement() {
    let m = one_title_manifest();
    let mut f = facts();
    f.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D9], Some(Architecture::X64));

    assert!(matches!(
        resolve(&m, &f),
        LumaResolution::Incompatible {
            reason: IncompatibilityReason::ApiNotAllowed { .. },
        }
    ));
}

#[test]
fn dgvoodoo_requirement_accepts_d3d9_and_forces_dxgi_proxy() {
    let mut t = title(
        "borderlands-2-and-the-pre-sequel",
        "Luma-Borderlands_2_and_The_Pre-Sequel-x32.zip",
        Architecture::X86,
        Status::Unknown,
        vec![rule(MatchKind::SteamAppid, "49520", 100)],
    );
    t.external_requirement = Some(sample_dgvoodoo_requirement());
    let m = manifest(vec![t]);
    let mut f = facts();
    f.external_id = Some("49520".to_owned());
    f.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D9], Some(Architecture::X86))
        .with_graphics_dlls(vec!["d3d9.dll".to_owned()]);

    match resolve(&m, &f) {
        LumaResolution::Installable(plan) => {
            assert_eq!(plan.proxy_dll_name, "dxgi.dll");
            assert!(matches!(
                plan.external_requirement,
                Some(LumaExternalRequirement::Dgvoodoo2 { .. })
            ));
        }
        other => panic!("expected dgVoodoo-backed installable title, got {other:?}"),
    }
}

#[test]
fn arch_mismatch_is_incompatible() {
    let m = one_title_manifest();
    let mut f = facts();
    f.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X86));
    assert!(matches!(
        resolve(&m, &f),
        LumaResolution::Incompatible {
            reason: IncompatibilityReason::ArchMismatch { .. },
        }
    ));
}

#[test]
fn unknown_arch_is_incompatible() {
    let m = one_title_manifest();
    let mut f = facts();
    f.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], None);
    assert!(matches!(
        resolve(&m, &f),
        LumaResolution::Incompatible {
            reason: IncompatibilityReason::ArchUnknown,
        }
    ));
}

#[test]
fn blacklisted_title_yields_unsupported_without_arch_or_api_gating() {
    let mut t = title(
        "vanquish",
        "Luma-Vanquish.zip",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "403640", 100)],
    );
    t.category = LumaCategory::Blacklist {
        message: crate::addons::CatalogMessage::new(
            "luma.reason.needs_dgvoodoo",
            "Requires dgVoodoo2",
        ),
    };
    let m = manifest(vec![t]);
    // Even an incompatible-looking renderer must still report the blacklist
    // reason, not an API/arch incompatibility.
    let mut f = facts();
    f.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
    match resolve(&m, &f) {
        LumaResolution::Blacklisted { message } => {
            assert_eq!(message.id, "luma.reason.needs_dgvoodoo");
            assert_eq!(message.fallback_text, "Requires dgVoodoo2");
        }
        other => panic!("expected unsupported, got {other:?}"),
    }
}

#[test]
fn construction_status_is_experimental_and_generic_flag_passes_through() {
    let mut t = title(
        "tekken-7",
        "Luma-Generic_Mod.zip",
        Architecture::X64,
        Status::Construction,
        vec![rule(MatchKind::SteamAppid, "403640", 100)],
    );
    t.profile = LumaProfile::Engine {
        engine: LumaEngine::Unity,
    };
    t.launch_args = vec!["-nod3d9ex".to_owned()];
    let m = manifest(vec![t]);
    match resolve(&m, &facts()) {
        LumaResolution::Installable(plan) => {
            assert_eq!(plan.confidence, MatchConfidence::Experimental);
            assert!(plan.profile.is_engine());
            assert_eq!(plan.launch_args, vec!["-nod3d9ex".to_owned()]);
        }
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn matches_by_exe_name_on_a_non_steam_launcher() {
    let m = manifest(vec![title(
        "dishonored-2",
        "Luma-Dishonored_2.zip",
        Architecture::X64,
        Status::Working,
        vec![
            rule(MatchKind::SteamAppid, "403640", 100),
            rule(MatchKind::ExeName, "Dishonored2.exe", 70),
        ],
    )]);
    let mut f = facts();
    f.launcher = Launcher::Manual;
    f.external_id = None;
    match resolve(&m, &f) {
        LumaResolution::Installable(plan) => assert_eq!(plan.asset, "Luma-Dishonored_2.zip"),
        other => panic!("expected installable by exe, got {other:?}"),
    }
}
