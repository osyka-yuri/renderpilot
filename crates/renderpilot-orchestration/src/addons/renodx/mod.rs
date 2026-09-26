//! RenoDX add-on installation subsystem.
//!
//! Introduces the RenoDX (Renovation Engine for DirectX) ReShade HDR add-on into a
//! game and can fully reverse the change. Add-ons are fetched **live from upstream**
//! (clshortfuse.github.io and the engine-generic repos) rather than mirrored, so the
//! manifest is a lightweight overrides + catalogue document with no hashes.
//!
//! The end-to-end flows live in [`use_cases`], built on the [`types`] model, its
//! [`parse_manifest`] validation, the `game_analysis` gatherer, the deterministic
//! `matcher`, `reshade` host orchestration, the `source` URL/host resolver, the
//! `fetch` downloader, and the `install` filesystem engine. The cross-cutting
//! file-safety authority lives in [`crate::file_safety`]. RenoDX-specific policy
//! (which API to target, installability) lives in `policy`, separate from the
//! generic detection facts.

pub(crate) mod dlss_fix;
pub(crate) mod dlss_fix_binding;
/// DTOs
pub mod dto;
mod errors;
mod fetch;
mod game_context;
pub(crate) mod game_participants;
pub(crate) mod install;
pub mod manifest_store;
pub(crate) mod matcher;
pub(crate) mod mutation_targets;
pub(crate) mod peer;
/// Platform infrastructure.
pub mod platform;
pub(crate) mod policy;
mod reconciliation;
pub(crate) mod reshade;
pub(crate) mod reshade_ini;
mod source;
pub(crate) mod tool;
mod tracking;
pub mod types;
/// Use cases.
pub mod use_cases;
mod validate;

pub use platform::vulkan;

/// Progress phase key for post-download finalization (i18n key the frontend looks up).
/// Exposed via the
/// [`finalizing_phase`](crate::addons::tool::AddonTool::finalizing_phase) method and
/// [`crate::addons::progress::emit_tool_finalizing`].
pub(crate) const RENODX_PHASE_FINALIZING: &str = "renodx.phase.finalizing";

#[cfg(test)]
pub(crate) mod test_support;

use renderpilot_domain::Architecture;

use crate::ServiceError;

use self::types::{RenoDxManifest, WireManifestV1, WireManifestV2};
use super::UTF8_BOM;

