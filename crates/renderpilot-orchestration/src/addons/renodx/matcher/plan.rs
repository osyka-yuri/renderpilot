use renderpilot_domain::Architecture;

use super::super::policy::check_title_compatibility;
use super::super::source;
use super::super::types::{Category, MatchRule, RenoDxManifest, Title};
use super::types::{RenoDxResolution, ResolvedInstall};
use crate::addons::matching::{
    IncompatibilityReason, MatchConfidence, MatchFacts, confidence_for_match,
    confidence_for_status, select_title,
};
use crate::addons::reshade::proxy::{HostKind, host_decision, primary_api, resolve_proxy_dll};

/// Resolves a game to a RenoDX outcome against a validated manifest.
#[must_use]
pub fn resolve(manifest: &RenoDxManifest, facts: &MatchFacts) -> RenoDxResolution {
    match select_title(&manifest.titles, facts) {
        Some((title, rule)) => resolve_title(manifest, title, rule, facts),
        None => resolve_generic(manifest, facts),
    }
}

/// Routes a matched title by its [`Category`]: a categorized game (external link /
/// native HDR / blacklist) takes its branch; an installable game is gated by its
/// compatibility constraints.
fn resolve_title(
    manifest: &RenoDxManifest,
    title: &Title,
    rule: &MatchRule,
    facts: &MatchFacts,
) -> RenoDxResolution {
    match &title.category {
        Category::Blacklist { reason } => RenoDxResolution::Unsupported {
            reason: Some(reason.clone()),
        },
        Category::NativeHdr => RenoDxResolution::NativeHdr,
        Category::External { url, label_key } => {
            // A compatible external game can be installed from a user-downloaded file.
            let file_install = build_install_plan(manifest, title, rule, facts)
                .ok()
                .map(|plan| Box::new(plan.into_external_install()));
            RenoDxResolution::External {
                url: url.clone(),
                label_key: label_key.clone(),
                file_install,
            }
        }
        Category::Installable => match build_install_plan(manifest, title, rule, facts) {
            Ok(plan) => RenoDxResolution::Installable(Box::new(plan)),
            Err(reason) => RenoDxResolution::Incompatible { reason },
        },
    }
}

/// Builds the full install plan for a matched, compatible title, or the reason it
/// is incompatible. Shared by the standard install path and the external
/// file-install path.
fn build_install_plan(
    manifest: &RenoDxManifest,
    title: &Title,
    rule: &MatchRule,
    facts: &MatchFacts,
) -> Result<ResolvedInstall, IncompatibilityReason> {
    let (host_kind, proxy_dll_name) = check_title_compatibility(title, facts)?;
    Ok(ResolvedInstall {
        slug: title.slug.clone(),
        addon_url: title_addon_url(manifest, title),
        arch: title.arch,
        host_kind,
        proxy_dll_name,
        confidence: confidence_for_match(title.status, rule.kind),
        notes_keys: title.notes_keys.clone(),
    })
}

/// Resolves a title's upstream add-on URL: an explicit per-title download URL
/// (third-party host) wins; otherwise, when the title's slug names one of the
/// manifest's engine-generics (a per-game title pointing at a universal add-on,
/// e.g. a Unity game curated onto the `unityengine` generic), that generic's
/// resolved URL is used — generics can carry an explicit non-clshortfuse host
/// (see [`source::generic_addon_url`]), and a title must not bypass that by
/// re-deriving a clshortfuse URL from the same slug. Only a slug that matches
/// neither falls back to the clshortfuse URL derived from the slug itself.
fn title_addon_url(manifest: &RenoDxManifest, title: &Title) -> String {
    if let Some(url) = title.download_url.clone() {
        return url;
    }
    if let Some(generic) = manifest
        .generics
        .iter()
        .find(|generic| generic.slug.as_deref() == Some(title.slug.as_str()))
        && let Some(url) = source::generic_addon_url(generic, title.arch)
    {
        return url;
    }
    source::addon_url(&title.slug, title.arch)
}

