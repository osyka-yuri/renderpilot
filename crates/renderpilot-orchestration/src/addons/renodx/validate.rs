//! Validation for the RenoDX overrides manifest.
//!
//! With add-ons fetched live from upstream there are no artifacts/hashes to
//! cross-check; validation now enforces a supported schema, well-formed slugs and
//! match rules, sane risk metadata, and that explicit add-on URL basenames match
//! the canonical local file name derived from the slug. A manifest that passes can
//! be resolved and installed without further structural checks.

use std::collections::HashSet;

use renderpilot_domain::Architecture;

use crate::ServiceError;
use crate::fs::is_safe_file_name;

use super::errors;
use super::source;
use super::types::{
    Category, Generic, MatchKind, MatchRule, RenoDxManifest, ReshadeConfig, Title,
    manifest_defaults,
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
    if let Some(stable) = &reshade.stable {
        ensure_stable_reshade_download("reshade stable url", &stable.url)?;
    }
    ensure_allowed_download("reshade nightly url64", &reshade.nightly.url64)?;
    ensure_allowed_download("reshade nightly url32", &reshade.nightly.url32)?;
    Ok(())
}

fn validate_generic(generic: &Generic) -> Result<(), ServiceError> {
    if let Some(slug) = &generic.slug {
        ensure_slug("generic slug", slug)?;
    }

    let has_url64 = generic.url64.is_some();
    let has_url32 = generic.url32.is_some();
    if has_url64 != has_url32 {
        return Err(errors::failed(
            "generic url64 and url32 must be provided together".to_owned(),
        ));
    }
    if let Some(url64) = &generic.url64 {
        ensure_allowed_addon_download_matches_file_name(
            "generic url64",
            url64,
            &source::addon_file_name(source::generic_local_slug(generic), Architecture::X64),
        )?;
    }
    if let Some(url32) = &generic.url32 {
        ensure_allowed_addon_download_matches_file_name(
            "generic url32",
            url32,
            &source::addon_file_name(source::generic_local_slug(generic), Architecture::X86),
        )?;
    }

    if generic.slug.is_none() && (!has_url64 || !has_url32) {
        return Err(errors::failed(
            "generic must define a slug or both url64 and url32".to_owned(),
        ));
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
        ensure_https_addon_download_matches_file_name(
            "title download_url",
            url,
            &source::addon_file_name(&title.slug, title.arch),
        )?;
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

fn ensure_https(field: &str, url: &str) -> Result<reqwest::Url, ServiceError> {
    crate::net::parse_https_url(url, field)
}

fn ensure_allowed_download(field: &str, url: &str) -> Result<reqwest::Url, ServiceError> {
    let parsed = ensure_https(field, url)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| errors::failed(format!("{field} has no host")))?;
    if !DOWNLOAD_HOST_ALLOWLIST.contains(&host) {
        return Err(errors::failed(format!(
            "{field} host `{host}` is not allow-listed"
        )));
    }
    Ok(parsed)
}

fn ensure_https_addon_download_matches_file_name(
    field: &str,
    url: &str,
    expected_name: &str,
) -> Result<(), ServiceError> {
    let parsed = ensure_https(field, url)?;
    ensure_url_basename_matches(field, &parsed, expected_name)
}

fn ensure_allowed_addon_download_matches_file_name(
    field: &str,
    url: &str,
    expected_name: &str,
) -> Result<(), ServiceError> {
    let parsed = ensure_allowed_download(field, url)?;
    ensure_url_basename_matches(field, &parsed, expected_name)
}

fn ensure_url_basename_matches(
    field: &str,
    url: &reqwest::Url,
    expected_name: &str,
) -> Result<(), ServiceError> {
    let basename = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or_default();
    let basename_field = format!("{field} basename");
    ensure_safe_file_name(&basename_field, basename)?;
    if !basename.eq_ignore_ascii_case(expected_name) {
        return Err(errors::failed(format!(
            "{field} basename `{basename}` must match canonical local add-on `{expected_name}`"
        )));
    }
    Ok(())
}

fn ensure_stable_reshade_download(field: &str, url: &str) -> Result<(), ServiceError> {
    let parsed = ensure_https(field, url)?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(errors::failed(format!("{field} must not include userinfo")));
    }
    if parsed.host_str() != Some("reshade.me") {
        return Err(errors::failed(format!("{field} host must be `reshade.me`")));
    }
    let path = parsed.path();
    if !path.starts_with("/downloads/ReShade_Setup_") || !path.ends_with("_Addon.exe") {
        return Err(errors::failed(format!(
            "{field} must point at `/downloads/ReShade_Setup_*_Addon.exe`"
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
    use crate::addons::renodx::types::{Category, Engine, Generic, MatchKind, Status};

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

    #[test]
    fn generic_accepts_canonical_slug_with_explicit_urls() {
        let mut m = one_title_manifest();
        m.generics = vec![Generic {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64".to_owned()),
            url32: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon32".to_owned()),
            label_key: Some("renodx.generic.unity".to_owned()),
        }];

        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn generic_validates_explicit_urls_even_with_slug() {
        let mut m = one_title_manifest();
        m.generics = vec![Generic {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("http://example.com/renodx-unityengine.addon64".to_owned()),
            url32: Some("https://example.com/renodx-unityengine.addon32".to_owned()),
            label_key: Some("renodx.generic.unity".to_owned()),
        }];

        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn generic_rejects_explicit_url_basename_mismatch() {
        let mut m = one_title_manifest();
        m.generics = vec![Generic {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unity.addon64".to_owned()),
            url32: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon32".to_owned()),
            label_key: Some("renodx.generic.unity".to_owned()),
        }];

        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn generic_explicit_urls_must_be_paired_even_with_slug() {
        let mut m = one_title_manifest();
        m.generics = vec![Generic {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64".to_owned()),
            url32: None,
            label_key: Some("renodx.generic.unity".to_owned()),
        }];

        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn generic_without_slug_requires_both_explicit_urls() {
        let mut m = one_title_manifest();
        m.generics = vec![Generic {
            engine: Engine::Unity,
            status: Status::Working,
            slug: None,
            url64: Some("https://example.com/renodx-unityengine.addon64".to_owned()),
            url32: None,
            label_key: Some("renodx.generic.unity".to_owned()),
        }];

        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn legacy_generic_without_slug_uses_engine_fallback_as_local_identity() {
        let mut m = one_title_manifest();
        m.generics = vec![Generic {
            engine: Engine::Unity,
            status: Status::Working,
            slug: None,
            url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unity.addon64".to_owned()),
            url32: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unity.addon32".to_owned()),
            label_key: Some("renodx.generic.unity".to_owned()),
        }];
        assert!(validate_manifest(&m).is_ok());

        m.generics[0].url64 = Some(
            "https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64"
                .to_owned(),
        );
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn title_download_url_basename_must_match_canonical_file_name() {
        let mut m = one_title_manifest();
        m.titles[0].download_url = Some("https://example.com/renodx-slugx.addon64".to_owned());
        assert!(validate_manifest(&m).is_ok());

        m.titles[0].download_url = Some("https://example.com/renodx-other.addon64".to_owned());
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn stable_reshade_requires_official_addon_download_shape() {
        let mut m = one_title_manifest();
        m.reshade.stable.as_mut().expect("stable").url =
            "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe".to_owned();
        assert!(validate_manifest(&m).is_ok());

        m.reshade.stable.as_mut().expect("stable").url =
            "https://example.com/downloads/ReShade_Setup_6.7.3_Addon.exe".to_owned();
        assert!(validate_manifest(&m).is_err());

        m.reshade.stable.as_mut().expect("stable").url =
            "https://reshade.me/downloads/ReShade_Setup_6.7.3.exe".to_owned();
        assert!(validate_manifest(&m).is_err());
    }
}
