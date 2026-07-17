use renderpilot_domain::{Architecture, ExeGraphicsInfo, GraphicsApi, Launcher};
use std::assert_matches;

use super::*;
use crate::addons::matching::{IncompatibilityReason, MatchConfidence, MatchFacts};
use crate::addons::renodx::source;
use crate::addons::renodx::test_support::{manifest, rule, title};
use crate::addons::renodx::types::{
    Engine, MatchKind, RenoDxCategory, RenoDxGeneric, RenoDxManifest, RenoDxTitle, Status,
};

fn message(id: &str) -> crate::addons::CatalogMessage {
    crate::addons::CatalogMessage::new(id, "Reviewed catalogue fallback")
}

/// A title carrying a non-default category (external / native-HDR / blacklist).
fn categorized(id: &str, category: RenoDxCategory) -> RenoDxTitle {
    let mut t = title(
        id,
        id,
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    );
    t.category = category;
    t
}

fn facts() -> MatchFacts {
    MatchFacts {
        launcher: Launcher::Steam,
        external_id: Some("1091500".to_owned()),
        exe_file_name: Some("Cyberpunk2077.exe".to_owned()),
        exe_sha256: None,
        engine: None,
        graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64)),
    }
}

#[test]
fn installs_a_verified_steam_match() {
    let m = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    )]);
    match resolve(&m, &facts()) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(
                plan.addon_url,
                source::addon_url("cp2077", Architecture::X64)
            );
            assert_eq!(plan.proxy_dll_name, "dxgi.dll");
            assert_eq!(plan.confidence, MatchConfidence::Verified);
        }
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn download_url_overrides_slug_derived_url() {
    // A title with a download_url (third-party host) must resolve to that URL,
    // not the clshortfuse URL derived from the slug.
    let mut t = title(
        "ryza2",
        "ryza2",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    );
    t.download_url = Some("https://marat569.github.io/renodx/renodx-ryza2.addon64".to_owned());
    let m = manifest(vec![t]);
    match resolve(&m, &facts()) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(
                plan.addon_url,
                "https://marat569.github.io/renodx/renodx-ryza2.addon64"
            );
            // The slug is still used for the on-disk file name.
            assert_eq!(plan.slug, "ryza2");
        }
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn title_slug_matching_a_generic_uses_the_generics_explicit_url() {
    // A per-game title curated onto a universal engine add-on (matched by
    // `slug`, e.g. a Unity game routed to the `unityengine` generic) must
    // resolve through that generic's explicit host — the clshortfuse URL
    // derived from the same slug may not exist (see `title_addon_url`).
    let t = title(
        "some-curated-unity-title",
        "unityengine",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    );
    let mut m = manifest(vec![t]);
    m.generics.push(RenoDxGeneric {
        engine: crate::addons::renodx::types::Engine::Unity,
        status: Status::Working,
        slug: Some("unityengine".to_owned()),
        url64: Some(
            "https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64"
                .to_owned(),
        ),
        url32: None,
        message: message("renodx.generic.unity"),
    });

    match resolve(&m, &facts()) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(
                plan.addon_url,
                "https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64"
            );
            assert_eq!(plan.slug, "unityengine");
        }
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn title_download_url_still_wins_over_a_matching_generic() {
    let mut t = title(
        "curated-unity-game",
        "unityengine",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    );
    t.download_url = Some("https://example.com/renodx-unityengine.addon64".to_owned());
    let mut m = manifest(vec![t]);
    m.generics.push(RenoDxGeneric {
        engine: crate::addons::renodx::types::Engine::Unity,
        status: Status::Working,
        slug: Some("unityengine".to_owned()),
        url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64".to_owned()),
        url32: None,
        message: message("renodx.generic.unity"),
    });

    match resolve(&m, &facts()) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(
                plan.addon_url,
                "https://example.com/renodx-unityengine.addon64"
            );
        }
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn title_slug_with_no_matching_generic_falls_back_to_clshortfuse() {
    let m = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    )]);
    match resolve(&m, &facts()) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(
                plan.addon_url,
                source::addon_url("cp2077", Architecture::X64)
            );
        }
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn construction_status_is_experimental() {
    let m = manifest(vec![title(
        "wip",
        "wip",
        Architecture::X64,
        Status::Construction,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    )]);
    match resolve(&m, &facts()) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(plan.confidence, MatchConfidence::Experimental);
        }
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn no_match_returns_no_match() {
    let m = manifest(vec![title(
        "other",
        "other",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "42", 100)],
    )]);
    assert_matches!(resolve(&m, &facts()), RenoDxResolution::NoMatch);
}

