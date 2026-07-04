//! Parsing and validation for the standalone, shared `reshade_manifest.json`.
//!
//! RenoDX and Luma each still embed their own `reshade` block (generated from
//! the same upstream constants as this document — see
//! `renderpilot-libraries/scripts/lib/reshade-sources.mjs`) so an app version
//! that never fetches this document keeps working unchanged off that embedded
//! block. A new-enough app additionally fetches this document
//! ([`super::manifest_store::shared_config`]) and overlays it onto whichever
//! tool-manifest it loaded, so a ReShade URL change becomes visible to both
//! tools at once instead of waiting for each tool's own manifest cache to
//! refresh independently.
//!
//! The two structural checks below ([`ensure_stable_reshade_download`],
//! [`ensure_allowed_nightly_download`]) are shared with each tool's own
//! `validate_reshade` (`renodx::validate`, `luma::validate`), so the embedded
//! blocks and this standalone document are held to the identical shape.

use serde::{Deserialize, Serialize};

use crate::ServiceError;

use super::super::UTF8_BOM;
use super::super::errors::failed;
use super::types::{ReshadeConfig, ReshadeNightly, ReshadeStable};

/// Schema version this build understands.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Hosts the shared ReShade nightly build may be downloaded from.
const NIGHTLY_HOST_ALLOWLIST: &[&str] = &["github.com", "nightly.link"];

/// Parsed `reshade_manifest.json` document — the single published source of
/// both tools' ReShade host URLs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ReshadeManifest {
    pub(crate) schema_version: u32,
    /// RFC 3339 timestamp recording when the document was generated. Carried
    /// for parity with the document rather than read by this build.
    pub(crate) generated_at: String,
    /// Manifest-current stable reshade.me add-on installer. `None` when no
    /// stable build is currently published — mirrors [`ReshadeConfig::stable`].
    #[serde(default)]
    pub(crate) stable: Option<ReshadeStable>,
    pub(crate) nightly: ReshadeNightly,
}

impl ReshadeManifest {
    /// Converts the parsed document into the tool-facing [`ReshadeConfig`].
    #[must_use]
    pub(crate) fn into_config(self) -> ReshadeConfig {
        ReshadeConfig {
            stable: self.stable,
            nightly: self.nightly,
        }
    }
}

/// Parses and validates a `reshade_manifest.json` document.
///
/// Strips a leading UTF-8 BOM, deserializes, then runs structural validation,
/// so a returned manifest can be converted to [`ReshadeConfig`] without
/// further checks.
pub(crate) fn parse_reshade_manifest(bytes: &[u8]) -> Result<ReshadeManifest, ServiceError> {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let manifest: ReshadeManifest = serde_json::from_slice(bytes)
        .map_err(|error| failed(format!("failed to parse ReShade manifest: {error}")))?;
    validate(&manifest)?;
    Ok(manifest)
}

fn validate(manifest: &ReshadeManifest) -> Result<(), ServiceError> {
    if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(failed(format!(
            "unsupported ReShade manifest schema version: expected {SUPPORTED_SCHEMA_VERSION}, got {}",
            manifest.schema_version
        )));
    }
    if let Some(stable) = &manifest.stable {
        ensure_stable_reshade_download("reshade stable url", &stable.url)?;
    }
    ensure_allowed_nightly_download("reshade nightly url64", &manifest.nightly.url64)?;
    ensure_allowed_nightly_download("reshade nightly url32", &manifest.nightly.url32)?;
    Ok(())
}

