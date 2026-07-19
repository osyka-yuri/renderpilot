//! Luma Framework add-on installation subsystem.
//!
//! Introduces the Luma Framework ReShade add-on (Filoppi) into a game and can
//! fully reverse the change. Its per-game DLSS / FSR and HDR availability is
//! explicit manifest data sourced from the upstream wiki. Unlike RenoDX, Luma
//! publishes no per-game upstream repository: every asset is a file on a
//! single rolling GitHub Release, so the manifest is a hand-curated catalogue
//! (dedicated per-game profiles plus wiki-listed Generic-Mod games) rather than
//! an overrides document.
//!
//! Luma is mutually exclusive with RenoDX per game (via the shared add-on
//! exclusivity policy) and, unlike RenoDX, always installs the **nightly**
//! ReShade host.
//!
//! The end-to-end flows live in [`use_cases`], built on the [`types`] model and
//! [`parse_manifest`] validation, plus private matching, host, source, fetch,
//! and install helpers under this module. Shared risk/anti-cheat and ReShade
//! host orchestration live in sibling `addons::*` modules.
//!
//! This module (and everything under it) never imports from the RenoDX tool
//! module — every genuinely shared concept (matching, risk, anti-cheat, the
//! ReShade host subsystem, the install engine) already lives in a common
//! `addons::*` module both tools depend on instead.

mod dgvoodoo;
mod dlss;
/// DTOs
pub mod dto;
mod errors;
mod fetch;
mod game_context;
pub(crate) mod install;
pub mod manifest_store;
pub(crate) mod matcher;
pub(crate) mod mutation_targets;
pub(crate) mod reconciliation;
mod source;
pub(crate) mod tool;
mod tracking;
pub mod types;
/// Use cases.
pub mod use_cases;
mod validate;
mod vcredist;

#[cfg(test)]
pub(crate) mod test_support;

use crate::ServiceError;

use self::types::{LumaManifest, WireManifestV1};
use super::UTF8_BOM;

/// i18n key for the indeterminate "finalizing" progress phase (see
/// [`crate::addons::progress::emit_tool_finalizing`]), looked up by the frontend
/// during the write/persist phase between a download finishing and an install
/// or update command returning. Exposed via [`tool::LumaTool::finalizing_phase`].
pub(crate) const LUMA_PHASE_FINALIZING: &str = "luma.phase.finalizing";