#[test]
fn curated_title_installs_despite_inconclusive_detection() {
    // PE-import detection returns empty for games that load Direct3D
    // dynamically; a curated title must still install — the proxy defaults to
    // dxgi and the architecture comes from the title, not from detection.
    let m = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    )]);
    let mut facts = facts();
    facts.graphics = ExeGraphicsInfo::new(Vec::new(), None);
    match resolve(&m, &facts) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(plan.proxy_dll_name, "dxgi.dll");
            assert_eq!(plan.arch, Architecture::X64);
        }
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn confirmed_vulkan_curated_title_installs_via_the_vulkan_layer() {
    // A confirmed Vulkan renderer is now hosted by the shared Vulkan layer, so a
    // curated title installs (host_kind = Vulkan, no proxy DLL) rather than being
    // declined as it was before.
    let m = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    )]);
    let mut facts = facts();
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
    match resolve(&m, &facts) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(
                plan.host_kind,
                crate::addons::reshade::proxy::HostKind::Vulkan
            );
            assert!(plan.proxy_dll_name.is_empty());
        }
        other => panic!("expected installable via Vulkan, got {other:?}"),
    }
}

#[test]
fn confirmed_opengl_curated_title_is_declined() {
    // OpenGL has no host RenoDX can drive, so even a curated title is declined.
    let m = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    )]);
    let mut facts = facts();
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
    assert_matches!(
        resolve(&m, &facts),
        RenoDxResolution::Incompatible {
            reason: IncompatibilityReason::ApiUnsupported { .. },
        }
    );
}

#[test]
fn matches_by_exe_name_on_a_non_steam_launcher() {
    // A GOG/Epic/Manual install has no Steam appid; the curated title still
    // resolves through its launcher-agnostic exe_name rule (tier 70).
    let m = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        Status::Working,
        vec![
            rule(MatchKind::SteamAppid, "1091500", 100),
            rule(MatchKind::ExeName, "Cyberpunk2077.exe", 70),
        ],
    )]);
    let mut facts = facts();
    facts.launcher = Launcher::Manual;
    facts.external_id = None;
    facts.exe_file_name = Some("Cyberpunk2077.exe".to_owned());
    match resolve(&m, &facts) {
        RenoDxResolution::Installable(plan) => assert_eq!(plan.slug, "cp2077"),
        other => panic!("expected installable by exe, got {other:?}"),
    }
}

#[test]
fn proxy_comes_from_the_imported_dll_not_a_blind_default() {
    // A D3D9 game must get the d3d9.dll proxy, not the dxgi.dll default.
    let m = manifest(vec![title(
        "g",
        "g",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    )]);
    let mut facts = facts();
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D9], Some(Architecture::X64))
        .with_graphics_dlls(vec!["d3d9.dll".to_owned()]);
    match resolve(&m, &facts) {
        RenoDxResolution::Installable(plan) => assert_eq!(plan.proxy_dll_name, "d3d9.dll"),
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn proxy_override_wins_over_detection() {
    let mut t = title(
        "g",
        "g",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    );
    t.proxy_dll_override = Some("dinput8.dll".to_owned());
    let m = manifest(vec![t]);
    match resolve(&m, &facts()) {
        RenoDxResolution::Installable(plan) => assert_eq!(plan.proxy_dll_name, "dinput8.dll"),
        other => panic!("expected installable, got {other:?}"),
    }
}

#[test]
fn engine_generic_fallback_is_untested() {
    let mut m = manifest(vec![title(
        "other",
        "other",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "42", 100)],
    )]);
    m.generics = vec![RenoDxGeneric {
        engine: Engine::Unreal,
        status: Status::Unknown,
        slug: Some("_univ".to_owned()),
        url64: None,
        url32: None,
        message: message("renodx.generic.universal"),
    }];
    let mut facts = facts();
    facts.external_id = Some("999".to_owned());
    facts.engine = Some(Engine::Unreal);

    match resolve(&m, &facts) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(plan.confidence, MatchConfidence::Untested);
            assert_eq!(
                plan.generic_profile.as_ref().map(|profile| profile.engine),
                Some(Engine::Unreal)
            );
            assert_eq!(
                plan.addon_url,
                source::addon_url("_univ", Architecture::X64)
            );
        }
        other => panic!("expected generic installable, got {other:?}"),
    }
}

