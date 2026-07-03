//! Deterministic, explainable resolver from an installed game to a RenoDX outcome.
//!
//! [`MatchFacts`] about a game are evaluated against every [`Title`]'s ordered,
//! tiered rules; the highest-specificity match wins by a documented tie-break. The
//! winner is then routed by its category (external / native-HDR / blacklist) or,
//! for a standard install, gated by its compatibility constraints. When no per-game
//! title matches, an **engine-generic** fallback is tried from the detected engine.
//! The resolution is owned (it clones the few fields the install needs) so
//! downstream layers carry no manifest borrow.

use renderpilot_domain::{Architecture, ExeGraphicsInfo, GraphicsApi, Launcher};
use serde::Serialize;

use super::policy::{
    HostKind, check_title_compatibility, generic_risk, host_decision, primary_api,
    resolve_proxy_dll,
};
use super::source;
use super::types::{
    Category, Channel, Engine, MatchKind, MatchRule, RenoDxManifest, Risk, Status, Title,
};

/// Facts about an installed game that match rules are evaluated against.
#[derive(Debug, Clone)]
pub struct MatchFacts {
    /// Launcher that owns the game.
    pub launcher: Launcher,
    /// Launcher-specific id (Steam AppID, Epic catalog id, GOG product id).
    pub external_id: Option<String>,
    /// Game executable file name (for example `Cyberpunk2077.exe`).
    pub exe_file_name: Option<String>,
    /// Lowercase SHA-256 hex of the game executable, when computed.
    pub exe_sha256: Option<String>,
    /// Detected engine identifier (for example `unreal`, `unity`).
    pub engine: Option<String>,
    /// Graphics API and architecture detected from the executable.
    pub graphics: ExeGraphicsInfo,
}

/// How confident we are that an install will work, from the wiki test-map status
/// and how the match was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchConfidence {
    /// Listed and verified working.
    Verified,
    /// Listed but work-in-progress / experimental.
    Experimental,
    /// Listed-but-untested, or matched only by engine (a generic guess).
    Untested,
}

/// Reason a matched game cannot be installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum IncompatibilityReason {
    /// The detected API is not one RenoDX (a DirectX engine) can target.
    ApiUnsupported {
        /// API detected from the executable.
        detected: GraphicsApi,
    },
    /// The detected API is supported by RenoDX but not allowed by this title.
    ApiNotAllowed {
        /// API detected from the executable.
        detected: GraphicsApi,
        /// APIs the title declares support for.
        required: Vec<GraphicsApi>,
    },
    /// The executable architecture could not be determined.
    ArchUnknown,
}

/// A matched, compatible game resolved to everything an install needs (owned).
#[derive(Debug, Clone)]
pub struct ResolvedInstall {
    /// Upstream add-on slug.
    pub slug: String,
    /// Derived upstream add-on download URL.
    pub addon_url: String,
    /// Architecture of the add-on / executable.
    pub arch: Architecture,
    /// How RenoDX hooks into this game: a per-game proxy DLL or the global Vulkan
    /// layer. Determines which install path the service drives.
    pub host_kind: HostKind,
    /// Proxy DLL file name to install ReShade as. Meaningful only for
    /// [`HostKind::Proxy`]; empty for a [`HostKind::Vulkan`] install (the host is
    /// the global layer, not a file in the game folder).
    pub proxy_dll_name: String,
    /// Risk assessment to gate the install on.
    pub risk: Risk,
    /// Confidence shown to the user.
    pub confidence: MatchConfidence,
    /// i18n note/requirement keys (a generic install carries its engine label here).
    pub notes_keys: Vec<String>,
}

impl ResolvedInstall {
    /// Projects the user-facing fields of a plan into an [`ExternalInstall`] for a
    /// file-installable external title.
    fn into_external_install(self) -> ExternalInstall {
        ExternalInstall {
            arch: self.arch,
            proxy_dll_name: self.proxy_dll_name,
            confidence: self.confidence,
            risk: self.risk,
            notes_keys: self.notes_keys,
            host_kind: self.host_kind,
        }
    }
}

