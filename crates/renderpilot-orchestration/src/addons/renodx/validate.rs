//! Validation for the RenoDX overrides manifest.
//!
//! With add-ons fetched live from upstream there are no artifacts/hashes to
//! cross-check; validation now enforces a supported schema, well-formed slugs and
//! match rules, sane risk metadata, and that explicit add-on URL basenames match
//! the canonical local file name derived from the slug. A manifest that passes can
//! be resolved and installed without further structural checks.

use renderpilot_domain::{Architecture, RenoDxManagedConfigKey};
use std::collections::HashSet;

use crate::ServiceError;

use super::errors;
use super::source;
use super::types::{
    RenoDxCategory, RenoDxConfig, RenoDxEngineIniRecipe, RenoDxGeneric, RenoDxGuidance,
    RenoDxGuidanceKind, RenoDxManifest, RenoDxTitle,
};
use crate::addons::manifest_validate::{
    ensure_not_blank, ensure_safe_file_name, ensure_unique_title_ids, validate_match_rules,
};

/// Schema version this build understands.
const SUPPORTED_SCHEMA_VERSIONS: &[u32] = &[1, 2];

/// Hosts a RenoDX add-on or ReShade build may be downloaded from.
const DOWNLOAD_HOST_ALLOWLIST: &[&str] = &[
    "renodx.com",
    "github.com",
    "nightly.link",
    "marat569.github.io",
];

/// Validates an entire manifest.
pub(super) fn validate_manifest(manifest: &RenoDxManifest) -> Result<(), ServiceError> {
    if !SUPPORTED_SCHEMA_VERSIONS.contains(&manifest.schema_version) {
        return Err(errors::failed(format!(
            "RenoDX schema version {} is unsupported",
            manifest.schema_version
        )));
    }
    ensure_not_blank("manifest generated_at", &manifest.generated_at)?;

    for generic in &manifest.generics {
        validate_generic(generic)?;
    }
    validate_profile_ids(manifest)?;
    validate_generic_fallbacks(manifest)?;

    for title in &manifest.titles {
        validate_title(title)?;
    }
    ensure_unique_title_ids(manifest.titles.iter().map(|title| title.id.as_str()))?;

    for guidance in &manifest.page_guidance {
        validate_guidance(guidance, "page guidance")?;
    }
    for (id, guidance) in &manifest.title_guidance {
        let context = format!("title `{id}` guidance");
        for item in guidance {
            validate_guidance(item, &context)?;
        }
    }

    Ok(())
}

fn validate_profile_ids(manifest: &RenoDxManifest) -> Result<(), ServiceError> {
    let mut profiles = HashSet::new();
    for generic in &manifest.generics {
        let Some(profile_id) = generic.profile_id.as_deref() else {
            continue;
        };
        ensure_not_blank("generic profile_id", profile_id)?;
        if !profiles.insert(profile_id) {
            return Err(errors::failed(format!(
                "duplicate RenoDX profile_id `{profile_id}`"
            )));
        }
    }

    for title in &manifest.titles {
        if let Some(profile_id) = title.profile_id.as_deref()
            && !profiles.contains(profile_id)
        {
            return Err(errors::failed(format!(
                "title `{}` references unknown RenoDX profile_id `{profile_id}`",
                title.id
            )));
        }
    }
    Ok(())
}

fn validate_generic_fallbacks(manifest: &RenoDxManifest) -> Result<(), ServiceError> {
    let mut fallbacks = std::collections::HashMap::new();
    for generic in &manifest.generics {
        if generic.generic_fallback {
            let identifier = generic
                .profile_id
                .as_deref()
                .or(generic.slug.as_deref())
                .unwrap_or_else(|| generic.engine.as_str());
            if let Some(previous) = fallbacks.insert(generic.engine, identifier) {
                return Err(errors::failed(format!(
                    "duplicate RenoDX generic fallback for engine `{}`: conflicting profiles `{previous}` and `{identifier}`",
                    generic.engine.as_str()
                )));
            }
        }
    }
    Ok(())
}