/// Parses and validates a Luma manifest document.
///
/// Strips a leading UTF-8 BOM, deserializes, then runs schema + structural
/// validation, so a returned manifest can be acted on without further checks.
pub fn parse_manifest(bytes: &[u8]) -> Result<LumaManifest, ServiceError> {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let wire: WireManifestV1 = serde_json::from_slice(bytes)
        .map_err(|error| errors::failed(format!("failed to parse Luma manifest: {error}")))?;
    let manifest = LumaManifest::from_wire_v1(wire)?;
    validate::validate_manifest(&manifest)?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "schema_version": 1,
        "generated_at": "2026-07-04T00:00:00Z",
        "minimum_reshade_version": "6.7.0",
        "games": [
            {
                "id": "dishonored-2", "name": "Dishonored 2",
                "package": { "release_asset": "Luma-Dishonored_2.zip", "addon_file": "Luma-Dishonored 2.addon" }, "profile": "game", "architecture": "X64", "status": "working",
                "match": [{ "kind": "steam_appid", "value": "403640", "tier": 100 }]
            },
            {
                "id": "tekken-7", "name": "TEKKEN 7",
                "package": { "release_asset": "Luma-Unreal_Engine.zip", "addon_file": "Luma-Unreal Engine.addon" }, "profile": "unreal", "architecture": "X64", "status": "unknown",
                "match": [{ "kind": "steam_appid", "value": "389730", "tier": 100 }],
                "features": { "dlss_fsr": "unknown", "hdr": "unknown" },
                "requirements": { "launch_arguments": ["-nod3d9ex"] }
            }
        ]
    }"#;

    #[test]
    fn parses_and_validates_a_sample_manifest() {
        let manifest = parse_manifest(SAMPLE.as_bytes()).expect("sample manifest is valid");
        assert_eq!(manifest.titles.len(), 2);
        assert_eq!(manifest.titles[0].asset, "Luma-Dishonored_2.zip");
        assert_eq!(manifest.titles[0].addon_file, "Luma-Dishonored 2.addon");
        assert!(!manifest.titles[0].profile.is_engine());
        assert!(manifest.titles[1].profile.is_engine());
        assert_eq!(manifest.titles[1].launch_args, vec!["-nod3d9ex".to_owned()]);
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
    fn rejects_an_unsupported_schema_version() {
        assert!(
            parse_manifest(
                SAMPLE
                    .replace("\"schema_version\": 1", "\"schema_version\": 99")
                    .as_bytes()
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_a_title_without_an_explicit_payload_identity() {
        assert!(
            parse_manifest(
                SAMPLE
                    .replace(", \"addon_file\": \"Luma-Dishonored 2.addon\"", "")
                    .as_bytes(),
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_a_title_without_feature_statuses() {
        assert!(
            parse_manifest(
                SAMPLE
                    .replace(
                        "\"features\": { \"dlss_fsr\": \"unknown\", \"hdr\": \"unknown\" },\n",
                        "",
                    )
                    .as_bytes(),
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_an_engine_profile_with_the_wrong_payload() {
        assert!(
            parse_manifest(
                SAMPLE
                    .replace("\"profile\": \"unreal\"", "\"profile\": \"unity\"")
                    .as_bytes(),
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_a_game_profile_using_a_shared_payload() {
        assert!(
            parse_manifest(
                SAMPLE
                    .replace("\"profile\": \"unreal\"", "\"profile\": \"game\"")
                    .as_bytes(),
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_a_unity_profile_with_the_wrong_architecture_asset() {
        let sample = SAMPLE
            .replace("\"profile\": \"unreal\"", "\"profile\": \"unity\"")
            .replace("Luma-Unreal_Engine.zip", "Luma-Unity_Engine-x32.zip");
        assert!(parse_manifest(sample.as_bytes()).is_err());
    }

    #[test]
    fn rejects_an_unknown_feature_status() {
        assert!(
            parse_manifest(
                SAMPLE
                    .replace("\"dlss_fsr\": \"unknown\"", "\"dlss_fsr\": \"enabled\"")
                    .as_bytes(),
            )
            .is_err()
        );
    }

    #[test]
    fn preserves_a_blocked_message_with_its_fallback() {
        let sample = SAMPLE.replace(
            r#""match": [{ "kind": "steam_appid", "value": "403640", "tier": 100 }]"#,
            r#""match": [{ "kind": "steam_appid", "value": "403640", "tier": 100 }],
                "availability": { "kind": "blocked", "message": { "id": "luma.blocked.test", "fallback_text": "This profile is known not to work." } }"#,
        );
        let manifest = parse_manifest(sample.as_bytes()).expect("blocked manifest is valid");
        match &manifest.titles[0].category {
            types::LumaCategory::Blacklist { message } => {
                assert_eq!(message.id, "luma.blocked.test");
                assert_eq!(message.fallback_text, "This profile is known not to work.");
            }
            other => panic!("expected blacklist, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_v1_fields() {
        let sample = SAMPLE.replace(
            r#""generated_at": "2026-07-04T00:00:00Z""#,
            r#""generated_at": "2026-07-04T00:00:00Z", "unexpected": true"#,
        );
        assert!(parse_manifest(sample.as_bytes()).is_err());
    }

    #[test]
    fn rejects_a_blank_catalogue_fallback() {
        let sample = SAMPLE.replace(
            r#""match": [{ "kind": "steam_appid", "value": "403640", "tier": 100 }]"#,
            r#""match": [{ "kind": "steam_appid", "value": "403640", "tier": 100 }],
                "availability": { "kind": "blocked", "message": { "id": "luma.blocked.test", "fallback_text": " " } }"#,
        );
        assert!(parse_manifest(sample.as_bytes()).is_err());
    }
}