/// Resolves a game to the install plan for an **external** file-install, or `None`
/// when the game is not a compatible external title (so the caller rejects it).
/// Mirrors [`resolve`]'s precedence: blacklist and native-HDR titles are excluded.
#[must_use]
pub fn resolve_external_install(
    manifest: &RenoDxManifest,
    facts: &MatchFacts,
) -> Option<ResolvedInstall> {
    let (title, rule) = select_title(&manifest.titles, facts)?;
    if !matches!(title.category, Category::External { .. }) {
        return None;
    }
    build_install_plan(manifest, title, rule, facts).ok()
}

/// Whether RenoDX can be installed for this game from a user-supplied add-on file.
/// True for a DirectX or inconclusive renderer (a per-game proxy) and for a confirmed
/// Vulkan renderer (the shared Vulkan layer); only a confirmed OpenGL renderer is
/// refused. Backs the manual-install escape hatch for games with no automatic or
/// curated-external path.
#[must_use]
pub fn file_installable(facts: &MatchFacts) -> bool {
    host_decision(primary_api(&facts.graphics)).is_some()
}

/// The catalogue add-on slug for this game, if a title matches — so a manual install
/// can show the expected file name (`renodx-<slug>.addon*`) for a soft check.
/// `None` for an unrecognized game.
#[must_use]
pub fn matched_slug(manifest: &RenoDxManifest, facts: &MatchFacts) -> Option<String> {
    select_title(&manifest.titles, facts).map(|(title, _)| title.slug.clone())
}

/// A generic install plan for a manual file install when no catalogue title matched:
/// the host is the proxy DLL the game loads (Direct3D) or the shared Vulkan layer
/// (confirmed Vulkan), `arch` is what the user's add-on targets. `None` when the
/// renderer is confirmed OpenGL (no
/// host RenoDX can drive).
#[must_use]
pub fn generic_file_install_plan(
    facts: &MatchFacts,
    arch: Architecture,
) -> Option<ResolvedInstall> {
    let host_kind = host_decision(primary_api(&facts.graphics))?;
    Some(ResolvedInstall {
        slug: String::new(),
        addon_url: String::new(),
        arch,
        host_kind,
        proxy_dll_name: match host_kind {
            HostKind::Proxy => resolve_proxy_dll(None, &facts.graphics),
            HostKind::Vulkan => String::new(),
        },
        confidence: MatchConfidence::Untested,
        notes_keys: Vec::new(),
    })
}

/// Engine-generic fallback when no per-game title matched.
fn resolve_generic(manifest: &RenoDxManifest, facts: &MatchFacts) -> RenoDxResolution {
    let Some(engine) = facts.engine else {
        return RenoDxResolution::NoMatch;
    };
    let Some(generic) = manifest.generics.iter().find(|g| g.engine == engine) else {
        return RenoDxResolution::NoMatch;
    };

    let api = primary_api(&facts.graphics);
    // Pick the host from the renderer: Direct3D / inconclusive → a proxy DLL (the
    // engine signal, e.g. `UnityPlayer.dll`, implies a DirectX renderer on Windows);
    // confirmed Vulkan -> the shared Vulkan layer; confirmed OpenGL -> unsupported.
    let Some(host_kind) = host_decision(api) else {
        return RenoDxResolution::Incompatible {
            reason: IncompatibilityReason::ApiUnsupported { detected: api },
        };
    };
    let Some(arch) = facts.graphics.architecture() else {
        return RenoDxResolution::Incompatible {
            reason: IncompatibilityReason::ArchUnknown,
        };
    };
    let Some(addon_url) = source::generic_addon_url(generic, arch) else {
        return RenoDxResolution::NoMatch;
    };

    RenoDxResolution::Installable(Box::new(ResolvedInstall {
        // Prefer the manifest's canonical slug for the local file name. Legacy
        // explicit-URL generics without a slug fall back to the engine key.
        slug: generic
            .slug
            .clone()
            .unwrap_or_else(|| engine.as_str().to_owned()),
        addon_url,
        arch,
        host_kind,
        proxy_dll_name: match host_kind {
            HostKind::Proxy => resolve_proxy_dll(None, &facts.graphics),
            HostKind::Vulkan => String::new(),
        },
        confidence: confidence_for_status(generic.status),
        // The generic's engine label is surfaced as a note so the card can flag
        // "this is a universal, not per-game, add-on".
        notes_keys: generic.label_key.clone().into_iter().collect(),
    }))
}
