use renderpilot_domain::{Architecture, GraphicsApi};

use super::validate_manifest;
use crate::addons::luma::test_support::{manifest, rule, sample_dgvoodoo_requirement, title};
use crate::addons::luma::types::{
    GENERIC_UNREAL_ASSET, LumaCategory, LumaEngine, LumaExternalRequirement, LumaFeatureStatus,
    LumaFeatures, LumaManifest, LumaProfile, Status,
};
use crate::addons::matching::MatchKind;
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
fn valid_manifest_passes() {
    assert!(validate_manifest(&one_title_manifest()).is_ok());
}

#[test]
fn blacklist_category_requires_a_reason() {
    let mut m = one_title_manifest();
    m.titles[0].category = LumaCategory::Blacklist {
        message: crate::addons::CatalogMessage::new("", "Reviewed fallback"),
    };
    assert!(validate_manifest(&m).is_err());
}

#[test]
fn blacklist_category_with_a_reason_passes() {
    let mut m = one_title_manifest();
    m.titles[0].category = LumaCategory::Blacklist {
        message: crate::addons::CatalogMessage::new(
            "luma.reason.needs_dgvoodoo",
            "Requires dgVoodoo2",
        ),
    };
    assert!(validate_manifest(&m).is_ok());
}

#[test]
fn dgvoodoo_external_requirement_passes() {
    let mut m = one_title_manifest();
    m.titles[0].external_requirement = Some(sample_dgvoodoo_requirement());

    assert!(validate_manifest(&m).is_ok());
}

#[test]
fn external_requirement_rejects_unsupported_proxy_slot() {
    let mut m = one_title_manifest();
    let mut requirement = sample_dgvoodoo_requirement();
    match &mut requirement {
        LumaExternalRequirement::Dgvoodoo2 {
            reshade_proxy_dll, ..
        } => {
            *reshade_proxy_dll = "user32.dll".to_owned();
        }
    }
    m.titles[0].external_requirement = Some(requirement);

    assert!(validate_manifest(&m).is_err());
}

#[test]
fn external_requirement_rejects_unsafe_install_map_path() {
    let mut m = one_title_manifest();
    let mut requirement = sample_dgvoodoo_requirement();
    match &mut requirement {
        LumaExternalRequirement::Dgvoodoo2 { install_map, .. } => {
            install_map[0].source = "../D3D9.dll".to_owned();
        }
    }
    m.titles[0].external_requirement = Some(requirement);

    assert!(validate_manifest(&m).is_err());
}

#[test]
fn external_requirement_rejects_config_target_conflict() {
    let mut m = one_title_manifest();
    let mut requirement = sample_dgvoodoo_requirement();
    match &mut requirement {
        LumaExternalRequirement::Dgvoodoo2 {
            install_map,
            config_file,
            ..
        } => {
            install_map[0].dest = "dgVoodoo.conf".to_owned();
            *config_file = "dgVoodoo.conf".to_owned();
        }
    }
    m.titles[0].external_requirement = Some(requirement);

    assert!(validate_manifest(&m).is_err());
}

#[test]
fn external_requirement_rejects_proxy_slot_install_map_conflict() {
    let mut m = one_title_manifest();
    let mut requirement = sample_dgvoodoo_requirement();
    match &mut requirement {
        LumaExternalRequirement::Dgvoodoo2 {
            install_map,
            reshade_proxy_dll,
            ..
        } => {
            // Case-insensitive collision with the ReShade proxy slot.
            *reshade_proxy_dll = "d3d9.dll".to_owned();
            install_map[0].dest = "D3D9.dll".to_owned();
        }
    }
    m.titles[0].external_requirement = Some(requirement);

    assert!(validate_manifest(&m).is_err());
}

#[test]
fn external_requirement_rejects_non_directx_accepted_api() {
    let mut m = one_title_manifest();
    let mut requirement = sample_dgvoodoo_requirement();
    match &mut requirement {
        LumaExternalRequirement::Dgvoodoo2 {
            accepted_detected_apis,
            ..
        } => {
            *accepted_detected_apis = vec![GraphicsApi::Vulkan];
        }
    }
    m.titles[0].external_requirement = Some(requirement);

    assert!(validate_manifest(&m).is_err());
}

#[test]
fn asset_must_start_with_luma_prefix_and_zip_suffix() {
    let mut m = one_title_manifest();
    m.titles[0].asset = "Dishonored_2.zip".to_owned();
    assert!(validate_manifest(&m).is_err());

    m.titles[0].asset = "Luma-Dishonored_2.rar".to_owned();
    assert!(validate_manifest(&m).is_err());
}

