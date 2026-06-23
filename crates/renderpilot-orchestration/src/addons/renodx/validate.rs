//! Validation for the RenoDX overrides manifest.
//!
//! With add-ons fetched live from upstream there are no artifacts/hashes to
//! cross-check; validation now enforces a supported schema, well-formed slugs and
//! match rules, sane risk metadata, and that every download host (ReShade sources,
//! engine generics) is HTTPS and on the allow-list. A manifest that passes can be
//! resolved and installed without further structural checks.

use std::collections::HashSet;

use crate::fs::is_safe_file_name;
use crate::ServiceError;

use super::errors;
use super::types::{
    manifest_defaults, Category, Generic, MatchKind, MatchRule, RenoDxManifest, ReshadeConfig,
    Title,
};

/// Schema version this build understands.
const SUPPORTED_SCHEMA_VERSION: u32 = 3;

/// Hosts a RenoDX add-on or ReShade build may be downloaded from.
const DOWNLOAD_HOST_ALLOWLIST: &[&str] = &["clshortfuse.github.io", "github.com", "nightly.link"];

/// Validates an entire manifest.
pub(super) fn validate_manifest(manifest: &RenoDxManifest) -> Result<(), ServiceError> {
    if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(errors::failed(format!(
            "unsupported RenoDX manifest schema version: expected {SUPPORTED_SCHEMA_VERSION}, got {}",
            manifest.schema_version
        )));
    }

    // The manifest's shared `defaults` (schema v3) must agree with the Rust-side
    // `#[serde(default)]` values used to fill omitted title fields; a drift would
    // silently change install behaviour, so catch it at load time.
    validate_defaults(&manifest.defaults)?;

    validate_reshade(&manifest.reshade)?;

    for generic in &manifest.generics {
        validate_generic(generic)?;
    }

    let mut title_ids: HashSet<&str> = HashSet::with_capacity(manifest.titles.len());
    for title in &manifest.titles {
        validate_title(title)?;
        if !title_ids.insert(title.id.as_str()) {
            return Err(errors::failed(format!(
                "duplicate title id `{}` in manifest",
                title.id
            )));
        }
    }

    Ok(())
}

/// Asserts that the manifest's `defaults` (schema v3) match the Rust-side defaults
/// backing the title `#[serde(default)]` fields. The generator emits these same
/// values, so a mismatch indicates a generator/validator drift.
fn validate_defaults(defaults: &super::types::Defaults) -> Result<(), ServiceError> {
    let expected = manifest_defaults();
    if defaults != &expected {
        return Err(errors::failed(
            "RenoDX manifest `defaults` do not match the validator's expected defaults \
             (risk / min_app_version / channel drift between generator and parser)"
                .to_owned(),
        ));
    }
    Ok(())
}

/// Validates a title's [`Category`] payload: an external link must be HTTPS with a
/// non-blank label; a blacklist entry must carry a non-blank reason. The
/// installable and native-HDR categories carry no payload to check.
fn validate_category(category: &Category) -> Result<(), ServiceError> {
    match category {
        Category::External { url, label_key } => {
            ensure_https("title external url", url)?;
            ensure_not_blank("title external label_key", label_key)?;
        }
        Category::Blacklist { reason } => {
            ensure_not_blank("title blacklist reason", reason)?;
        }
        Category::Installable | Category::NativeHdr => {}
    }
    Ok(())
}

fn validate_reshade(reshade: &ReshadeConfig) -> Result<(), ServiceError> {
    ensure_allowed_download("reshade nightly url64", &reshade.nightly.url64)?;
    ensure_allowed_download("reshade nightly url32", &reshade.nightly.url32)?;
    Ok(())
}

fn validate_generic(generic: &Generic) -> Result<(), ServiceError> {
    match (&generic.slug, &generic.url64) {
        (Some(slug), _) => ensure_slug("generic slug", slug)?,
        (None, Some(url64)) => {
            ensure_allowed_download("generic url64", url64)?;
            if let Some(url32) = &generic.url32 {
                ensure_allowed_download("generic url32", url32)?;
            }
        }
        (None, None) => {
            return Err(errors::failed(
                "generic must define either a slug or url64".to_owned(),
            ))
        }
    }
    Ok(())
}