/// What a file-installable external title offers the UI alongside the link, so a
/// user who downloaded the add-on can install it locally. Built only when the game
/// is compatible; risk is the raw manifest risk (the service assesses it).
#[derive(Debug, Clone)]
pub struct ExternalInstall {
    /// Architecture of the add-on / executable.
    pub arch: Architecture,
    /// Proxy DLL file name to install ReShade as.
    pub proxy_dll_name: String,
    /// Confidence shown to the user.
    pub confidence: MatchConfidence,
    /// Raw ban/stability risk to gate the install on (assessed by the service).
    pub risk: Risk,
    /// i18n note/requirement keys.
    pub notes_keys: Vec<String>,
    /// How RenoDX would hook into this game (proxy DLL or the shared Vulkan layer),
    /// so the file-install path drives the right one.
    pub host_kind: HostKind,
}

/// Outcome of resolving a game against the manifest.
#[derive(Debug, Clone)]
pub enum RenoDxResolution {
    /// A compatible game matched; the install plan is ready.
    Installable(Box<ResolvedInstall>),
    /// The add-on is distributed off-GitHub; link the user out, and — when the
    /// game is compatible — let them install a file they downloaded themselves.
    External {
        /// Where to send the user (Discord/Nexus).
        url: String,
        /// i18n label key for the link.
        label_key: String,
        /// Present when the game is compatible, enabling "install from file".
        file_install: Option<Box<ExternalInstall>>,
    },
    /// The game already has native HDR; RenoDX is not offered.
    NativeHdr,
    /// A title matched but cannot be installed for this game.
    Incompatible {
        /// Why it cannot be installed.
        reason: IncompatibilityReason,
    },
    /// The game is blacklisted / known-broken.
    Unsupported {
        /// i18n reason key, when the manifest gives one.
        reason: Option<String>,
    },
    /// Nothing matched the game.
    NoMatch,
}

/// Resolves a game to a RenoDX outcome against a validated manifest.
#[must_use]
pub fn resolve(manifest: &RenoDxManifest, facts: &MatchFacts) -> RenoDxResolution {
    match select_title(manifest, facts) {
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
        risk: title.risk.clone(),
        confidence: confidence_for(title.status, rule.kind),
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
    let (title, rule) = select_title(manifest, facts)?;
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
    select_title(manifest, facts).map(|(title, _)| title.slug.clone())
}

/// A generic install plan for a manual file install when no catalogue title matched:
/// the host is the proxy DLL the game loads (Direct3D) or the shared Vulkan layer
/// (confirmed Vulkan), `arch` is what the user's add-on targets, and risk is the
/// conservative generic assessment. `None` when the renderer is confirmed OpenGL (no
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
        risk: generic_risk(),
        confidence: MatchConfidence::Untested,
        notes_keys: Vec::new(),
    })
}

/// Engine-generic fallback when no per-game title matched.
fn resolve_generic(manifest: &RenoDxManifest, facts: &MatchFacts) -> RenoDxResolution {
    let Some(engine) = facts.engine.as_deref().and_then(parse_engine) else {
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
        risk: generic_risk(),
        confidence: confidence_for_status(generic.status),
        // The generic's engine label is surfaced as a note so the card can flag
        // "this is a universal, not per-game, add-on".
        notes_keys: generic.label_key.clone().into_iter().collect(),
    }))
}

/// Selects the best `(title, matching-rule)` for the facts: highest rule tier,
/// then more-stable channel, then lexicographically smallest title id.
fn select_title<'m>(
    manifest: &'m RenoDxManifest,
    facts: &MatchFacts,
) -> Option<(&'m Title, &'m MatchRule)> {
    manifest
        .titles
        .iter()
        .filter_map(|title| best_matching_rule(title, facts).map(|rule| (title, rule)))
        .max_by(|left, right| selection_key(left).cmp(&selection_key(right)))
}

fn selection_key<'a>(
    candidate: &(&'a Title, &'a MatchRule),
) -> (u32, std::cmp::Reverse<u8>, std::cmp::Reverse<&'a str>) {
    let (title, rule) = *candidate;
    (
        rule.tier,
        std::cmp::Reverse(channel_rank(title.channel)),
        std::cmp::Reverse(title.id.as_str()),
    )
}