/// Validates a title's [`RenoDxCategory`] payload: external URLs must be HTTPS and all
/// catalogue messages must retain both their id and reviewed fallback. The
/// installable and native-HDR categories carry no payload to check.
fn validate_category(category: &RenoDxCategory) -> Result<(), ServiceError> {
    match category {
        RenoDxCategory::External { url, message } => {
            ensure_https("title external url", url)?;
            message.validate("title external message")?;
        }
        RenoDxCategory::Blacklist { message } => {
            message.validate("title blacklist message")?;
        }
        RenoDxCategory::Installable | RenoDxCategory::NativeHdr => {}
    }
    Ok(())
}

fn validate_generic(generic: &RenoDxGeneric) -> Result<(), ServiceError> {
    generic.message.validate("generic message")?;
    if let Some(slug) = &generic.slug {
        ensure_slug("generic slug", slug)?;
        ensure_not_reserved_dlss_fix_slug("generic slug", slug)?;
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
    for guidance in &generic.guidance {
        validate_guidance(guidance, "generic guidance")?;
    }
    Ok(())
}

fn validate_title(title: &RenoDxTitle) -> Result<(), ServiceError> {
    ensure_not_blank("title id", &title.id)?;
    ensure_not_blank("title name", &title.name)?;
    ensure_slug("title slug", &title.slug)?;
    ensure_not_reserved_dlss_fix_slug("title slug", &title.slug)?;

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
    for conflict in &title.compatibility.conflicts {
        ensure_not_blank("title compatibility.conflicts entry", conflict)?;
    }
    ensure_compatibility_source(title)?;
    validate_category(&title.category)?;
    if let Some(config) = &title.renodx_config {
        let Some(profile_id) = title.profile_id.as_deref() else {
            return Err(errors::failed(format!(
                "title `{}` RenoDX config requires profile_id",
                title.id
            )));
        };
        validate_config(
            config,
            &title.id,
            profile_id,
            title.has_unsupported_settings,
        )?;
    }
    if let Some(launch) = &title.launch
        && (launch.arguments.is_empty() || launch.arguments.iter().any(|arg| arg.trim().is_empty()))
    {
        return Err(errors::failed(format!(
            "title `{}` launch arguments must be non-empty",
            title.id
        )));
    }
    Ok(())
}

fn validate_config(
    config: &RenoDxConfig,
    title_id: &str,
    profile_id: &str,
    allow_empty_config: bool,
) -> Result<(), ServiceError> {
    if config.settings.is_empty() && !allow_empty_config {
        return Err(errors::failed(format!(
            "title `{title_id}` RenoDX config must contain at least one setting"
        )));
    }
    let mut keys = HashSet::new();
    for setting in &config.settings {
        if !keys.insert(setting.key) {
            return Err(errors::failed(format!(
                "title `{title_id}` RenoDX config contains duplicate key `{}`",
                setting.key.as_str()
            )));
        }
        if !config_key_supported_by_profile(setting.key, profile_id) {
            return Err(errors::failed(format!(
                "title `{title_id}` RenoDX config key `{}` is incompatible with profile `{profile_id}`",
                setting.key.as_str()
            )));
        }
        let key = setting.key.managed_key();
        let valid = key != RenoDxManagedConfigKey::SetPath && key.accepts_value(setting.value);
        if !valid {
            return Err(errors::failed(format!(
                "title `{title_id}` RenoDX config value {} is invalid for `{}`",
                setting.value,
                setting.key.as_str()
            )));
        }
    }
    Ok(())
}

fn config_key_supported_by_profile(key: super::types::RenoDxConfigKey, profile: &str) -> bool {
    use super::types::RenoDxConfigKey;
    match profile {
        "ue_extended" => matches!(
            key,
            RenoDxConfigKey::UpgradeB8G8R8A8Typeless
                | RenoDxConfigKey::UpgradeB8G8R8A8Unorm
                | RenoDxConfigKey::UpgradeR8G8B8A8Typeless
                | RenoDxConfigKey::UpgradeR8G8B8A8Unorm
                | RenoDxConfigKey::UpgradeR10G10B10A2Unorm
                | RenoDxConfigKey::UpgradeR10G10B10A2Typeless
                | RenoDxConfigKey::UpgradeR11G11B10Float
                | RenoDxConfigKey::UpgradeR16G16B16A16Typeless
                | RenoDxConfigKey::UpgradeCopyDestinations
                | RenoDxConfigKey::ForcePipelineCloning
        ),
        "unreal_legacy" => !matches!(
            key,
            RenoDxConfigKey::SwapchainEncoding
                | RenoDxConfigKey::ScalingOffset
                | RenoDxConfigKey::TonemapOffset
                | RenoDxConfigKey::BlitCopyHack
                | RenoDxConfigKey::UseSwapchainProxy
                | RenoDxConfigKey::ForcePipelineCloning
        ),
        "unity" => !matches!(
            key,
            RenoDxConfigKey::ForceBorderless | RenoDxConfigKey::UpgradeUseScrgb
        ),
        _ => false,
    }
}

fn validate_guidance(guidance: &RenoDxGuidance, context: &str) -> Result<(), ServiceError> {
    ensure_not_blank_lazy(&guidance.id, || format!("{context} id"))?;
    ensure_not_blank_lazy(&guidance.message_id, || format!("{context} message_id"))?;
    ensure_not_blank_lazy(&guidance.fallback_text, || {
        format!("{context} fallback_text")
    })?;
    if matches!(guidance.kind, RenoDxGuidanceKind::EngineIni)
        && guidance
            .code
            .as_deref()
            .is_none_or(|code| code.trim().is_empty())
    {
        return Err(errors::failed(format!(
            "{context} Engine.ini guidance requires code"
        )));
    }
    if guidance.engine_ini.is_some() && !matches!(guidance.kind, RenoDxGuidanceKind::EngineIni) {
        return Err(errors::failed(format!(
            "{context} Engine.ini recipe is only valid for engine_ini guidance"
        )));
    }
    if let Some(recipe) = &guidance.engine_ini {
        validate_engine_ini_recipe(recipe, guidance.code.as_deref(), context)?;
    }
    if matches!(guidance.kind, RenoDxGuidanceKind::AddonSetting) && guidance.settings.is_empty() {
        return Err(errors::failed(format!(
            "{context} add-on setting requires settings"
        )));
    }
    if guidance
        .settings
        .iter()
        .any(|setting| setting.name.trim().is_empty() || setting.value.trim().is_empty())
    {
        return Err(errors::failed(format!("{context} has a blank setting")));
    }
    if let Some(url) = &guidance.url {
        ensure_https(&format!("{context} url"), url)?;
    }
    if let Some(condition) = &guidance.condition {
        if condition.unreal_major.is_some_and(|major| major < 4) {
            return Err(errors::failed(format!(
                "{context} Unreal major version must be at least 4"
            )));
        }
        if condition
            .unreal_minor_min
            .zip(condition.unreal_minor_max)
            .is_some_and(|(min, max)| min > max)
        {
            return Err(errors::failed(format!(
                "{context} has an inverted Unreal minor-version range"
            )));
        }
        if (condition.unreal_major.is_some()
            || condition.unreal_minor_min.is_some()
            || condition.unreal_minor_max.is_some())
            && condition.engine != Some(super::types::Engine::Unreal)
        {
            return Err(errors::failed(format!(
                "{context} Unreal version bounds require engine `unreal`"
            )));
        }
    }
    Ok(())
}

fn validate_engine_ini_recipe(
    recipe: &RenoDxEngineIniRecipe,
    code: Option<&str>,
    context: &str,
) -> Result<(), ServiceError> {
    if recipe.schema_version != 1 {
        return Err(errors::failed(format!(
            "{context} Engine.ini recipe schema_version must be 1"
        )));
    }
    if recipe.revision == 0 {
        return Err(errors::failed(format!(
            "{context} Engine.ini recipe revision must be greater than zero"
        )));
    }
    if recipe.sections.is_empty() {
        return Err(errors::failed(format!(
            "{context} Engine.ini recipe requires at least one section"
        )));
    }

    let mut section_names = HashSet::new();
    for (section_index, section) in recipe.sections.iter().enumerate() {
        let section_context = format!("{context} Engine.ini section[{section_index}]");
        validate_ini_scalar(
            &format!("{section_context} name"),
            &section.name,
            Some(&['[', ']']),
        )?;
        let section_name = section.name.to_ascii_lowercase();
        if !section_names.insert(section_name) {
            return Err(errors::failed(format!(
                "{context} Engine.ini recipe contains duplicate section names"
            )));
        }
        if section.entries.is_empty() {
            return Err(errors::failed(format!(
                "{section_context} requires at least one entry"
            )));
        }

        let mut section_keys = HashSet::new();
        for (entry_index, entry) in section.entries.iter().enumerate() {
            let entry_context = format!("{section_context} entry[{entry_index}]");
            validate_ini_scalar(&format!("{entry_context} key"), &entry.key, Some(&['=']))?;
            validate_ini_scalar(&format!("{entry_context} value"), &entry.value, None)?;
            if !section_keys.insert(entry.key.to_ascii_lowercase()) {
                return Err(errors::failed(format!(
                    "{context} Engine.ini recipe contains duplicate section/key targets"
                )));
            }
        }
    }

    let canonical = recipe.canonical_code();
    if code != Some(canonical.as_str()) {
        return Err(errors::failed(format!(
            "{context} Engine.ini code does not match its structured recipe"
        )));
    }
    Ok(())
}

#[inline]
fn ensure_not_blank_lazy(value: &str, field: impl FnOnce() -> String) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(errors::failed(format!("`{}` cannot be blank", field())));
    }
    Ok(())
}

