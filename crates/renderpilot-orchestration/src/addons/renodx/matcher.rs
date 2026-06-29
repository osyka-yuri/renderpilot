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

use super::policy::{api_supports_renodx, is_non_directx_renderer, primary_api, proxy_dll};
use super::source;
use super::types::{
    AnticheatEngine, AssessmentConfidence, Category, Channel, Engine, MatchKind, MatchRule,
    OnlineKind, RenoDxManifest, Risk, RiskSeverity, Status, Title,
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
    /// Proxy DLL file name to install ReShade as.
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
        Some((title, rule)) => resolve_title(title, rule, facts),
        None => resolve_generic(manifest, facts),
    }
}

/// Routes a matched title by its [`Category`]: a categorized game (external link /
/// native HDR / blacklist) takes its branch; an installable game is gated by its
/// compatibility constraints.
fn resolve_title(title: &Title, rule: &MatchRule, facts: &MatchFacts) -> RenoDxResolution {
    match &title.category {
        Category::Blacklist { reason } => RenoDxResolution::Unsupported {
            reason: Some(reason.clone()),
        },
        Category::NativeHdr => RenoDxResolution::NativeHdr,
        Category::External { url, label_key } => {
            // A compatible external game can be installed from a user-downloaded file.
            let file_install = build_install_plan(title, rule, facts)
                .ok()
                .map(|plan| Box::new(plan.into_external_install()));
            RenoDxResolution::External {
                url: url.clone(),
                label_key: label_key.clone(),
                file_install,
            }
        }
        Category::Installable => match build_install_plan(title, rule, facts) {
            Ok(plan) => RenoDxResolution::Installable(Box::new(plan)),
            Err(reason) => RenoDxResolution::Incompatible { reason },
        },
    }
}

/// Builds the full install plan for a matched, compatible title, or the reason it
/// is incompatible. Shared by the standard install path and the external
/// file-install path.
fn build_install_plan(
    title: &Title,
    rule: &MatchRule,
    facts: &MatchFacts,
) -> Result<ResolvedInstall, IncompatibilityReason> {
    let proxy_dll_name = check_title_compatibility(title, facts)?;
    Ok(ResolvedInstall {
        slug: title.slug.clone(),
        // Prefer an explicit per-title download URL (third-party host) over the
        // clshortfuse URL derived from the slug.
        addon_url: title
            .download_url
            .clone()
            .unwrap_or_else(|| source::addon_url(&title.slug, title.arch)),
        arch: title.arch,
        proxy_dll_name,
        risk: title.risk.clone(),
        confidence: confidence_for(title.status, rule.kind),
        notes_keys: title.notes_keys.clone(),
    })
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
    build_install_plan(title, rule, facts).ok()
}

/// Whether RenoDX can be installed for this game from a user-supplied add-on file.
/// The renderer must be DirectX or inconclusive — a confirmed Vulkan/OpenGL renderer
/// cannot load a dxgi-style proxy. Backs the manual-install escape hatch for games
/// with no automatic or curated-external path.
#[must_use]
pub fn file_installable(facts: &MatchFacts) -> bool {
    !is_non_directx_renderer(primary_api(&facts.graphics))
}

/// The catalogue add-on slug for this game, if a title matches — so a manual install
/// can show the expected file name (`renodx-<slug>.addon*`) for a soft check.
/// `None` for an unrecognized game.
#[must_use]
pub fn matched_slug(manifest: &RenoDxManifest, facts: &MatchFacts) -> Option<String> {
    select_title(manifest, facts).map(|(title, _)| title.slug.clone())
}

/// A generic install plan for a manual file install when no catalogue title matched:
/// the host is the proxy DLL the game actually loads, `arch` is what the user's
/// add-on targets, and risk is the conservative generic assessment. `None` when the
/// renderer is confirmed non-DirectX (no proxy can load).
#[must_use]
pub fn generic_file_install_plan(
    facts: &MatchFacts,
    arch: Architecture,
) -> Option<ResolvedInstall> {
    file_installable(facts).then(|| ResolvedInstall {
        slug: String::new(),
        addon_url: String::new(),
        arch,
        proxy_dll_name: resolve_proxy_dll(None, &facts.graphics),
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
    // A confirmed non-DirectX renderer rules RenoDX out even with an engine
    // match; an inconclusive (`Unknown`) detection does not — the engine signal
    // (e.g. `UnityPlayer.dll`) already implies a DirectX renderer on Windows.
    if is_non_directx_renderer(api) {
        return RenoDxResolution::Incompatible {
            reason: IncompatibilityReason::ApiUnsupported { detected: api },
        };
    }
    let Some(arch) = facts.graphics.architecture() else {
        return RenoDxResolution::Incompatible {
            reason: IncompatibilityReason::ArchUnknown,
        };
    };
    let proxy_dll_name = resolve_proxy_dll(None, &facts.graphics);
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
        proxy_dll_name,
        risk: generic_risk(),
        confidence: confidence_for_status(generic.status),
        // The generic's engine label is surfaced as a note so the card can flag
        // "this is a universal, not per-game, add-on".
        notes_keys: generic.label_key.clone().into_iter().collect(),
    }))
}