#[test]
fn engine_generic_uses_manifest_slug_for_local_identity_with_explicit_url() {
    let mut m = manifest(vec![]);
    m.generics = vec![RenoDxGeneric {
        engine: Engine::Unity,
        status: Status::Working,
        slug: Some("unityengine".to_owned()),
        url64: Some("https://example/renodx-unityengine.addon64".to_owned()),
        url32: Some("https://example/renodx-unityengine.addon32".to_owned()),
        message: message("renodx.generic.unity"),
    }];
    let mut facts = facts();
    facts.external_id = Some("999".to_owned());
    facts.engine = Some(Engine::Unity);

    match resolve(&m, &facts) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(plan.confidence, MatchConfidence::Verified);
            assert_eq!(plan.slug, "unityengine");
            assert_eq!(
                plan.generic_profile.as_ref().map(|profile| profile.engine),
                Some(Engine::Unity)
            );
            assert_eq!(plan.addon_url, "https://example/renodx-unityengine.addon64");
        }
        other => panic!("expected generic installable, got {other:?}"),
    }
}

#[test]
fn engine_generic_installs_on_inconclusive_detection() {
    // A detected engine with no curated title and empty graphics (dynamic
    // Direct3D loading) still gets the engine generic — the engine signal
    // implies a DirectX renderer on Windows. (The Tainted Grail / Unity case.)
    let mut m = manifest(vec![]);
    m.generics = vec![RenoDxGeneric {
        engine: Engine::Unreal,
        status: Status::Unknown,
        slug: Some("_univ".to_owned()),
        url64: None,
        url32: None,
        message: message("renodx.generic.universal"),
    }];
    let mut facts = facts();
    facts.engine = Some(Engine::Unreal);
    facts.graphics = ExeGraphicsInfo::new(Vec::new(), Some(Architecture::X64));
    match resolve(&m, &facts) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(plan.proxy_dll_name, "dxgi.dll");
            assert_eq!(plan.confidence, MatchConfidence::Untested);
        }
        other => panic!("expected generic installable, got {other:?}"),
    }
}

#[test]
fn engine_generic_installs_vulkan_and_declines_opengl() {
    // An engine match with a confirmed Vulkan renderer now installs the generic
    // via the shared Vulkan layer; a confirmed OpenGL renderer is still declined.
    let mut m = manifest(vec![]);
    m.generics = vec![RenoDxGeneric {
        engine: Engine::Unreal,
        status: Status::Unknown,
        slug: Some("_univ".to_owned()),
        url64: None,
        url32: None,
        message: message("renodx.generic.universal"),
    }];
    let mut facts = facts();
    facts.engine = Some(Engine::Unreal);

    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
    match resolve(&m, &facts) {
        RenoDxResolution::Installable(plan) => {
            assert_eq!(
                plan.host_kind,
                crate::addons::reshade::proxy::HostKind::Vulkan
            );
            assert_eq!(plan.confidence, MatchConfidence::Untested);
        }
        other => panic!("expected generic Vulkan install, got {other:?}"),
    }

    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
    assert_matches!(
        resolve(&m, &facts),
        RenoDxResolution::Incompatible {
            reason: IncompatibilityReason::ApiUnsupported { .. },
        }
    );
}

fn external_manifest() -> RenoDxManifest {
    let mut t = title(
        "ext.game",
        "extslug",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    );
    t.category = RenoDxCategory::External {
        url: "https://discord.gg/example".to_owned(),
        message: message("renodx.external.discord"),
    };
    manifest(vec![t])
}

#[test]
fn external_vulkan_title_offers_a_vulkan_file_install() {
    // An external title whose renderer is confirmed Vulkan still shows its link
    // (e.g. RDR2's Discord) AND now offers a file-install hosted by the global
    // Vulkan layer (host_kind = Vulkan).
    let m = external_manifest();
    let mut facts = facts();
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
    match resolve(&m, &facts) {
        RenoDxResolution::External {
            file_install, url, ..
        } => {
            let fi = file_install.expect("a Vulkan external is file-installable via the layer");
            assert_eq!(
                fi.host_kind,
                crate::addons::reshade::proxy::HostKind::Vulkan
            );
            assert_eq!(url, "https://discord.gg/example");
        }
        other => panic!("expected external link, got {other:?}"),
    }
    let plan = resolve_external_install(&m, &facts).expect("external vulkan install plan");
    assert_eq!(
        plan.host_kind,
        crate::addons::reshade::proxy::HostKind::Vulkan
    );
    assert!(plan.proxy_dll_name.is_empty());
}

#[test]
fn external_opengl_title_keeps_link_without_a_file_install() {
    // OpenGL has no host RenoDX can drive: the external add-on stays link-only.
    let m = external_manifest();
    let mut facts = facts();
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
    match resolve(&m, &facts) {
        RenoDxResolution::External { file_install, .. } => assert!(file_install.is_none()),
        other => panic!("expected external link, got {other:?}"),
    }
    assert!(resolve_external_install(&m, &facts).is_none());
}