fn validate_title(title: &Title) -> Result<(), ServiceError> {
    ensure_not_blank("title id", &title.id)?;
    ensure_not_blank("title name", &title.name)?;
    ensure_slug("title slug", &title.slug)?;
    ensure_not_blank("title risk message_key", &title.risk.message_key)?;

    if title.match_rules.is_empty() {
        return Err(errors::failed(format!(
            "title `{}` must declare at least one match rule",
            title.id
        )));
    }
    for rule in &title.match_rules {
        if rule.tier == 0 {
            return Err(errors::failed(format!(
                "title `{}` match rule tier must be greater than zero",
                title.id
            )));
        }
        if rule.kind != MatchKind::Generic && rule.value.trim().is_empty() {
            return Err(errors::failed(format!(
                "title `{}` match rule of kind {:?} requires a value",
                title.id, rule.kind
            )));
        }
        validate_match_rule_value(title, rule)?;
    }

    if let Some(proxy) = &title.proxy_dll_override {
        ensure_safe_file_name("title proxy_dll_override", proxy)?;
    }
    if let Some(url) = &title.download_url {
        // download_url targets third-party hosts (various github.io pages, GitHub
        // releases), so only HTTPS is enforced — no host allow-list.
        ensure_https("title download_url", url)?;
    }
    ensure_semver("title min_app_version", &title.id, &title.min_app_version)?;
    for conflict in &title.compatibility.conflicts {
        ensure_not_blank("title compatibility.conflicts entry", conflict)?;
    }
    validate_category(&title.category)?;
    Ok(())
}

/// Asserts a value is a dotted-triple version (e.g. `1.0.0`).
fn ensure_semver(field: &str, title_id: &str, value: &str) -> Result<(), ServiceError> {
    let ok = !value.is_empty()
        && value
            .split('.')
            .map(|part| part.parse::<u32>())
            .all(|part| part.is_ok());
    if !ok {
        return Err(errors::failed(format!(
            "title `{title_id}` {field} must be a dotted-triple version, got `{value}`"
        )));
    }
    Ok(())
}

fn validate_match_rule_value(title: &Title, rule: &MatchRule) -> Result<(), ServiceError> {
    match rule.kind {
        MatchKind::ExeSha256 if !is_lowercase_sha256_hex(&rule.value) => {
            Err(errors::failed(format!(
                "title `{}` ExeSha256 rule value must be lowercase hex SHA-256",
                title.id
            )))
        }
        MatchKind::SteamAppid if !rule.value.parse::<u64>().is_ok_and(|appid| appid > 0) => {
            Err(errors::failed(format!(
                "title `{}` SteamAppid rule value must be a positive integer",
                title.id
            )))
        }
        MatchKind::EpicId | MatchKind::GogId if rule.value.trim().is_empty() => {
            Err(errors::failed(format!(
                "title `{}` {:?} rule value must not be empty",
                title.id, rule.kind
            )))
        }
        _ => Ok(()),
    }
}

fn ensure_not_blank(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(errors::failed(format!("`{field}` must not be empty")));
    }
    Ok(())
}

/// A slug is a bare upstream add-on identifier: file-name-safe (so it can be
/// interpolated into a URL/path) and non-empty.
fn ensure_slug(field: &str, value: &str) -> Result<(), ServiceError> {
    ensure_not_blank(field, value)?;
    let ok = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !ok {
        return Err(errors::failed(format!(
            "`{field}` must be a bare slug ([A-Za-z0-9._-]), got `{value}`"
        )));
    }
    Ok(())
}

fn ensure_safe_file_name(field: &str, value: &str) -> Result<(), ServiceError> {
    ensure_not_blank(field, value)?;
    if !is_safe_file_name(value) {
        return Err(errors::failed(format!(
            "`{field}` must be a bare file name, got `{value}`"
        )));
    }
    Ok(())
}

fn ensure_https(field: &str, url: &str) -> Result<(), ServiceError> {
    crate::net::parse_https_url(url, field)?;
    Ok(())
}

fn ensure_allowed_download(field: &str, url: &str) -> Result<(), ServiceError> {
    let parsed = crate::net::parse_https_url(url, field)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| errors::failed(format!("{field} has no host")))?;
    if !DOWNLOAD_HOST_ALLOWLIST.contains(&host) {
        return Err(errors::failed(format!(
            "{field} host `{host}` is not allow-listed"
        )));
    }
    Ok(())
}

fn is_lowercase_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::Architecture;

    use super::*;
    use crate::addons::renodx::test_support::{manifest, rule, title};
    use crate::addons::renodx::types::{Category, MatchKind, Status};

    fn one_title_manifest() -> RenoDxManifest {
        manifest(vec![title(
            "game.x",
            "slugx",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "100", 100)],
        )])
    }

    #[test]
    fn valid_manifest_passes() {
        assert!(validate_manifest(&one_title_manifest()).is_ok());
    }

    #[test]
    fn external_category_passes_with_https_url_and_label() {
        let mut m = one_title_manifest();
        m.titles[0].category = Category::External {
            url: "https://discord.gg/example".to_owned(),
            label_key: "renodx.external.discord".to_owned(),
        };
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn external_category_rejects_non_https_url() {
        let mut m = one_title_manifest();
        m.titles[0].category = Category::External {
            url: "http://discord.gg/example".to_owned(),
            label_key: "renodx.external.discord".to_owned(),
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn blacklist_category_requires_a_reason() {
        let mut m = one_title_manifest();
        m.titles[0].category = Category::Blacklist {
            reason: String::new(),
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn defaults_drift_is_rejected() {
        let mut m = one_title_manifest();
        m.defaults.min_app_version = "2.0.0".to_owned();
        assert!(validate_manifest(&m).is_err());
    }
}