#[test]
fn asset_rejects_test_and_dev_build_markers() {
    let mut m = one_title_manifest();
    m.titles[0].asset = "Luma-Dishonored_2-Test.zip".to_owned();
    assert!(validate_manifest(&m).is_err());

    m.titles[0].asset = "Luma-Dishonored_2-Dev.zip".to_owned();
    assert!(validate_manifest(&m).is_err());
}

#[test]
fn asset_x32_suffix_must_agree_with_declared_arch() {
    let mut m = one_title_manifest();
    m.titles[0].asset = "Luma-Dishonored_2-x32.zip".to_owned();
    // arch is still X64 -- mismatch.
    assert!(validate_manifest(&m).is_err());

    m.titles[0].arch = Architecture::X86;
    assert!(validate_manifest(&m).is_ok());

    m.titles[0].asset = "Luma-Dishonored_2.zip".to_owned();
    // Now X86 but no -x32 suffix -- mismatch.
    assert!(validate_manifest(&m).is_err());
}

#[test]
fn generic_ue_profiles_require_features() {
    let mut m = one_title_manifest();
    m.titles[0].asset = GENERIC_UNREAL_ASSET.to_owned();
    m.titles[0].profile = LumaProfile::Engine {
        engine: LumaEngine::Unreal,
    };
    assert!(validate_manifest(&m).is_err());
    m.titles[0].features = Some(LumaFeatures {
        dlss_fsr: LumaFeatureStatus::Unknown,
        hdr: LumaFeatureStatus::Unknown,
    });
    assert!(validate_manifest(&m).is_ok());
}

#[test]
fn addon_file_must_be_a_safe_root_luma_addon() {
    let mut m = one_title_manifest();
    m.titles[0].addon_file = "nested/Luma-Dishonored 2.addon".to_owned();
    assert!(validate_manifest(&m).is_err());

    m.titles[0].addon_file = "Luma-Dishonored 2.dll".to_owned();
    assert!(validate_manifest(&m).is_err());
}

#[test]
fn addon_file_prefix_and_suffix_are_ascii_case_insensitive() {
    let mut m = one_title_manifest();
    m.titles[0].addon_file = "lUmA-Dishonored 2.AdDoN".to_owned();
    assert!(validate_manifest(&m).is_ok());
}

#[test]
fn addon_file_rejects_blank_name() {
    let mut m = one_title_manifest();
    m.titles[0].addon_file = "Luma-   .addon".to_owned();
    assert!(validate_manifest(&m).is_err());
}

#[test]
fn asset_prefix_and_suffix_are_case_sensitive() {
    let mut m = one_title_manifest();
    m.titles[0].asset = "luma-Dishonored_2.zip".to_owned();
    assert!(validate_manifest(&m).is_err());
}

#[test]
fn one_release_asset_cannot_claim_multiple_payload_names() {
    let mut m = one_title_manifest();
    let mut second = title(
        "other-title",
        "Luma-Dishonored_2.zip",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1", 100)],
    );
    second.addon_file = "Luma-Other.addon".to_owned();
    m.titles.push(second);

    assert!(validate_manifest(&m).is_err());
}

#[test]
fn duplicate_title_ids_are_rejected() {
    let mut m = one_title_manifest();
    let mut second = title(
        "dishonored-2",
        "Luma-Other.zip",
        Architecture::X64,
        Status::Working,
        vec![rule(MatchKind::SteamAppid, "1", 100)],
    );
    second.name = "Other".to_owned();
    m.titles.push(second);
    assert!(validate_manifest(&m).is_err());
}

#[test]
fn min_reshade_version_must_be_dotted_triple() {
    let mut m = one_title_manifest();
    m.min_reshade_version = "not-a-version".to_owned();
    assert!(validate_manifest(&m).is_err());
}

#[test]
fn match_rule_requires_a_value_unless_generic() {
    let mut m = one_title_manifest();
    m.titles[0].match_rules = vec![rule(MatchKind::SteamAppid, "", 100)];
    assert!(validate_manifest(&m).is_err());

    m.titles[0].match_rules = vec![rule(MatchKind::Generic, "", 10)];
    assert!(validate_manifest(&m).is_ok());
}

#[test]
fn steam_appid_rule_must_be_a_positive_integer() {
    let mut m = one_title_manifest();
    m.titles[0].match_rules = vec![rule(MatchKind::SteamAppid, "not-a-number", 100)];
    assert!(validate_manifest(&m).is_err());
}
