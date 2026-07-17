//! Per-title Luma manifest field validation.

use renderpilot_domain::Architecture;

use crate::ServiceError;

use super::super::errors;
use super::super::types::{LumaCategory, LumaGuidanceKind, LumaTitle};
use super::external::validate_external_requirement;
use crate::addons::manifest_validate::{
    ensure_not_blank, ensure_safe_file_name, validate_match_rules,
};

/// Required prefix of every curated Luma asset file name.
const ASSET_PREFIX: &str = "Luma-";
/// Required suffix of every curated Luma asset file name.
const ASSET_SUFFIX: &str = ".zip";
/// Curation excludes non-Publishing builds; a name ending in either marker
/// (case-insensitive) is rejected outright.
const ASSET_FORBIDDEN_MARKERS: &[&str] = &["-test", "-dev"];
/// The 32-bit asset-name marker, present iff a title's `arch` is [`Architecture::X86`].
const ASSET_X32_SUFFIX: &str = "-x32";
pub(super) fn validate_title(title: &LumaTitle) -> Result<(), ServiceError> {
    ensure_not_blank("title id", &title.id)?;
    ensure_not_blank("title name", &title.name)?;
    ensure_asset(title)?;
    ensure_addon_file(title)?;
    validate_match_rules(&title.id, &title.match_rules)?;

    if let LumaCategory::Blacklist { message } = &title.category {
        message.validate("title blacklist message")?;
    }
    if let Some(requirement) = &title.external_requirement {
        validate_external_requirement(&title.id, requirement)?;
    }
    validate_features(title)?;
    validate_guidance(title)?;

    Ok(())
}

fn validate_features(title: &LumaTitle) -> Result<(), ServiceError> {
    use super::super::types::is_generic_unreal_asset;

    if is_generic_unreal_asset(&title.asset) && !title.profile.is_generic_unreal() {
        return Err(errors::failed(format!(
            "Generic UE asset on title `{}` must use an Unreal engine profile",
            title.id
        )));
    }
    let is_generic_ue = title.is_generic_unreal();
    if is_generic_ue && title.features.is_none() {
        return Err(errors::failed(format!(
            "Generic UE title `{}` must include features",
            title.id
        )));
    }
    if !is_generic_ue && title.features.is_some() {
        return Err(errors::failed(format!(
            "title `{}` features are only valid for Generic UE profiles",
            title.id
        )));
    }
    Ok(())
}

fn validate_guidance(title: &LumaTitle) -> Result<(), ServiceError> {
    for guidance in &title.guidance {
        ensure_not_blank("guidance id", &guidance.id)?;
        ensure_not_blank("guidance fallback_text", &guidance.fallback_text)?;
        let needs_code = matches!(
            guidance.kind,
            LumaGuidanceKind::EngineIni | LumaGuidanceKind::LaunchArgument
        );
        if needs_code != guidance.code.is_some() {
            return Err(errors::failed(format!(
                "title `{}` guidance `{}` {} include code",
                title.id,
                guidance.id,
                if needs_code { "must" } else { "must not" }
            )));
        }
        if let Some(code) = &guidance.code {
            ensure_not_blank("guidance code", code)?;
        }
    }
    Ok(())
}

fn ensure_addon_file(title: &LumaTitle) -> Result<(), ServiceError> {
    let field = format!("title `{}` addon_file", title.id);
    ensure_safe_file_name(&field, &title.addon_file)?;
    let lower = title.addon_file.to_ascii_lowercase();
    if !lower.starts_with("luma-") || !lower.ends_with(".addon") {
        return Err(errors::failed(format!(
            "{field} `{}` must be a root Luma .addon file",
            title.addon_file
        )));
    }
    if title.addon_file["Luma-".len()..title.addon_file.len() - ".addon".len()]
        .trim()
        .is_empty()
    {
        return Err(errors::failed(format!(
            "{field} must include a name between `Luma-` and `.addon`"
        )));
    }
    Ok(())
}

/// Asserts `title.asset` matches Luma's own release-naming convention
/// (`Luma-<name>[-x32].zip`, no `-Test`/`-Dev` build markers -- those are
/// curated out) and that its `-x32` suffix agrees with the title's declared
/// architecture.
fn ensure_asset(title: &LumaTitle) -> Result<(), ServiceError> {
    let Some(stem) = title
        .asset
        .strip_prefix(ASSET_PREFIX)
        .and_then(|rest| rest.strip_suffix(ASSET_SUFFIX))
        .filter(|stem| !stem.is_empty())
    else {
        return Err(errors::failed(format!(
            "title `{}` asset `{}` must match `{ASSET_PREFIX}<name>[-x32]{ASSET_SUFFIX}`",
            title.id, title.asset
        )));
    };

    let lower = stem.to_ascii_lowercase();
    if ASSET_FORBIDDEN_MARKERS
        .iter()
        .any(|marker| lower.ends_with(marker))
    {
        return Err(errors::failed(format!(
            "title `{}` asset `{}` is a non-Publishing build (-Test/-Dev); only Publishing assets are curated",
            title.id, title.asset
        )));
    }

    let is_x32 = stem.ends_with(ASSET_X32_SUFFIX);
    let name_part = stem.strip_suffix(ASSET_X32_SUFFIX).unwrap_or(stem);
    if name_part.is_empty()
        || !name_part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '(' | ')' | '\'' | '-'))
    {
        return Err(errors::failed(format!(
            "title `{}` asset `{}` has an invalid name component",
            title.id, title.asset
        )));
    }

    let expected_x32 = title.arch == Architecture::X86;
    if is_x32 != expected_x32 {
        return Err(errors::failed(format!(
            "title `{}` asset `{}` `-x32` suffix must agree with arch ({:?})",
            title.id, title.asset, title.arch
        )));
    }

    Ok(())
}