fn best_matching_rule<'m>(title: &'m Title, facts: &MatchFacts) -> Option<&'m MatchRule> {
    title
        .match_rules
        .iter()
        .filter(|rule| rule_matches(rule.kind, &rule.value, facts))
        .max_by_key(|rule| rule.tier)
}

fn rule_matches(kind: MatchKind, value: &str, facts: &MatchFacts) -> bool {
    match kind {
        MatchKind::SteamAppid => facts.launcher == Launcher::Steam && external_id_eq(facts, value),
        MatchKind::EpicId => facts.launcher == Launcher::Epic && external_id_eq(facts, value),
        MatchKind::GogId => facts.launcher == Launcher::Gog && external_id_eq(facts, value),
        MatchKind::ExeSha256 => facts
            .exe_sha256
            .as_deref()
            .is_some_and(|hash| hash.eq_ignore_ascii_case(value.trim())),
        MatchKind::ExeName => facts
            .exe_file_name
            .as_deref()
            .is_some_and(|name| glob_matches_ci(value, name)),
        MatchKind::Engine => facts
            .engine
            .as_deref()
            .is_some_and(|engine| engine.eq_ignore_ascii_case(value.trim())),
        MatchKind::Generic => true,
    }
}

fn external_id_eq(facts: &MatchFacts, value: &str) -> bool {
    facts
        .external_id
        .as_deref()
        .is_some_and(|id| id.trim() == value.trim())
}

/// Confidence from the wiki status, downgraded to `Untested` for any engine /
/// generic match (the universal add-on's per-game compatibility is unknown).
fn confidence_for(status: Status, kind: MatchKind) -> MatchConfidence {
    if matches!(kind, MatchKind::Engine | MatchKind::Generic) {
        return MatchConfidence::Untested;
    }
    confidence_for_status(status)
}

fn confidence_for_status(status: Status) -> MatchConfidence {
    match status {
        Status::Working => MatchConfidence::Verified,
        Status::Construction => MatchConfidence::Experimental,
        Status::Unknown => MatchConfidence::Untested,
    }
}

fn channel_rank(channel: Channel) -> u8 {
    match channel {
        Channel::Stable => 0,
        Channel::Beta => 1,
        Channel::Snapshot => 2,
    }
}

fn parse_engine(value: &str) -> Option<Engine> {
    match value.trim().to_ascii_lowercase().as_str() {
        "unreal" => Some(Engine::Unreal),
        "unreal_extended" | "unreal-extended" => Some(Engine::UnrealExtended),
        "unity" => Some(Engine::Unity),
        _ => None,
    }
}

