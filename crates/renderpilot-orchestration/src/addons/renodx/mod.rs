//! RenoDX add-on installation subsystem.
//!
//! Introduces the RenoDX (Renovation Engine for DirectX) ReShade HDR add-on into a
//! game and can fully reverse the change. Add-ons are fetched **live from upstream**
//! (clshortfuse.github.io and the engine-generic repos) rather than mirrored, so the
//! manifest is a lightweight overrides + catalogue document with no hashes.
//!
//! The end-to-end flows live in [`service`] (`availability`, `status`, `install`,
//! `uninstall`) and [`update`] (`check_update`, `update`, `check_updates`), built on
//! the [`types`] model, its [`parse_manifest`] validation, the [`facts`] gatherer,
//! the deterministic [`matcher`], the [`anticheat`] risk gate, [`reshade`] host
//! orchestration, the [`source`] URL/host resolver, the [`fetch`] downloader, and
//! the [`install`] filesystem engine. RenoDX-specific policy (which API to target,
//! installability) lives in [`policy`], separate from the generic detection facts.

pub(crate) mod anticheat;
pub(crate) mod dlss_fix;
mod errors;
pub(crate) mod facts;
mod fetch;
pub(crate) mod install;
pub mod manifest_store;
pub(crate) mod matcher;
pub(crate) mod policy;
pub(crate) mod reshade;
pub mod service;
mod source;
pub mod types;
pub mod update;
mod validate;

#[cfg(test)]
mod test_support;

use renderpilot_domain::Architecture;

use crate::ServiceError;

use self::types::RenoDxManifest;

/// UTF-8 byte-order mark some publishing tools prepend to JSON, which `serde_json`
/// rejects; stripped at the parse boundary.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Parses and validates a RenoDX manifest document.
///
/// Strips a leading UTF-8 BOM, deserializes, then runs schema + structural
/// validation, so a returned manifest can be acted on without further checks.
pub fn parse_manifest(bytes: &[u8]) -> Result<RenoDxManifest, ServiceError> {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let manifest: RenoDxManifest = serde_json::from_slice(bytes)
        .map_err(|error| errors::failed(format!("failed to parse RenoDX manifest: {error}")))?;
    validate::validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Derives the add-on architecture from an add-on file name's extension
/// (`renodx-<slug>.addon64` → X64, `.addon32` → X86).
#[must_use]
fn arch_from_addon_file(name: &str) -> Option<Architecture> {
    if name.ends_with("addon64") {
        Some(Architecture::X64)
    } else if name.ends_with("addon32") {
        Some(Architecture::X86)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "schema_version": 3,
        "generated_at": "2026-06-15T00:00:00Z",
        "reshade": {
            "nightly": {
                "url64": "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip",
                "url32": "https://nightly.link/crosire/reshade/workflows/build/main/x32.zip"
            }
        },
        "generics": [
            { "engine": "unity", "url64": "https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64", "label_key": "renodx.generic.unity" },
            { "engine": "unreal", "slug": "_univ", "label_key": "renodx.generic.universal" }
        ],
        "defaults": {
            "risk": { "anticheat_engine": "none", "online": "singleplayer", "severity": "info", "message_key": "renodx.risk.sp_safe", "confidence": "medium", "source": "https://github.com/clshortfuse/renodx/wiki/Mods" },
            "min_app_version": "1.0.0",
            "channel": "stable"
        },
        "titles": [
            {
                "id": "cyberpunk-2077", "name": "Cyberpunk 2077", "slug": "cp2077",
                "arch": "X64", "status": "working",
                "match": [{ "kind": "steam_appid", "value": "1091500", "tier": 100 }],
                "compatibility": { "conflicts": ["special_k"] },
                "notes_keys": ["renodx.note.cp2077.hdr10"]
            },
            {
                "id": "nexus-game", "name": "Nexus Game", "slug": "nexusgame",
                "arch": "X64", "status": "working",
                "category": { "kind": "external", "url": "https://www.nexusmods.com/x", "label_key": "renodx.external.nexus" },
                "match": [{ "kind": "steam_appid", "value": "424242", "tier": 100 }]
            }
        ]
    }"#;

    #[test]
    fn parses_and_validates_a_sample_manifest() {
        let manifest = parse_manifest(SAMPLE.as_bytes()).expect("sample manifest is valid");
        assert_eq!(manifest.titles.len(), 2);
        assert_eq!(manifest.titles[0].slug, "cp2077");
        assert_eq!(manifest.generics.len(), 2);
        // The title omits `risk`/`channel`/`min_app_version`; the parser fills them
        // from the manifest's `defaults` via `#[serde(default)]`.
        assert_eq!(manifest.titles[0].risk.message_key, "renodx.risk.sp_safe");
        assert_eq!(manifest.titles[0].min_app_version, "1.0.0");
        assert_eq!(
            manifest.titles[0].compatibility.conflicts,
            vec!["special_k"]
        );
        // An installable title omits `category`, defaulting to `Installable`; a
        // categorized title carries its tagged payload.
        assert_eq!(manifest.titles[0].category, types::Category::Installable);
        assert!(matches!(
            manifest.titles[1].category,
            types::Category::External { .. }
        ));
    }

    #[test]
    fn ignores_legacy_top_level_override_maps() {
        // A manifest from before the unified catalogue still carries top-level
        // `external`/`native_hdr`/`blacklist` keys; the parser ignores them (serde
        // does not deny unknown fields) so an old cached manifest keeps loading.
        let legacy = SAMPLE.replace(
            "\"titles\": [",
            "\"external\": { \"old\": { \"url\": \"https://x/y\", \"label_key\": \"k\" } }, \
             \"native_hdr\": [\"old\"], \"blacklist\": { \"old\": \"why\" }, \"titles\": [",
        );
        assert!(parse_manifest(legacy.as_bytes()).is_ok());
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
}