/// Default ReShade proxy DLL when the detected API is inconclusive: `dxgi.dll`
/// loads for D3D10/11/12, which covers essentially every modern RenoDX title.
const DEFAULT_PROXY_DLL: &str = "dxgi.dll";

/// The proxy DLL ReShade installs as: an explicit per-title override, else the
/// DLL the game actually imports (via [`proxy_dll`], in ReShade preference), else
/// the [`DEFAULT_PROXY_DLL`] — so the install hooks the DLL the game really loads
/// and an inconclusive read still resolves to a sane modern default.
fn resolve_proxy_dll(override_name: Option<&str>, graphics: &ExeGraphicsInfo) -> String {
    override_name
        .map(str::to_owned)
        .or_else(|| proxy_dll(graphics))
        .unwrap_or_else(|| DEFAULT_PROXY_DLL.to_owned())
}

/// Validates a matched curated title and returns its proxy DLL name.
///
/// A curated title is RenoDX-supported by definition (it is in the catalogue) and
/// carries its own architecture, so unreliable runtime API/architecture detection
/// — which comes back empty whenever a game loads Direct3D dynamically — must
/// never veto it. Detection is advisory here: it only refines the proxy DLL.
///
/// The two hard gates are physical: a **confirmed** non-DirectX renderer
/// (Vulkan/OpenGL) cannot load a dxgi-style proxy, so the title is declined even
/// though it is curated (an `Unknown`/inconclusive read is still trusted); and an
/// explicit `required_api`, enforced only when detection identified a supported API
/// to check against. External/native-HDR titles route by category before reaching
/// here, so e.g. RDR2-Vulkan stays a Discord link rather than a broken proxy install.
fn check_title_compatibility(
    title: &Title,
    facts: &MatchFacts,
) -> Result<String, IncompatibilityReason> {
    let detected = primary_api(&facts.graphics);
    if is_non_directx_renderer(detected) {
        return Err(IncompatibilityReason::ApiUnsupported { detected });
    }
    if api_supports_renodx(detected)
        && !title.compatibility.required_api.is_empty()
        && !title.compatibility.required_api.contains(&detected)
    {
        return Err(IncompatibilityReason::ApiNotAllowed {
            detected,
            required: title.compatibility.required_api.clone(),
        });
    }
    Ok(resolve_proxy_dll(
        title.proxy_dll_override.as_deref(),
        &facts.graphics,
    ))
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

/// i18n key for the generic-install risk message (engine fallback matches).
const RISK_GENERIC_KEY: &str = "renodx.risk.generic";

/// A generic match has no curated risk; treat it as single-player safe (the user
/// still sees the `Untested` confidence).
pub(super) fn generic_risk() -> Risk {
    Risk {
        anticheat_engine: AnticheatEngine::None,
        online: OnlineKind::Singleplayer,
        severity: RiskSeverity::Info,
        message_key: RISK_GENERIC_KEY.to_owned(),
        confidence: AssessmentConfidence::Low,
        source: None,
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
    fn confirmed_non_directx_renderer_declines_even_a_curated_title() {
        // A curated title is trusted over an inconclusive read, but a *confirmed*
        // Vulkan/OpenGL renderer physically cannot load a dxgi-style proxy, so it is
        // declined rather than given a non-loading install. (The unified resolver
        // picks the real renderer, so a Vulkan reading is the game, not a stub.)
        let m = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            Status::Working,
            vec![rule(MatchKind::SteamAppid, "1091500", 100)],
        )]);
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
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
    fn engine_generic_declines_confirmed_non_directx() {
        // Detection that positively identifies a non-DirectX renderer still rules
        // RenoDX out, even with an engine match.
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
    fn external_vulkan_title_keeps_link_without_a_broken_file_install() {
        // An external title whose renderer is confirmed Vulkan still shows its link
        // (e.g. RDR2's Discord), but offers no file-install — a dxgi proxy can't load.
        let m = external_manifest();
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
        match resolve(&m, &facts) {
            RenoDxResolution::External {
                file_install, url, ..
            } => {
                assert!(
                    file_install.is_none(),
                    "no proxy file-install for a Vulkan renderer"
                );
                assert_eq!(url, "https://discord.gg/example");
            }
            other => panic!("expected external link, got {other:?}"),
        }
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
    fn file_installable_for_directx_and_inconclusive_but_not_vulkan() {
        let mut facts = facts();
        // A confirmed Direct3D renderer is file-installable.
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D12], Some(Architecture::X64));
        assert!(file_installable(&facts));
        // An inconclusive read still allows it (the renderer is unknown, not Vulkan).
        facts.graphics = ExeGraphicsInfo::new(Vec::new(), None);
        assert!(file_installable(&facts));
        // A confirmed Vulkan renderer cannot load a proxy.
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
        assert!(!file_installable(&facts));
    }

    #[test]
    fn generic_file_install_plan_for_directx_none_for_vulkan() {
        let mut facts = facts();
        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X64));
        let plan =
            generic_file_install_plan(&facts, Architecture::X64).expect("directx installable");
        assert!(plan.slug.is_empty(), "a generic plan has no catalogue slug");
        assert_eq!(plan.arch, Architecture::X64);
        assert!(!plan.proxy_dll_name.is_empty());

        facts.graphics = ExeGraphicsInfo::new(vec![GraphicsApi::Vulkan], Some(Architecture::X64));
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