/// Case-insensitive glob match supporting `*` (any run) and `?` (any one char).
fn glob_matches_ci(pattern: &str, file_name: &str) -> bool {
    let pattern: Vec<u8> = pattern.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let name: Vec<u8> = file_name.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let (mut p, mut f) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut star_f = 0usize;

    while f < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[f]) {
            p += 1;
            f += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            star_f = f;
            p += 1;
        } else if let Some(star_p) = star {
            p = star_p + 1;
            star_f += 1;
            f = star_f;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::test_support::{manifest, rule, title};
    use crate::addons::renodx::types::{Category, Generic, MatchKind};

    /// A title carrying a non-default category (external / native-HDR / blacklist).
    fn categorized(id: &str, category: Category) -> Title {
        let mut t = title(
            id,
            id,
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        );
        t.category = category;
        t
    }

    fn facts() -> MatchFacts {
        MatchFacts {
            launcher: Launcher::Steam,
            external_id: Some("1091500".to_owned()),
            exe_file_name: Some("Cyberpunk2077.exe".to_owned()),
            exe_sha256: None,
            engine: None,
            graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64)),
        }
    }

    #[test]
    fn installs_a_verified_steam_match() {
        let m = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        match resolve(&m, &facts()) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(
                    plan.addon_url,
                    source::addon_url("cp2077", Architecture::X64)
                );
                assert_eq!(plan.proxy_dll_name, "dxgi.dll");
                assert_eq!(plan.confidence, MatchConfidence::Verified);
            }
            other => panic!("expected installable, got {other:?}"),
        }
    }

    #[test]
    fn download_url_overrides_slug_derived_url() {
        // A title with a download_url (third-party host) must resolve to that URL,
        // not the clshortfuse URL derived from the slug.
        let mut t = title(
            "ryza2",
            "ryza2",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        );
        t.download_url = Some("https://marat569.github.io/renodx/renodx-ryza2.addon64".to_owned());
        let m = manifest(vec![t]);
        match resolve(&m, &facts()) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(
                    plan.addon_url,
                    "https://marat569.github.io/renodx/renodx-ryza2.addon64"
                );
                // The slug is still used for the on-disk file name.
                assert_eq!(plan.slug, "ryza2");
            }
            other => panic!("expected installable, got {other:?}"),
        }
    }

    #[test]
    fn title_slug_matching_a_generic_uses_the_generics_explicit_url() {
        // A per-game title curated onto a universal engine add-on (matched by
        // `slug`, e.g. a Unity game routed to the `unityengine` generic) must
        // resolve through that generic's explicit host — the clshortfuse URL
        // derived from the same slug may not exist (see `title_addon_url`).
        let t = title(
            "some-curated-unity-title",
            "unityengine",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        );
        let mut m = manifest(vec![t]);
        m.generics.push(Generic {
            engine: crate::addons::renodx::types::Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some(
                "https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64"
                    .to_owned(),
            ),
            url32: None,
            label_key: None,
        });

        match resolve(&m, &facts()) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(
                    plan.addon_url,
                    "https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64"
                );
                assert_eq!(plan.slug, "unityengine");
            }
            other => panic!("expected installable, got {other:?}"),
        }
    }

    #[test]
    fn title_download_url_still_wins_over_a_matching_generic() {
        let mut t = title(
            "curated-unity-game",
            "unityengine",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        );
        t.download_url = Some("https://example.com/renodx-unityengine.addon64".to_owned());
        let mut m = manifest(vec![t]);
        m.generics.push(Generic {
            engine: crate::addons::renodx::types::Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://github.com/NotVoosh/renodx-unity/releases/download/snapshot/renodx-unityengine.addon64".to_owned()),
            url32: None,
            label_key: None,
        });

        match resolve(&m, &facts()) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(
                    plan.addon_url,
                    "https://example.com/renodx-unityengine.addon64"
                );
            }
            other => panic!("expected installable, got {other:?}"),
        }
    }

    #[test]
    fn title_slug_with_no_matching_generic_falls_back_to_clshortfuse() {
        let m = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        match resolve(&m, &facts()) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(
                    plan.addon_url,
                    source::addon_url("cp2077", Architecture::X64)
                );
            }
            other => panic!("expected installable, got {other:?}"),
        }
    }

    #[test]
    fn construction_status_is_experimental() {
        let m = manifest(vec![title(
            "wip",
            "wip",
            Architecture::X64,
            Status::Construction,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        match resolve(&m, &facts()) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(plan.confidence, MatchConfidence::Experimental);
            }
            other => panic!("expected installable, got {other:?}"),
        }
    }

    #[test]
    fn no_match_returns_no_match() {
        let m = manifest(vec![title(
            "other",
            "other",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "42", 100)],
        )]);
        assert!(matches!(resolve(&m, &facts()), RenoDxResolution::NoMatch));
    }

    #[test]
    fn curated_title_installs_despite_inconclusive_detection() {
        // PE-import detection returns empty for games that load Direct3D
        // dynamically; a curated title must still install — the proxy defaults to
        // dxgi and the architecture comes from the title, not from detection.
        let m = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(Vec::new(), None);
        match resolve(&m, &facts) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(plan.proxy_dll_name, "dxgi.dll");
                assert_eq!(plan.arch, Architecture::X64);
            }
            other => panic!("expected installable, got {other:?}"),
        }
    }

    #[test]
    fn confirmed_vulkan_curated_title_installs_via_the_vulkan_layer() {
        // A confirmed Vulkan renderer is now hosted by the shared Vulkan layer, so a
        // curated title installs (host_kind = Vulkan, no proxy DLL) rather than being
        // declined as it was before.
        let m = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
        match resolve(&m, &facts) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(plan.host_kind, HostKind::Vulkan);
                assert!(plan.proxy_dll_name.is_empty());
            }
            other => panic!("expected installable via Vulkan, got {other:?}"),
        }
    }

    #[test]
    fn confirmed_opengl_curated_title_is_declined() {
        // OpenGL has no host RenoDX can drive, so even a curated title is declined.
        let m = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
        assert!(matches!(
            resolve(&m, &facts),
            RenoDxResolution::Incompatible {
                reason: IncompatibilityReason::ApiUnsupported { .. },
            }
        ));
    }

    #[test]
    fn matches_by_exe_name_on_a_non_steam_launcher() {
        // A GOG/Epic/Manual install has no Steam appid; the curated title still
        // resolves through its launcher-agnostic exe_name rule (tier 70).
        let m = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            Status::Working,
            vec![
                rule(MatchKind::SteamAppid, "1091500", 100),
                rule(MatchKind::ExeName, "Cyberpunk2077.exe", 70),
            ],
        )]);
        let mut facts = facts();
        facts.launcher = Launcher::Manual;
        facts.external_id = None;
        facts.exe_file_name = Some("Cyberpunk2077.exe".to_owned());
        match resolve(&m, &facts) {
            RenoDxResolution::Installable(plan) => assert_eq!(plan.slug, "cp2077"),
            other => panic!("expected installable by exe, got {other:?}"),
        }
    }

    #[test]
    fn proxy_comes_from_the_imported_dll_not_a_blind_default() {
        // A D3D9 game must get the d3d9.dll proxy, not the dxgi.dll default.
        let m = manifest(vec![title(
            "g",
            "g",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D9], Some(Architecture::X64))
            .with_graphics_dlls(vec!["d3d9.dll".to_owned()]);
        match resolve(&m, &facts) {
            RenoDxResolution::Installable(plan) => assert_eq!(plan.proxy_dll_name, "d3d9.dll"),
            other => panic!("expected installable, got {other:?}"),
        }
    }

    #[test]
    fn proxy_override_wins_over_detection() {
        let mut t = title(
            "g",
            "g",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        );
        t.proxy_dll_override = Some("dinput8.dll".to_owned());
        let m = manifest(vec![t]);
        match resolve(&m, &facts()) {
            RenoDxResolution::Installable(plan) => assert_eq!(plan.proxy_dll_name, "dinput8.dll"),
            other => panic!("expected installable, got {other:?}"),
        }
    }

    #[test]
    fn engine_generic_fallback_is_untested() {
        let mut m = manifest(vec![title(
            "other",
            "other",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "42", 100)],
        )]);
        m.generics = vec![Generic {
            engine: Engine::Unreal,
            status: Status::Unknown,
            slug: Some("_univ".to_owned()),
            url64: None,
            url32: None,
            label_key: Some("renodx.generic.universal".to_owned()),
        }];
        let mut facts = facts();
        facts.external_id = Some("999".to_owned());
        facts.engine = Some("unreal".to_owned());

        match resolve(&m, &facts) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(plan.confidence, MatchConfidence::Untested);
                assert_eq!(
                    plan.addon_url,
                    source::addon_url("_univ", Architecture::X64)
                );
            }
            other => panic!("expected generic installable, got {other:?}"),
        }
    }

    #[test]
    fn engine_generic_uses_manifest_slug_for_local_identity_with_explicit_url() {
        let mut m = manifest(vec![]);
        m.generics = vec![Generic {
            engine: Engine::Unity,
            status: Status::Working,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://example/renodx-unityengine.addon64".to_owned()),
            url32: Some("https://example/renodx-unityengine.addon32".to_owned()),
            label_key: Some("renodx.generic.unity".to_owned()),
        }];
        let mut facts = facts();
        facts.external_id = Some("999".to_owned());
        facts.engine = Some("unity".to_owned());

        match resolve(&m, &facts) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(plan.confidence, MatchConfidence::Verified);
                assert_eq!(plan.slug, "unityengine");
                assert_eq!(plan.addon_url, "https://example/renodx-unityengine.addon64");
            }
            other => panic!("expected generic installable, got {other:?}"),
        }
    }

    #[test]
    fn engine_generic_installs_on_inconclusive_detection() {
        // A detected engine with no curated title and empty graphics (dynamic
        // Direct3D loading) still gets the engine generic — the engine signal
        // implies a DirectX renderer on Windows. (The Tainted Grail / Unity case.)
        let mut m = manifest(vec![]);
        m.generics = vec![Generic {
            engine: Engine::Unreal,
            status: Status::Unknown,
            slug: Some("_univ".to_owned()),
            url64: None,
            url32: None,
            label_key: Some("renodx.generic.universal".to_owned()),
        }];
        let mut facts = facts();
        facts.engine = Some("unreal".to_owned());
        facts.graphics = ExeGraphicsInfo::new(Vec::new(), Some(Architecture::X64));
        match resolve(&m, &facts) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(plan.proxy_dll_name, "dxgi.dll");
                assert_eq!(plan.confidence, MatchConfidence::Untested);
            }
            other => panic!("expected generic installable, got {other:?}"),
        }
    }

    #[test]
    fn engine_generic_installs_vulkan_and_declines_opengl() {
        // An engine match with a confirmed Vulkan renderer now installs the generic
        // via the shared Vulkan layer; a confirmed OpenGL renderer is still declined.
        let mut m = manifest(vec![]);
        m.generics = vec![Generic {
            engine: Engine::Unreal,
            status: Status::Unknown,
            slug: Some("_univ".to_owned()),
            url64: None,
            url32: None,
            label_key: Some("renodx.generic.universal".to_owned()),
        }];
        let mut facts = facts();
        facts.engine = Some("unreal".to_owned());

        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
        match resolve(&m, &facts) {
            RenoDxResolution::Installable(plan) => {
                assert_eq!(plan.host_kind, HostKind::Vulkan);
                assert_eq!(plan.confidence, MatchConfidence::Untested);
            }
            other => panic!("expected generic Vulkan install, got {other:?}"),
        }

        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
        assert!(matches!(
            resolve(&m, &facts),
            RenoDxResolution::Incompatible {
                reason: IncompatibilityReason::ApiUnsupported { .. },
            }
        ));
    }

    fn external_manifest() -> RenoDxManifest {
        let mut t = title(
            "ext.game",
            "extslug",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        );
        t.category = Category::External {
            url: "https://discord.gg/example".to_owned(),
            label_key: "renodx.external.discord".to_owned(),
        };
        manifest(vec![t])
    }

    #[test]
    fn external_vulkan_title_offers_a_vulkan_file_install() {
        // An external title whose renderer is confirmed Vulkan still shows its link
        // (e.g. RDR2's Discord) AND now offers a file-install hosted by the global
        // Vulkan layer (host_kind = Vulkan).
        let m = external_manifest();
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
        match resolve(&m, &facts) {
            RenoDxResolution::External {
                file_install, url, ..
            } => {
                let fi = file_install.expect("a Vulkan external is file-installable via the layer");
                assert_eq!(fi.host_kind, HostKind::Vulkan);
                assert_eq!(url, "https://discord.gg/example");
            }
            other => panic!("expected external link, got {other:?}"),
        }
        let plan = resolve_external_install(&m, &facts).expect("external vulkan install plan");
        assert_eq!(plan.host_kind, HostKind::Vulkan);
        assert!(plan.proxy_dll_name.is_empty());
    }

    #[test]
    fn external_opengl_title_keeps_link_without_a_file_install() {
        // OpenGL has no host RenoDX can drive: the external add-on stays link-only.
        let m = external_manifest();
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
        match resolve(&m, &facts) {
            RenoDxResolution::External { file_install, .. } => assert!(file_install.is_none()),
            other => panic!("expected external link, got {other:?}"),
        }
        assert!(resolve_external_install(&m, &facts).is_none());
    }

    #[test]
    fn compatible_external_title_offers_file_install() {
        let m = external_manifest();
        match resolve(&m, &facts()) {
            RenoDxResolution::External { file_install, .. } => {
                let fi = file_install.expect("compatible external is file-installable");
                assert_eq!(fi.confidence, MatchConfidence::Verified);
            }
            other => panic!("expected external, got {other:?}"),
        }
        let plan = resolve_external_install(&m, &facts()).expect("external install plan");
        assert_eq!(plan.slug, "extslug");
        assert_eq!(plan.proxy_dll_name, "dxgi.dll");
    }

    #[test]
    fn file_installable_for_directx_inconclusive_and_vulkan_but_not_opengl() {
        let mut facts = facts();
        // A confirmed Direct3D renderer is file-installable (proxy).
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64));
        assert!(file_installable(&facts));
        // An inconclusive read still allows it (defaults to a proxy).
        facts.graphics = ExeGraphicsInfo::new(Vec::new(), None);
        assert!(file_installable(&facts));
        // A confirmed Vulkan renderer is file-installable via the global layer.
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
        assert!(file_installable(&facts));
        // A confirmed OpenGL renderer is not.
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
        assert!(!file_installable(&facts));
    }

    #[test]
    fn generic_file_install_plan_routes_host_and_declines_opengl() {
        let mut facts = facts();
        // Direct3D → a proxy-hosted generic plan.
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X64));
        let plan =
            generic_file_install_plan(&facts, Architecture::X64).expect("directx installable");
        assert_eq!(plan.host_kind, HostKind::Proxy);
        assert!(plan.slug.is_empty(), "a generic plan has no catalogue slug");
        assert_eq!(plan.arch, Architecture::X64);
        assert!(!plan.proxy_dll_name.is_empty());

        // Vulkan → a layer-hosted generic plan (no proxy DLL).
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
        let vk = generic_file_install_plan(&facts, Architecture::X64).expect("vulkan installable");
        assert_eq!(vk.host_kind, HostKind::Vulkan);
        assert!(vk.proxy_dll_name.is_empty());

        // OpenGL → no plan.
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::OpenGl], Some(Architecture::X64));
        assert!(generic_file_install_plan(&facts, Architecture::X64).is_none());
    }

    #[test]
    fn matched_slug_is_the_matching_titles_slug_or_none() {
        let m = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        assert_eq!(matched_slug(&m, &facts()).as_deref(), Some("cp2077"));
        assert_eq!(matched_slug(&manifest(vec![]), &facts()), None);
    }

    #[test]
    fn external_title_is_link_only_on_required_api_mismatch() {
        // An explicit `required_api` the detected (supported) API does not satisfy
        // is the one remaining hard gate: the external add-on stays link-only, with
        // no file install offered. (Inconclusive or merely non-DirectX detection no
        // longer vetoes a curated title.)
        let mut t = title(
            "ext.game",
            "extslug",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        );
        t.category = Category::External {
            url: "https://discord.gg/example".to_owned(),
            label_key: "renodx.external.discord".to_owned(),
        };
        t.compatibility.required_api = vec![GraphicsApi::D3D12];
        let m = manifest(vec![t]);
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X64));

        match resolve(&m, &facts) {
            RenoDxResolution::External { file_install, .. } => assert!(file_install.is_none()),
            other => panic!("expected external, got {other:?}"),
        }
        assert!(resolve_external_install(&m, &facts).is_none());
    }

    #[test]
    fn resolve_external_install_rejects_non_external_title() {
        let m = manifest(vec![title(
            "plain",
            "plain",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        assert!(resolve_external_install(&m, &facts()).is_none());
    }

    #[test]
    fn blacklist_category_yields_unsupported() {
        let m = manifest(vec![categorized(
            "blk",
            Category::Blacklist {
                reason: "renodx.reason.broken".to_owned(),
            },
        )]);
        match resolve(&m, &facts()) {
            RenoDxResolution::Unsupported { reason } => {
                assert_eq!(reason.as_deref(), Some("renodx.reason.broken"));
            }
            other => panic!("expected unsupported, got {other:?}"),
        }
        // A blacklisted game is never offered a file install.
        assert!(resolve_external_install(&m, &facts()).is_none());
    }

    #[test]
    fn native_hdr_category_yields_native_hdr() {
        let m = manifest(vec![categorized("nh", Category::NativeHdr)]);
        assert!(matches!(resolve(&m, &facts()), RenoDxResolution::NativeHdr));
        assert!(resolve_external_install(&m, &facts()).is_none());
    }
}