fn validate_ini_scalar(
    field: &str,
    value: &str,
    forbidden: Option<&[char]>,
) -> Result<(), ServiceError> {
    ensure_not_blank(field, value)?;
    if value.contains(['\r', '\n']) {
        return Err(errors::failed(format!("`{field}` must be single-line")));
    }
    if forbidden.is_some_and(|characters| value.contains(characters)) {
        return Err(errors::failed(format!(
            "`{field}` contains a forbidden character"
        )));
    }
    Ok(())
}

/// A non-empty `conflicts` list must carry a `source`, so an unsourced conflict
/// claim can't reappear silently the way the historical `special_k`/Cyberpunk 2077
/// entry did.
fn ensure_compatibility_source(title: &RenoDxTitle) -> Result<(), ServiceError> {
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

fn ensure_not_reserved_dlss_fix_slug(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.eq_ignore_ascii_case(source::DLSS_FIX_SLUG) {
        return Err(errors::failed(format!(
            "`{field}` must not use reserved DLSS-Fix slug `{}`",
            source::DLSS_FIX_SLUG
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
        Engine, MatchKind, RenoDxCategory, RenoDxCompatibility, RenoDxConfig, RenoDxConfigKey,
        RenoDxConfigSetting, RenoDxEngineIniEntry, RenoDxEngineIniRecipe, RenoDxEngineIniSection,
        RenoDxGeneric, RenoDxGuidance, RenoDxGuidanceKind, Status,
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
    fn renodx_config_requires_profile_and_enforces_key_ranges() {
        let mut manifest = one_title_manifest();
        manifest.titles[0].renodx_config = Some(RenoDxConfig {
            settings: vec![RenoDxConfigSetting {
                key: RenoDxConfigKey::UpgradeR10G10B10A2Unorm,
                value: 99,
            }],
        });
        assert!(validate_manifest(&manifest).is_err());

        manifest.titles[0].renodx_config = Some(RenoDxConfig {
            settings: vec![RenoDxConfigSetting {
                key: RenoDxConfigKey::UpgradeR10G10B10A2Unorm,
                value: 2,
            }],
        });
        assert!(validate_manifest(&manifest).is_err());

        manifest.generics.push(RenoDxGeneric {
            engine: Engine::Unreal,
            status: Status::Working,
            slug: Some("unrealengine".to_owned()),
            url64: None,
            url32: None,
            message: crate::addons::CatalogMessage::new("renodx.generic.unreal", "Unreal"),
            profile_id: Some("unreal_legacy".to_owned()),
            generic_fallback: false,
            guidance: Vec::new(),
            processing_path: Default::default(),
        });
        manifest.titles[0].profile_id = Some("unreal_legacy".to_owned());
        assert!(validate_manifest(&manifest).is_ok());
    }

    #[test]
    fn force_pipeline_cloning_is_supported_by_unity_and_ue_extended_only() {
        let key = RenoDxConfigKey::ForcePipelineCloning;
        assert!(config_key_supported_by_profile(key, "unity"));
        assert!(config_key_supported_by_profile(key, "ue_extended"));
        assert!(!config_key_supported_by_profile(key, "unreal_legacy"));
        let managed = key.managed_key();
        assert_eq!(managed.value_range(), (0, 1));
        assert!(managed.accepts_value(0));
        assert!(managed.accepts_value(1));
        assert!(!managed.accepts_value(-1));
        assert!(!managed.accepts_value(2));
    }

    #[test]
    fn typed_config_keys_match_domain_canonical_contract() {
        let keys = [
            RenoDxConfigKey::UpgradeB8G8R8A8Typeless,
            RenoDxConfigKey::UpgradeB8G8R8A8Unorm,
            RenoDxConfigKey::UpgradeR8G8B8A8Typeless,
            RenoDxConfigKey::UpgradeR8G8B8A8Unorm,
            RenoDxConfigKey::UpgradeR10G10B10A2Unorm,
            RenoDxConfigKey::UpgradeR10G10B10A2Typeless,
            RenoDxConfigKey::UpgradeR11G11B10Float,
            RenoDxConfigKey::UpgradeR16G16B16A16Typeless,
            RenoDxConfigKey::UpgradeCopyDestinations,
            RenoDxConfigKey::ForceBorderless,
            RenoDxConfigKey::UpgradeUseScrgb,
            RenoDxConfigKey::SwapchainEncoding,
            RenoDxConfigKey::ScalingOffset,
            RenoDxConfigKey::TonemapOffset,
            RenoDxConfigKey::BlitCopyHack,
            RenoDxConfigKey::UseSwapchainProxy,
            RenoDxConfigKey::ForcePipelineCloning,
            RenoDxConfigKey::ColorGradeContrast,
            RenoDxConfigKey::ColorGradeSaturation,
            RenoDxConfigKey::ColorGradeBlowout,
        ];
        assert_eq!(keys.len(), RenoDxManagedConfigKey::ALL.len() - 1);
        for key in keys {
            let canonical = RenoDxManagedConfigKey::parse(key.as_str())
                .expect("every public key must be in the domain allowlist");
            assert_ne!(canonical, RenoDxManagedConfigKey::SetPath);
            let (minimum, maximum) = canonical.value_range();
            assert!(canonical.accepts_value(minimum));
            assert!(canonical.accepts_value(maximum));
            assert!(!canonical.accepts_value(minimum - 1));
            assert!(!canonical.accepts_value(maximum + 1));
        }
    }

    fn recipe() -> RenoDxEngineIniRecipe {
        RenoDxEngineIniRecipe {
            schema_version: 1,
            revision: 1,
            sections: vec![RenoDxEngineIniSection {
                name: "SystemSettings".to_owned(),
                entries: vec![RenoDxEngineIniEntry {
                    key: "r.AllowHDR".to_owned(),
                    value: "1".to_owned(),
                }],
            }],
        }
    }

    fn engine_ini_guidance(
        kind: RenoDxGuidanceKind,
        recipe: Option<RenoDxEngineIniRecipe>,
        code: Option<&str>,
    ) -> RenoDxGuidance {
        RenoDxGuidance {
            id: "engine_ini".to_owned(),
            kind,
            message_id: "engine_ini".to_owned(),
            fallback_text: "Apply this Engine.ini setting.".to_owned(),
            code: code.map(str::to_owned),
            settings: Vec::new(),
            engine_ini: recipe,
            url: None,
            condition: None,
        }
    }

    fn manifest_with_guidance(guidance: RenoDxGuidance) -> RenoDxManifest {
        let mut manifest = one_title_manifest();
        manifest.page_guidance = vec![guidance];
        manifest
    }

    #[test]
    fn structured_engine_ini_recipe_requires_canonical_code() {
        let valid = recipe();
        let code = valid.canonical_code();
        assert!(
            validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                RenoDxGuidanceKind::EngineIni,
                Some(valid),
                Some(&code),
            )))
            .is_ok()
        );

        let drifted = recipe();
        assert!(
            validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                RenoDxGuidanceKind::EngineIni,
                Some(drifted),
                Some("[SystemSettings]\nr.AllowHDR=0"),
            )))
            .is_err()
        );
    }

    #[test]
    fn manual_engine_ini_code_only_guidance_remains_valid() {
        assert!(
            validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                RenoDxGuidanceKind::EngineIni,
                None,
                Some("r.HDR.EnableHDROutput=1"),
            )))
            .is_ok()
        );
    }

    #[test]
    fn engine_ini_recipe_rejects_wrong_kind_and_invalid_metadata() {
        let canonical = recipe().canonical_code();
        assert!(
            validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                RenoDxGuidanceKind::Warning,
                Some(recipe()),
                Some(&canonical),
            )))
            .is_err()
        );

        for (schema_version, revision) in [(2, 1), (1, 0)] {
            let mut invalid = recipe();
            invalid.schema_version = schema_version;
            invalid.revision = revision;
            assert!(
                validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                    RenoDxGuidanceKind::EngineIni,
                    Some(invalid),
                    Some("[SystemSettings]\nr.AllowHDR=1"),
                )))
                .is_err()
            );
        }
    }

    #[test]
    fn engine_ini_recipe_rejects_empty_invalid_and_duplicate_targets() {
        let mut empty_sections = recipe();
        empty_sections.sections.clear();
        assert!(
            validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                RenoDxGuidanceKind::EngineIni,
                Some(empty_sections),
                Some(""),
            )))
            .is_err()
        );

        let mut empty_entries = recipe();
        empty_entries.sections[0].entries.clear();
        assert!(
            validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                RenoDxGuidanceKind::EngineIni,
                Some(empty_entries),
                Some(""),
            )))
            .is_err()
        );

        let mut duplicate_sections = recipe();
        duplicate_sections.sections.push(RenoDxEngineIniSection {
            name: "systemsettings".to_owned(),
            entries: vec![RenoDxEngineIniEntry {
                key: "other".to_owned(),
                value: "1".to_owned(),
            }],
        });
        assert!(
            validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                RenoDxGuidanceKind::EngineIni,
                Some(duplicate_sections),
                Some(""),
            )))
            .is_err()
        );

        let mut duplicate_targets = recipe();
        duplicate_targets.sections[0]
            .entries
            .push(RenoDxEngineIniEntry {
                key: "R.ALLOWHDR".to_owned(),
                value: "1".to_owned(),
            });
        assert!(
            validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                RenoDxGuidanceKind::EngineIni,
                Some(duplicate_targets),
                Some(""),
            )))
            .is_err()
        );

        for (name, key, value) in [
            ("System\nSettings", "r.AllowHDR", "1"),
            ("[SystemSettings]", "r.AllowHDR", "1"),
            ("SystemSettings", "r=AllowHDR", "1"),
            ("SystemSettings", "r.AllowHDR", "1\n0"),
        ] {
            let invalid = RenoDxEngineIniRecipe {
                schema_version: 1,
                revision: 1,
                sections: vec![RenoDxEngineIniSection {
                    name: name.to_owned(),
                    entries: vec![RenoDxEngineIniEntry {
                        key: key.to_owned(),
                        value: value.to_owned(),
                    }],
                }],
            };
            assert!(
                validate_manifest(&manifest_with_guidance(engine_ini_guidance(
                    RenoDxGuidanceKind::EngineIni,
                    Some(invalid),
                    Some(""),
                )))
                .is_err()
            );
        }
    }

    #[test]
    fn external_category_passes_with_https_url_and_label() {
        let mut m = one_title_manifest();
        m.titles[0].category = RenoDxCategory::External {
            url: "https://discord.gg/example".to_owned(),
            message: crate::addons::CatalogMessage::new(
                "renodx.external.discord",
                "Open the RenoDX Discord",
            ),
        };
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn external_category_rejects_non_https_url() {
        let mut m = one_title_manifest();
        m.titles[0].category = RenoDxCategory::External {
            url: "http://discord.gg/example".to_owned(),
            message: crate::addons::CatalogMessage::new(
                "renodx.external.discord",
                "Open the RenoDX Discord",
            ),
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn blacklist_category_requires_a_reason() {
        let mut m = one_title_manifest();
        m.titles[0].category = RenoDxCategory::Blacklist {
            message: crate::addons::CatalogMessage::new("", "Known broken"),
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn generic_accepts_canonical_slug_with_explicit_urls() {
        let mut m = one_title_manifest();
        m.generics = vec![RenoDxGeneric {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64".to_owned()),
            url32: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon32".to_owned()),
            message: crate::addons::CatalogMessage::new(
                "renodx.generic.unity",
                "Generic Unity profile",
            ),
            profile_id: None,
            generic_fallback: true,
            guidance: Vec::new(),
            processing_path: Default::default(),
        }];

        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn reserved_dlss_fix_slug_is_rejected_case_insensitively_for_titles_and_generics() {
        let mut title_manifest = one_title_manifest();
        title_manifest.titles[0].slug = "DLSSFIX".to_owned();
        assert!(validate_manifest(&title_manifest).is_err());

        let mut generic_manifest = one_title_manifest();
        generic_manifest.generics = vec![RenoDxGeneric {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("DlSsFiX".to_owned()),
            url64: None,
            url32: None,
            message: crate::addons::CatalogMessage::new(
                "renodx.generic.unity",
                "Generic Unity profile",
            ),
            profile_id: None,
            generic_fallback: true,
            guidance: Vec::new(),
            processing_path: Default::default(),
        }];
        assert!(validate_manifest(&generic_manifest).is_err());
    }

    #[test]
    fn generic_validates_explicit_urls_even_with_slug() {
        let mut m = one_title_manifest();
        m.generics = vec![RenoDxGeneric {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("http://example.com/renodx-unityengine.addon64".to_owned()),
            url32: Some("https://example.com/renodx-unityengine.addon32".to_owned()),
            message: crate::addons::CatalogMessage::new(
                "renodx.generic.unity",
                "Generic Unity profile",
            ),
            profile_id: None,
            generic_fallback: true,
            guidance: Vec::new(),
            processing_path: Default::default(),
        }];

        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn generic_rejects_explicit_url_basename_mismatch() {
        let mut m = one_title_manifest();
        m.generics = vec![RenoDxGeneric {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unity.addon64".to_owned()),
            url32: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon32".to_owned()),
            message: crate::addons::CatalogMessage::new(
                "renodx.generic.unity",
                "Generic Unity profile",
            ),
            profile_id: None,
            generic_fallback: true,
            guidance: Vec::new(),
            processing_path: Default::default(),
        }];

        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn generic_explicit_urls_must_be_paired_even_with_slug() {
        let mut m = one_title_manifest();
        m.generics = vec![RenoDxGeneric {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64".to_owned()),
            url32: None,
            message: crate::addons::CatalogMessage::new(
                "renodx.generic.unity",
                "Generic Unity profile",
            ),
            profile_id: None,
            generic_fallback: true,
            guidance: Vec::new(),
            processing_path: Default::default(),
        }];

        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn generic_without_slug_requires_both_explicit_urls() {
        let mut m = one_title_manifest();
        m.generics = vec![RenoDxGeneric {
            engine: Engine::Unity,
            status: Status::Working,
            slug: None,
            url64: Some("https://example.com/renodx-unityengine.addon64".to_owned()),
            url32: None,
            message: crate::addons::CatalogMessage::new(
                "renodx.generic.unity",
                "Generic Unity profile",
            ),
            profile_id: None,
            generic_fallback: true,
            guidance: Vec::new(),
            processing_path: Default::default(),
        }];

        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn legacy_generic_without_slug_uses_engine_fallback_as_local_identity() {
        let mut m = one_title_manifest();
        m.generics = vec![RenoDxGeneric {
            engine: Engine::Unity,
            status: Status::Working,
            slug: None,
            url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unity.addon64".to_owned()),
            url32: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unity.addon32".to_owned()),
            message: crate::addons::CatalogMessage::new(
                "renodx.generic.unity",
                "Generic Unity profile",
            ),
            profile_id: None,
            generic_fallback: true,
            guidance: Vec::new(),
            processing_path: Default::default(),
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
    fn compatibility_conflicts_with_source_passes() {
        let mut m = one_title_manifest();
        m.titles[0].compatibility = RenoDxCompatibility {
            conflicts: vec!["special_k".to_owned()],
            source: Some("https://example.test/conflict-report".to_owned()),
            ..Default::default()
        };
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn compatibility_conflicts_without_source_is_rejected() {
        let mut m = one_title_manifest();
        m.titles[0].compatibility = RenoDxCompatibility {
            conflicts: vec!["special_k".to_owned()],
            source: None,
            ..Default::default()
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn compatibility_conflicts_with_blank_source_is_rejected() {
        let mut m = one_title_manifest();
        m.titles[0].compatibility = RenoDxCompatibility {
            conflicts: vec!["special_k".to_owned()],
            source: Some("   ".to_owned()),
            ..Default::default()
        };
        assert!(validate_manifest(&m).is_err());
    }

    #[test]
    fn empty_compatibility_conflicts_does_not_require_source() {
        let mut m = one_title_manifest();
        m.titles[0].compatibility = RenoDxCompatibility {
            conflicts: Vec::new(),
            source: None,
            ..Default::default()
        };
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn duplicate_generic_fallback_for_same_engine_names_conflicting_profiles() {
        let mut m = one_title_manifest();
        m.generics = vec![
            RenoDxGeneric {
                engine: crate::addons::matching::Engine::Unreal,
                status: Status::Working,
                slug: Some("ue-generic".to_owned()),
                url64: None,
                url32: None,
                message: crate::addons::CatalogMessage::new("m1", "Generic UE"),
                profile_id: Some("profile-ue-generic".to_owned()),
                generic_fallback: true,
                guidance: Vec::new(),
                processing_path: Default::default(),
            },
            RenoDxGeneric {
                engine: crate::addons::matching::Engine::Unreal,
                status: Status::Working,
                slug: Some("ue-extended".to_owned()),
                url64: None,
                url32: None,
                message: crate::addons::CatalogMessage::new("m2", "Generic UE Extended"),
                profile_id: Some("profile-ue-extended".to_owned()),
                generic_fallback: true,
                guidance: Vec::new(),
                processing_path: Default::default(),
            },
        ];
        let error = validate_manifest(&m).expect_err("duplicate fallback must fail");
        assert_eq!(
            error.to_string(),
            "duplicate RenoDX generic fallback for engine `unreal`: conflicting profiles `profile-ue-generic` and `profile-ue-extended`"
        );
    }
}
