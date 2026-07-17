//! Parsing and validation for the standalone, shared ReShade v1 document.
//!
//! It is the only published source of ReShade channels. Tool manifests carry
//! no host download URLs; when the shared document and its cache are
//! unavailable, the app uses a bundled snapshot of this same wire format.
//!
//! The structural checks below validate every remote, cached, and bundled copy.

use serde::Deserialize;

use crate::ServiceError;

use super::super::UTF8_BOM;
use super::super::errors::failed;
use super::types::{ReshadeNightly, ReshadeSourceCatalog, ReshadeStable};

/// Schema version this build understands.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Hosts the shared ReShade nightly build may be downloaded from.
const NIGHTLY_HOST_ALLOWLIST: &[&str] = &["github.com", "nightly.link"];

/// Parsed `addons/v1/reshade.json` document — the single published source of
/// both tools' ReShade host URLs.
#[derive(Debug, Clone)]
pub(crate) struct ReshadeManifest {
    pub(crate) schema_version: u32,
    /// RFC 3339 timestamp recording when the document was generated. Carried
    /// for parity with the document rather than read by this build.
    pub(crate) generated_at: String,
    /// Manifest-current stable reshade.me add-on installer. `None` when no
    /// stable build is currently published — mirrors [`ReshadeSourceCatalog::stable`].
    pub(crate) stable: Option<ReshadeStable>,
    pub(crate) nightly: ReshadeNightly,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireManifestV1 {
    schema_version: u32,
    generated_at: String,
    channels: WireChannels,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireChannels {
    #[serde(default)]
    stable: Option<WireStable>,
    nightly: WireNightly,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireStable {
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireNightly {
    url64: String,
    url32: String,
}

impl ReshadeManifest {
    /// Converts the parsed document into the operation-facing source catalogue.
    #[must_use]
    pub(crate) fn into_sources(self) -> ReshadeSourceCatalog {
        ReshadeSourceCatalog {
            stable: self.stable,
            nightly: self.nightly,
        }
    }
}

/// Parses and validates the ReShade v1 document.
///
/// Strips a leading UTF-8 BOM, deserializes, then runs structural validation,
/// so a returned manifest can be converted to [`ReshadeSourceCatalog`] without
/// further checks.
pub(crate) fn parse_reshade_manifest(bytes: &[u8]) -> Result<ReshadeManifest, ServiceError> {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    let wire: WireManifestV1 = serde_json::from_slice(bytes)
        .map_err(|error| failed(format!("failed to parse ReShade manifest: {error}")))?;
    let manifest = ReshadeManifest {
        schema_version: wire.schema_version,
        generated_at: wire.generated_at,
        stable: wire
            .channels
            .stable
            .map(|stable| ReshadeStable { url: stable.url }),
        nightly: ReshadeNightly {
            url64: wire.channels.nightly.url64,
            url32: wire.channels.nightly.url32,
        },
    };
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
    crate::addons::manifest_validate::ensure_not_blank(
        "reshade generated_at",
        &manifest.generated_at,
    )?;
    if let Some(stable) = &manifest.stable {
        ensure_stable_reshade_download("reshade stable url", &stable.url)?;
    }
    ensure_allowed_nightly_download("reshade nightly url64", &manifest.nightly.url64)?;
    ensure_allowed_nightly_download("reshade nightly url32", &manifest.nightly.url32)?;
    Ok(())
}

/// Asserts a stable ReShade URL is the official `reshade.me` add-on installer
/// shape (`https://reshade.me/downloads/ReShade_Setup_<version>_Addon.exe`,
/// no userinfo).
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
/// Applied identically to remote, cache, and bundled copies.
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
        "channels": {
          "stable": { "url": "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe" },
          "nightly": {
              "url64": "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip",
              "url32": "https://nightly.link/crosire/reshade/workflows/build/main/x32.zip"
          }
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
        let sample = SAMPLE.replace(r#""schema_version": 1"#, r#""schema_version": 3"#);
        assert!(parse_reshade_manifest(sample.as_bytes()).is_err());
    }

    #[test]
    fn rejects_unknown_v1_fields() {
        let sample = SAMPLE.replace(
            r#""generated_at": "2026-07-05T00:00:00Z""#,
            r#""generated_at": "2026-07-05T00:00:00Z", "unexpected": true"#,
        );
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
    fn into_sources_carries_stable_and_nightly_through() {
        let manifest = parse_reshade_manifest(SAMPLE.as_bytes()).expect("parse");
        let config = manifest.into_sources();
        assert!(config.stable.is_some());
        assert_eq!(
            config.nightly.url64,
            "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip"
        );
    }
}