#[test]
fn compatible_external_title_offers_file_install() {
    let m = external_manifest();
    match resolve(&m, &facts()) {
        RenoDxResolution::External { file_install, .. } => {
            let fi = file_install.expect("compatible external is file-installable");
            assert_eq!(fi.confidence, MatchConfidence::Verified);
        }
        other => panic!("expected external, got {other:?}"),
    }
    let plan = resolve_external_install(&m, &facts()).expect("external install plan");
    assert_eq!(plan.slug, "extslug");
    assert_eq!(plan.proxy_dll_name, "dxgi.dll");
}

#[test]
fn file_installable_for_directx_inconclusive_and_vulkan_but_not_opengl() {
    let mut facts = facts();
    // A confirmed Direct3D renderer is file-installable (proxy).
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64));
    assert!(file_installable(&facts));
    // An inconclusive read still allows it (defaults to a proxy).
    facts.graphics = ExeGraphicsInfo::new(Vec::new(), None);
    assert!(file_installable(&facts));
    // A confirmed Vulkan renderer is file-installable via the global layer.
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
    assert!(file_installable(&facts));
    // A confirmed OpenGL renderer is not.
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
    assert!(!file_installable(&facts));
}

#[test]
fn generic_file_install_plan_routes_host_and_declines_opengl() {
    let mut facts = facts();
    // Direct3D → a proxy-hosted generic plan.
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X64));
    let plan = generic_file_install_plan(&facts, Architecture::X64).expect("directx installable");
    assert_eq!(
        plan.host_kind,
        crate::addons::reshade::proxy::HostKind::Proxy
    );
    assert!(plan.slug.is_empty(), "a generic plan has no catalogue slug");
    assert_eq!(plan.arch, Architecture::X64);
    assert!(!plan.proxy_dll_name.is_empty());

    // Vulkan → a layer-hosted generic plan (no proxy DLL).
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
    let vk = generic_file_install_plan(&facts, Architecture::X64).expect("vulkan installable");
    assert_eq!(
        vk.host_kind,
        crate::addons::reshade::proxy::HostKind::Vulkan
    );
    assert!(vk.proxy_dll_name.is_empty());

    // OpenGL → no plan.
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
    assert!(generic_file_install_plan(&facts, Architecture::X64).is_none());
}

#[test]
fn matched_slug_is_the_matching_titles_slug_or_none() {
    let m = manifest(vec![title(
        "cp2077",
        "cp2077",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    )]);
    assert_eq!(matched_slug(&m, &facts()).as_deref(), Some("cp2077"));
    assert_eq!(matched_slug(&manifest(vec![]), &facts()), None);
}

#[test]
fn external_title_is_link_only_on_required_api_mismatch() {
    // An explicit `required_api` the detected (supported) API does not satisfy
    // is the one remaining hard gate: the external add-on stays link-only, with
    // no file install offered. (Inconclusive or merely non-DirectX detection no
    // longer vetoes a curated title.)
    let mut t = title(
        "ext.game",
        "extslug",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    );
    t.category = RenoDxCategory::External {
        url: "https://discord.gg/example".to_owned(),
        message: message("renodx.external.discord"),
    };
    t.compatibility.required_api = vec![GraphicsApi::D3D12];
    let m = manifest(vec![t]);
    let mut facts = facts();
    facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X64));

    match resolve(&m, &facts) {
        RenoDxResolution::External { file_install, .. } => assert!(file_install.is_none()),
        other => panic!("expected external, got {other:?}"),
    }
    assert!(resolve_external_install(&m, &facts).is_none());
}

#[test]
fn resolve_external_install_rejects_non_external_title() {
    let m = manifest(vec![title(
        "plain",
        "plain",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1091500", 100)],
    )]);
    assert!(resolve_external_install(&m, &facts()).is_none());
}

#[test]
fn blacklist_category_yields_unsupported() {
    let m = manifest(vec![categorized(
        "blk",
        RenoDxCategory::Blacklist {
            message: message("renodx.reason.broken"),
        },
    )]);
    match resolve(&m, &facts()) {
        RenoDxResolution::Blacklisted { message } => {
            assert_eq!(message.id, "renodx.reason.broken");
            assert_eq!(message.fallback_text, "Reviewed catalogue fallback");
        }
        other => panic!("expected unsupported, got {other:?}"),
    }
    // A blacklisted game is never offered a file install.
    assert!(resolve_external_install(&m, &facts()).is_none());
}

#[test]
fn native_hdr_category_yields_native_hdr() {
    let m = manifest(vec![categorized("nh", RenoDxCategory::NativeHdr)]);
    assert_matches!(resolve(&m, &facts()), RenoDxResolution::NativeHdr);
    assert!(resolve_external_install(&m, &facts()).is_none());
}