/// Parses and validates a RenoDX manifest document.
///
/// Strips a leading UTF-8 BOM, deserializes, then runs schema + structural
/// validation, so a returned manifest can be acted on without further checks.
pub fn parse_manifest(bytes: &[u8]) -> Result<RenoDxManifest, ServiceError> {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let wire: WireManifestV1 = serde_json::from_slice(bytes)
        .map_err(|error| errors::failed(format!("failed to parse RenoDX manifest: {error}")))?;
    let manifest = RenoDxManifest::from_wire_v1(wire);
    ensure_wire_schema_version(&manifest, 1, "RenoDX")?;
    validate::validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Parses the strict v2 RenoDX document. v2 is deliberately a separate entry
/// point so malformed v2 data is never silently interpreted as v1.
pub fn parse_manifest_v2(bytes: &[u8]) -> Result<RenoDxManifest, ServiceError> {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let wire: WireManifestV2 = serde_json::from_slice(bytes).map_err(|error| {
        ServiceError::manifest_contract_rejected(
            crate::ManifestContract::RenoDxV2,
            format!("parsing rejected: {error}"),
        )
    })?;
    wire.validate_raw_renodx_config().map_err(|error| {
        ServiceError::manifest_contract_rejected(crate::ManifestContract::RenoDxV2, error)
    })?;
    let mut manifest = RenoDxManifest::from_wire_v2(wire);
    ensure_wire_schema_version(&manifest, 2, "RenoDX v2").map_err(|error| {
        ServiceError::manifest_contract_rejected(
            crate::ManifestContract::RenoDxV2,
            error.to_string(),
        )
    })?;
    validate::validate_manifest(&manifest).map_err(|error| {
        ServiceError::manifest_contract_rejected(
            crate::ManifestContract::RenoDxV2,
            error.to_string(),
        )
    })?;
    // Keep known settings in the temporary normalized form through validation
    // so bad known values and profile combinations still reject the v2 catalog.
    // A title with unsupported settings must never expose a partial config to callers.
    for title in &mut manifest.titles {
        if title.has_unsupported_settings {
            title.renodx_config = None;
        }
    }
    Ok(manifest)
}

fn ensure_wire_schema_version(
    manifest: &RenoDxManifest,
    expected: u32,
    document: &str,
) -> Result<(), ServiceError> {
    if manifest.schema_version != expected {
        return Err(errors::failed(format!(
            "{document} schema version {} does not match parser version {expected}",
            manifest.schema_version
        )));
    }
    Ok(())
}

/// Derives the add-on architecture from an add-on file name's extension
/// (`renodx-<slug>.addon64` → X64, `.addon32` → X86).
#[must_use]
pub(super) fn arch_from_addon_file(name: &str) -> Option<Architecture> {
    let name = name.to_ascii_lowercase();
    if name.ends_with(".addon64") {
        Some(Architecture::X64)
    } else if name.ends_with(".addon32") {
        Some(Architecture::X86)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "schema_version": 1,
        "generated_at": "2026-06-15T00:00:00Z",
        "engine_profiles": [
            { "engine": "unity", "status": "working", "addon": { "slug": "unityengine", "sources": { "x64": "https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64", "x86": "https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon32" } }, "message": { "id": "renodx.generic.unity", "fallback_text": "Uses the shared Unity engine profile." } },
            { "engine": "unreal", "status": "working", "addon": { "slug": "_univ" }, "message": { "id": "renodx.generic.universal", "fallback_text": "Uses the shared Unreal Engine profile." } }
        ],
        "games": [
            {
                "id": "cyberpunk-2077", "name": "Cyberpunk 2077", "architecture": "X64", "status": "working", "addon": { "slug": "cp2077" },
                "match": [{ "kind": "steam_appid", "value": "1091500", "tier": 100 }],
                "constraints": { "conflicts": ["special_k"], "source": "https://example.test/conflict-report" }
            },
            {
                "id": "nexus-game", "name": "Nexus Game", "architecture": "X64", "status": "working", "addon": { "slug": "nexusgame" },
                "availability": { "kind": "external", "url": "https://www.nexusmods.com/x", "message": { "id": "renodx.external.nexus", "fallback_text": "Get the add-on from Nexus Mods." } },
                "match": [{ "kind": "steam_appid", "value": "424242", "tier": 100 }]
            }
        ]
    }"#;

    const SAMPLE_V2: &str = r#"{
        "schema_version": 2,
        "generated_at": "2026-09-15T00:00:00Z",
        "games": [{
            "id": "black-myth-wukong",
            "name": "Black Myth: Wukong",
            "architecture": "X64",
            "status": "working",
            "match": [{ "kind": "steam_appid", "value": "2358720", "tier": 100 }],
            "addon": { "slug": "ue-extended" },
            "profile_id": "ue_extended",
            "inherit_page_guidance": false,
            "guidance": [{
                "id": "renodx.black_myth_wukong.hdr",
                "kind": "engine_ini",
                "message_id": "renodx.black_myth_wukong.hdr",
                "fallback_text": "Add the title-specific HDR setting to Engine.ini.",
                "code": "r.HDR.EnableHDROutput=1"
            }]
        }],
        "engine_profiles": [{
            "id": "ue_extended",
            "engine": "unreal",
            "processing_path": "native",
            "generic_fallback": true,
            "status": "unknown",
            "addon": {
                "slug": "ue-extended",
                "sources": {
                    "x64": "https://marat569.github.io/renodx/renodx-ue-extended.addon64",
                    "x86": "https://marat569.github.io/renodx/renodx-ue-extended.addon32"
                }
            },
            "message": {
                "id": "renodx.generic.ue_extended",
                "fallback_text": "Uses the shared Unreal Engine Extended profile."
            },
            "guidance": [{
                "id": "renodx.ue_extended.hdr_engine_ini",
                "kind": "engine_ini",
                "message_id": "renodx.ue_extended.hdr_engine_ini",
                "fallback_text": "Add the UE5 HDR settings to Engine.ini.",
                "code": "[SystemSettings]\nr.AllowHDR=1",
                "engine_ini": {
                    "schema_version": 1,
                    "revision": 1,
                    "sections": [{
                        "name": "SystemSettings",
                        "entries": [{ "key": "r.AllowHDR", "value": "1" }]
                    }]
                },
                "condition": { "engine": "unreal", "unreal_major": 5 }
            }]
        }],
        "page_guidance": []
    }"#;

    #[test]
    fn parses_and_validates_a_sample_manifest() {
        let manifest = parse_manifest(SAMPLE.as_bytes()).expect("sample manifest is valid");
        assert_eq!(manifest.titles.len(), 2);
        assert_eq!(manifest.titles[0].slug, "cp2077");
        assert_eq!(manifest.generics.len(), 2);
        assert_eq!(manifest.generics[0].message.id, "renodx.generic.unity");
        assert_eq!(
            manifest.generics[0].message.fallback_text,
            "Uses the shared Unity engine profile."
        );
        assert_eq!(
            manifest.titles[0].compatibility.conflicts,
            vec!["special_k"]
        );
        assert_eq!(
            manifest.titles[0].compatibility.source.as_deref(),
            Some("https://example.test/conflict-report")
        );
        // An installable title omits `category`, defaulting to `Installable`; a
        // categorized title carries its tagged payload.
        assert_eq!(
            manifest.titles[0].category,
            types::RenoDxCategory::Installable
        );
        match &manifest.titles[1].category {
            types::RenoDxCategory::External { message, .. } => {
                assert_eq!(message.id, "renodx.external.nexus");
                assert_eq!(message.fallback_text, "Get the add-on from Nexus Mods.");
            }
            other => panic!("expected external category, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_manifest_with_a_canonical_xbox_store_id_rule() {
        let xbox_manifest = SAMPLE.replacen(
            r#""kind": "steam_appid", "value": "1091500""#,
            r#""kind": "xbox_store_id", "value": "9MW53ZKZH168""#,
            1,
        );

        assert!(parse_manifest(xbox_manifest.as_bytes()).is_ok());
    }

    #[test]
    fn parses_strict_v2_guidance_and_profile_references() {
        let manifest = parse_manifest_v2(SAMPLE_V2.as_bytes()).expect("v2 sample is valid");
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(
            manifest.generics[0].profile_id.as_deref(),
            Some("ue_extended")
        );
        assert_eq!(
            manifest.titles[0].profile_id.as_deref(),
            Some("ue_extended")
        );
        assert_eq!(
            manifest.title_guidance["black-myth-wukong"][0]
                .code
                .as_deref(),
            Some("r.HDR.EnableHDROutput=1")
        );
        assert!(
            manifest.title_guidance["black-myth-wukong"][0]
                .engine_ini
                .is_none()
        );
        let hdr = &manifest.generics[0].guidance[0];
        assert_eq!(
            hdr.engine_ini.as_ref().map(|recipe| recipe.revision),
            Some(1)
        );
        assert_eq!(
            hdr.engine_ini
                .as_ref()
                .map(|recipe| recipe.sections[0].entries[0].key.as_str()),
            Some("r.AllowHDR")
        );
    }

    #[test]
    fn versioned_parsers_reject_the_other_schema_number() {
        assert!(
            parse_manifest(
                &SAMPLE
                    .replace("\"schema_version\": 1", "\"schema_version\": 2")
                    .into_bytes()
            )
            .is_err()
        );
        assert!(
            parse_manifest_v2(
                &SAMPLE_V2
                    .replace("\"schema_version\": 2", "\"schema_version\": 1")
                    .into_bytes()
            )
            .is_err()
        );
    }

    #[test]
    fn v2_rejects_unknown_profiles_and_malformed_engine_ini_guidance() {
        let unknown_profile = SAMPLE_V2.replace(
            "\"profile_id\": \"ue_extended\"",
            "\"profile_id\": \"missing_profile\"",
        );
        assert!(parse_manifest_v2(unknown_profile.as_bytes()).is_err());

        let missing_code = SAMPLE_V2.replace(
            ",\n                \"code\": \"r.HDR.EnableHDROutput=1\"",
            "",
        );
        assert!(parse_manifest_v2(missing_code.as_bytes()).is_err());
    }

    #[test]
    fn v2_rejects_shapes_outside_the_published_closed_contract() {
        fn sample_value() -> serde_json::Value {
            serde_json::from_str(SAMPLE_V2).expect("sample JSON")
        }
        fn rejected(value: &serde_json::Value) -> bool {
            parse_manifest_v2(&serde_json::to_vec(value).expect("serialize mutation")).is_err()
        }

        let mut missing_page_guidance = sample_value();
        missing_page_guidance
            .as_object_mut()
            .expect("manifest object")
            .remove("page_guidance");
        assert!(rejected(&missing_page_guidance));

        let mut unknown_profile = sample_value();
        unknown_profile["engine_profiles"][0]["id"] = serde_json::json!("anything");
        assert!(rejected(&unknown_profile));

        let mut unsupported_engine = sample_value();
        unsupported_engine["engine_profiles"][0]["engine"] = serde_json::json!("unreal_extended");
        assert!(rejected(&unsupported_engine));

        let mut catch_all_match = sample_value();
        catch_all_match["games"][0]["match"][0]["kind"] = serde_json::json!("generic");
        assert!(rejected(&catch_all_match));

        let mut ue3_condition = sample_value();
        ue3_condition["engine_profiles"][0]["guidance"][0]["condition"]["unreal_major"] =
            serde_json::json!(3);
        assert!(rejected(&ue3_condition));
    }

    #[test]
    fn v2_rejects_invalid_engine_ini_recipes_and_code_drift() {
        fn sample_value() -> serde_json::Value {
            serde_json::from_str(SAMPLE_V2).expect("sample JSON")
        }
        fn rejected(value: &serde_json::Value) -> bool {
            parse_manifest_v2(&serde_json::to_vec(value).expect("serialize mutation")).is_err()
        }

        let mut unknown_recipe_field = sample_value();
        unknown_recipe_field["engine_profiles"][0]["guidance"][0]["engine_ini"]["extra"] =
            serde_json::json!(true);
        assert!(rejected(&unknown_recipe_field));

        for (field, value) in [
            ("schema_version", serde_json::json!(2)),
            ("revision", serde_json::json!(0)),
        ] {
            let mut invalid = sample_value();
            invalid["engine_profiles"][0]["guidance"][0]["engine_ini"][field] = value;
            assert!(rejected(&invalid));
        }

        let mut empty_sections = sample_value();
        empty_sections["engine_profiles"][0]["guidance"][0]["engine_ini"]["sections"] =
            serde_json::json!([]);
        assert!(rejected(&empty_sections));

        let mut invalid_scalar = sample_value();
        invalid_scalar["engine_profiles"][0]["guidance"][0]["engine_ini"]["sections"][0]["name"] =
            serde_json::json!("System\nSettings");
        assert!(rejected(&invalid_scalar));

        let mut duplicate_section = sample_value();
        duplicate_section["engine_profiles"][0]["guidance"][0]["engine_ini"]["sections"] = serde_json::json!([
            { "name": "SystemSettings", "entries": [{ "key": "r.AllowHDR", "value": "1" }] },
            { "name": "systemsettings", "entries": [{ "key": "other", "value": "1" }] }
        ]);
        assert!(rejected(&duplicate_section));

        let mut duplicate_target = sample_value();
        duplicate_target["engine_profiles"][0]["guidance"][0]["engine_ini"]["sections"][0]["entries"] = serde_json::json!([
            { "key": "r.AllowHDR", "value": "1" },
            { "key": "R.ALLOWHDR", "value": "1" }
        ]);
        assert!(rejected(&duplicate_target));

        let mut code_drift = sample_value();
        code_drift["engine_profiles"][0]["guidance"][0]["code"] =
            serde_json::json!("[SystemSettings]\nr.AllowHDR=0");
        assert!(rejected(&code_drift));

        let mut recipe_on_wrong_kind = sample_value();
        recipe_on_wrong_kind["engine_profiles"][0]["guidance"][0]["kind"] =
            serde_json::json!("warning");
        assert!(rejected(&recipe_on_wrong_kind));

        let mut explicit_null_engine_ini = sample_value();
        explicit_null_engine_ini["engine_profiles"][0]["guidance"][0]["engine_ini"] =
            serde_json::Value::Null;
        assert!(rejected(&explicit_null_engine_ini));

        let mut explicit_null_wrong_kind = sample_value();
        explicit_null_wrong_kind["engine_profiles"][0]["guidance"][0]["kind"] =
            serde_json::json!("warning");
        explicit_null_wrong_kind["engine_profiles"][0]["guidance"][0]["engine_ini"] =
            serde_json::Value::Null;
        assert!(rejected(&explicit_null_wrong_kind));

        let mut max_revision = sample_value();
        max_revision["engine_profiles"][0]["guidance"][0]["engine_ini"]["revision"] =
            serde_json::json!(u64::from(u32::MAX));
        assert!(
            parse_manifest_v2(&serde_json::to_vec(&max_revision).expect("serialize max revision"))
                .is_ok()
        );

        let mut overflowing_revision = sample_value();
        overflowing_revision["engine_profiles"][0]["guidance"][0]["engine_ini"]["revision"] =
            serde_json::json!(u64::from(u32::MAX) + 1);
        assert!(rejected(&overflowing_revision));
    }

    #[test]
    fn v2_unknown_config_key_marks_its_title_without_exposing_partial_config() {
        let mut value: serde_json::Value = serde_json::from_str(SAMPLE_V2).expect("sample JSON");
        let game = value["games"][0].clone();
        value["games"][0]["renodx_config"] = serde_json::json!({
            "settings": [
                { "key": "Upgrade_R11G11B10_FLOAT", "value": 2 },
                { "key": "Future_RenoDX_Setting", "value": 1 }
            ]
        });
        let mut other = game;
        other["id"] = serde_json::json!("known-title");
        other["name"] = serde_json::json!("Known title remains available");
        other["match"][0]["value"] = serde_json::json!("424242");
        value["games"].as_array_mut().expect("games").push(other);

        let manifest =
            parse_manifest_v2(&serde_json::to_vec(&value).expect("serialize mutated manifest"))
                .expect("unknown setting is title-local");

        let unknown = &manifest.titles[0];
        assert_eq!(unknown.id, "black-myth-wukong");
        assert_eq!(unknown.name, "Black Myth: Wukong");
        assert!(unknown.has_unsupported_settings);
        assert!(unknown.renodx_config.is_none());

        let other = &manifest.titles[1];
        assert_eq!(other.name, "Known title remains available");
        assert!(!other.has_unsupported_settings);
    }

    #[test]
    fn v2_unknown_only_config_is_accepted_without_exposing_partial_config() {
        let mut value: serde_json::Value = serde_json::from_str(SAMPLE_V2).expect("sample JSON");
        value["games"][0]["renodx_config"] = serde_json::json!({
            "settings": [{ "key": "Future_RenoDX_Setting", "value": 1 }]
        });

        let manifest =
            parse_manifest_v2(&serde_json::to_vec(&value).expect("serialize mutated manifest"))
                .expect("unknown-only setting is title-local");
        let title = &manifest.titles[0];
        assert!(title.has_unsupported_settings);
        assert!(title.renodx_config.is_none());
    }

    #[test]
    fn v2_unknown_setting_does_not_hide_invalid_known_contract_data() {
        fn rejected(value: &serde_json::Value) -> bool {
            parse_manifest_v2(&serde_json::to_vec(&value).expect("serialize mutation")).is_err()
        }
        fn with_settings(settings: &serde_json::Value) -> serde_json::Value {
            let mut value: serde_json::Value =
                serde_json::from_str(SAMPLE_V2).expect("sample JSON");
            value["games"][0]["renodx_config"] = serde_json::json!({ "settings": settings });
            value
        }

        assert!(rejected(&with_settings(&serde_json::json!([]))));
        assert!(rejected(&with_settings(&serde_json::json!([
            { "key": "Future_RenoDX_Setting", "value": 1 },
            { "key": "Future_RenoDX_Setting", "value": 2 }
        ]))));
        assert!(rejected(&with_settings(&serde_json::json!([
            { "key": "Set_Path", "value": 1 }
        ]))));
        assert!(rejected(&with_settings(&serde_json::json!([
            { "key": "Upgrade_R11G11B10_FLOAT", "value": 4 },
            { "key": "Future_RenoDX_Setting", "value": 1 }
        ]))));
        assert!(rejected(&with_settings(&serde_json::json!([
            { "key": "ForceBorderless", "value": 1 },
            { "key": "Future_RenoDX_Setting", "value": 1 }
        ]))));
        assert!(rejected(&with_settings(&serde_json::json!([
            { "key": "Future_RenoDX_Setting", "value": "1" }
        ]))));
        assert!(rejected(&with_settings(&serde_json::json!([
            { "key": 42, "value": 1 }
        ]))));
        assert!(rejected(&with_settings(&serde_json::json!([
            { "key": " \t", "value": 1 }
        ]))));
        assert!(rejected(&with_settings(&serde_json::json!([
            { "key": "Future_RenoDX_Setting", "value": 1, "extra": true }
        ]))));

        let mut unknown_profile: serde_json::Value =
            serde_json::from_str(SAMPLE_V2).expect("sample JSON");
        unknown_profile["games"][0]["profile_id"] = serde_json::json!("future_profile");
        unknown_profile["games"][0]["renodx_config"] = serde_json::json!({
            "settings": [{ "key": "Future_RenoDX_Setting", "value": 1 }]
        });
        assert!(rejected(&unknown_profile));
    }

    #[test]
    fn parses_the_current_renderpilot_libraries_renodx_manifest() {
        let path = std::env::var_os("RENODX_V2_MANIFEST_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../renderpilot-libraries/addons/v2/renodx.json")
            });
        if !path.is_file() {
            return;
        }
        let bytes = std::fs::read(&path).expect("read current RenoDX producer manifest");
        let manifest = parse_manifest_v2(&bytes).expect("current producer manifest parses");
        let title = manifest
            .titles
            .iter()
            .find(|title| title.id == "gamble-with-your-friends")
            .expect("producer Force_Pipeline_Cloning title");
        assert_eq!(title.profile_id.as_deref(), Some("unity"));
        assert!(!title.has_unsupported_settings);
        assert!(title.renodx_config.as_ref().is_some_and(|config| {
            config.settings.iter().any(|setting| {
                setting.key.as_str() == "Force_Pipeline_Cloning" && setting.value == 1
            })
        }));
    }

    #[test]
    fn rejects_schema_v3() {
        assert!(
            parse_manifest(
                SAMPLE
                    .replace("\"schema_version\": 1", "\"schema_version\": 3")
                    .as_bytes()
            )
            .is_err()
        );
    }

    #[test]
    fn tolerates_a_utf8_bom() {
        let mut bytes = UTF8_BOM.to_vec();
        bytes.extend_from_slice(SAMPLE.as_bytes());
        assert!(parse_manifest(&bytes).is_ok());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_manifest(b"not json").is_err());
    }

    #[test]
    fn rejects_unknown_v1_fields() {
        let sample = SAMPLE.replace(
            r#""generated_at": "2026-06-15T00:00:00Z""#,
            r#""generated_at": "2026-06-15T00:00:00Z", "unexpected": true"#,
        );
        assert!(parse_manifest(sample.as_bytes()).is_err());
    }

    #[test]
    fn rejects_a_blank_catalogue_fallback() {
        let sample = SAMPLE.replace("Uses the shared Unity engine profile.", " ");
        assert!(parse_manifest(sample.as_bytes()).is_err());
    }
}
