//! Validation for the RenoDX overrides manifest.
//!
//! With add-ons fetched live from upstream there are no artifacts/hashes to
//! cross-check; validation now enforces a supported schema, well-formed slugs and
//! match rules, sane risk metadata, and that explicit add-on URL basenames match
//! the canonical local file name derived from the slug. A manifest that passes can
//! be resolved and installed without further structural checks.

use renderpilot_domain::Architecture;

use crate::ServiceError;

use super::errors;
use super::source;
use super::types::{Category, Generic, RenoDxManifest, Title, manifest_defaults};
use crate::addons::manifest_validate::{
    ensure_defaults_match, ensure_not_blank, ensure_safe_file_name, ensure_schema_version,
    ensure_semver, ensure_unique_title_ids, validate_match_rules,
};
use crate::addons::reshade::types::ReshadeConfig;

/// Schema version this build understands.
const SUPPORTED_SCHEMA_VERSION: u32 = 3;

/// Hosts a RenoDX add-on or ReShade build may be downloaded from.
const DOWNLOAD_HOST_ALLOWLIST: &[&str] = &["clshortfuse.github.io", "github.com", "nightly.link"];

/// Validates an entire manifest.
pub(super) fn validate_manifest(manifest: &RenoDxManifest) -> Result<(), ServiceError> {
    ensure_schema_version("RenoDX", manifest.schema_version, SUPPORTED_SCHEMA_VERSION)?;

    // The manifest's shared `defaults` (schema v3) must agree with the Rust-side
    // `#[serde(default)]` values used to fill omitted title fields; a drift would
    // silently change install behaviour, so catch it at load time.
    ensure_defaults_match("RenoDX", &manifest.defaults, &manifest_defaults())?;

    validate_reshade(&manifest.reshade)?;

    for generic in &manifest.generics {
        validate_generic(generic)?;
    }

    for title in &manifest.titles {
        validate_title(title)?;
    }
    ensure_unique_title_ids(manifest.titles.iter().map(|title| title.id.as_str()))?;

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

// Delegates to the shared checks in `addons::reshade::manifest`, so RenoDX's
// own embedded `reshade` block is held to exactly the same shape as the
// standalone `reshade_manifest.json` (see that module's doc comment).
fn validate_reshade(reshade: &ReshadeConfig) -> Result<(), ServiceError> {
    if let Some(stable) = &reshade.stable {
        crate::addons::reshade::manifest::ensure_stable_reshade_download(
            "reshade stable url",
            &stable.url,
        )?;
    }
    crate::addons::reshade::manifest::ensure_allowed_nightly_download(
        "reshade nightly url64",
        &reshade.nightly.url64,
    )?;
    crate::addons::reshade::manifest::ensure_allowed_nightly_download(
        "reshade nightly url32",
        &reshade.nightly.url32,
    )?;
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

    validate_match_rules(&title.id, &title.match_rules)?;

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
    ensure_semver(
        &format!("title `{}`", title.id),
        "min_app_version",
        &title.min_app_version,
    )?;
    for conflict in &title.compatibility.conflicts {
        ensure_not_blank("title compatibility.conflicts entry", conflict)?;
    }
    ensure_compatibility_source(title)?;
    validate_category(&title.category)?;
    Ok(())
}

/// A non-empty `conflicts` list must carry a `source`, so an unsourced conflict
/// claim can't reappear silently the way the historical `special_k`/Cyberpunk 2077
/// entry did.
fn ensure_compatibility_source(title: &Title) -> Result<(), ServiceError> {
    if title.compatibility.conflicts.is_empty() {
        return Ok(());
    }
    match &title.compatibility.source {
        Some(source) if !source.trim().is_empty() => Ok(()),
        _ => Err(errors::failed(format!(
            "title `{}` compatibility.conflicts is non-empty but compatibility.source is missing",
            title.id
        ))),
    }
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

#[cfg(test)]
mod tests {
    use renderpilot_domain::Architecture;

    use super::*;
    use crate::addons::renodx::test_support::{manifest, rule, title};
    use crate::addons::renodx::types::{
        Category, Compatibility, Engine, Generic, MatchKind, Status,
    };

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

    #[test]
    fn compatibility_conflicts_with_source_passes() {
        let mut m = one_title_manifest();
        m.titles[0].compatibility = Compatibility {
            conflicts: vec!["special_k".to_owned()],
            source: Some("https://example.test/conflict-report".to_owned()),
            ..Default::default()
        };
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn compatibility_conflicts_without_source_is_rejected() {
        let mut m = one_title_manifest();
        m.titles[0].compatibility = Compatibility {
            conflicts: vec!["special_k".to_owned()],
            source: None,
            ..Default::default()
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn compatibility_conflicts_with_blank_source_is_rejected() {
        let mut m = one_title_manifest();
        m.titles[0].compatibility = Compatibility {
            conflicts: vec!["special_k".to_owned()],
            source: Some("   ".to_owned()),
            ..Default::default()
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn empty_compatibility_conflicts_does_not_require_source() {
        let mut m = one_title_manifest();
        m.titles[0].compatibility = Compatibility {
            conflicts: Vec::new(),
            source: None,
            ..Default::default()
        };
        assert!(validate_manifest(&m).is_ok());
    }
}