/// Asserts a stable ReShade URL is the official `reshade.me` add-on installer
/// shape (`https://reshade.me/downloads/ReShade_Setup_<version>_Addon.exe`,
/// no userinfo). Shared by this standalone manifest and RenoDX's own embedded
/// `reshade.stable` block (`renodx::validate::validate_reshade`) — Luma has no
/// stable field, so it never calls this.
pub(crate) fn ensure_stable_reshade_download(field: &str, url: &str) -> Result<(), ServiceError> {
    let parsed = crate::net::parse_https_url(url, field)?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(failed(format!("{field} must not include userinfo")));
    }
    if parsed.host_str() != Some("reshade.me") {
        return Err(failed(format!("{field} host must be `reshade.me`")));
    }
    let path = parsed.path();
    if !path.starts_with("/downloads/ReShade_Setup_") || !path.ends_with("_Addon.exe") {
        return Err(failed(format!(
            "{field} must point at `/downloads/ReShade_Setup_*_Addon.exe`"
        )));
    }
    Ok(())
}

/// Asserts a nightly ReShade URL is hosted on an allow-listed CI-proxy host.
/// Shared by this standalone manifest and each tool's own embedded
/// `reshade.nightly` block (`renodx::validate`, `luma::validate`).
pub(crate) fn ensure_allowed_nightly_download(field: &str, url: &str) -> Result<(), ServiceError> {
    let parsed = crate::net::parse_https_url(url, field)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| failed(format!("{field} has no host")))?;
    if !NIGHTLY_HOST_ALLOWLIST.contains(&host) {
        return Err(failed(format!("{field} host `{host}` is not allow-listed")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "schema_version": 1,
        "generated_at": "2026-07-05T00:00:00Z",
        "stable": { "url": "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe" },
        "nightly": {
            "url64": "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip",
            "url32": "https://nightly.link/crosire/reshade/workflows/build/main/x32.zip"
        }
    }"#;

    #[test]
    fn parses_a_valid_manifest() {
        let manifest = parse_reshade_manifest(SAMPLE.as_bytes()).expect("parse");
        assert_eq!(manifest.schema_version, 1);
        assert!(manifest.stable.is_some());
        assert_eq!(
            manifest.nightly.url64,
            "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip"
        );
    }

    #[test]
    fn tolerates_a_utf8_bom() {
        let mut bytes = UTF8_BOM.to_vec();
        bytes.extend_from_slice(SAMPLE.as_bytes());
        assert!(parse_reshade_manifest(&bytes).is_ok());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_reshade_manifest(b"not json").is_err());
    }

    #[test]
    fn stable_is_optional() {
        let sample = SAMPLE.replace(
            r#""stable": { "url": "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe" },"#,
            "",
        );
        let manifest = parse_reshade_manifest(sample.as_bytes()).expect("parse");
        assert!(manifest.stable.is_none());
    }

    #[test]
    fn rejects_an_unsupported_schema_version() {
        let sample = SAMPLE.replace(r#""schema_version": 1"#, r#""schema_version": 2"#);
        assert!(parse_reshade_manifest(sample.as_bytes()).is_err());
    }

    #[test]
    fn rejects_a_stable_url_on_the_wrong_host() {
        let sample = SAMPLE.replace(
            "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe",
            "https://example.com/downloads/ReShade_Setup_6.7.3_Addon.exe",
        );
        assert!(parse_reshade_manifest(sample.as_bytes()).is_err());
    }

    #[test]
    fn rejects_a_stable_url_with_the_wrong_shape() {
        let sample = SAMPLE.replace(
            "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe",
            "https://reshade.me/downloads/ReShade_Setup_6.7.3.exe",
        );
        assert!(parse_reshade_manifest(sample.as_bytes()).is_err());
    }

    #[test]
    fn rejects_a_nightly_url_on_a_disallowed_host() {
        let sample = SAMPLE.replace(
            "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip",
            "https://example.com/x64.zip",
        );
        assert!(parse_reshade_manifest(sample.as_bytes()).is_err());
    }

    #[test]
    fn into_config_carries_stable_and_nightly_through() {
        let manifest = parse_reshade_manifest(SAMPLE.as_bytes()).expect("parse");
        let config = manifest.into_config();
        assert!(config.stable.is_some());
        assert_eq!(
            config.nightly.url64,
            "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip"
        );
    }
}
