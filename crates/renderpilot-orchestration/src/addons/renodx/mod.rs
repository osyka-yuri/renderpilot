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
/// Platform infrastructure.
pub mod platform;
pub(crate) mod policy;
mod reconciliation;
pub(crate) mod reshade;
mod reshade_ini;
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

use self::types::{RenoDxManifest, WireManifestV1};
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
    validate::validate_manifest(&manifest)?;
    Ok(manifest)
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
