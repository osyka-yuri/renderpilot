//! Data types for the RenoDX manifest.
//!
//! The manifest is an **overrides + catalogue** document, not a content-addressed
//! artifact list: RenoDX add-ons are rolling per-game snapshots fetched live from
//! upstream at install time, so nothing here is hashed or mirrored. It carries:
//!
//! * [`Title`] — a matchable game: ordered tiered match rules, a structured risk
//!   assessment, compatibility constraints, the upstream add-on `slug`, the wiki
//!   test-map [`Status`], and optional per-game overrides.
//! * [`Generic`] — engine fallbacks (Unreal/Unity/…) used when no per-game title
//!   matches; the add-on is derived from a slug or an explicit upstream URL.
//! * [`ReshadeConfig`] — the global add-on-enabled ReShade host sources
//!   (reshade.me stable scrape + the nightly.link build).
//! * [`Defaults`] — shared title defaults (`risk` / `min_app_version` / `channel`)
//!   hoisted in schema v3 so the per-title boilerplate is emitted only on
//!   deviation; the parser merges them via `#[serde(default)]` backed by the same
//!   values, and [`super::validate`] asserts the manifest's `defaults` match.
//! * `external` / `native_hdr` / `blacklist` — games that are not a standard
//!   install (off-GitHub, native HDR, or unsupported).
//!
//! Source resolution and the resulting [`super::matcher::RenoDxResolution`] live in
//! [`super::source`] and [`super::matcher`]; this module is just the wire model.

use std::fmt;
use std::str::FromStr;

use renderpilot_domain::{Architecture, GraphicsApi};
use serde::{Deserialize, Serialize};

/// Top-level RenoDX manifest document.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenoDxManifest {
    /// Schema version used to interpret this document.
    pub schema_version: u32,
    /// RFC 3339 timestamp recording when the manifest was generated.
    pub generated_at: String,
    /// Global add-on-enabled ReShade host sources.
    pub reshade: ReshadeConfig,
    /// Engine-detected fallbacks, tried when no per-game title matches.
    pub generics: Vec<Generic>,
    /// Shared title defaults (schema v3); the parser merges these onto every
    /// title that omits the corresponding field.
    pub defaults: Defaults,
    /// The single catalogue of every game RenoDX knows. Each entry is matched by
    /// its rules and routed by its [`Title::category`] (installable / external /
    /// native-HDR / blacklist) — there are no separate per-category collections.
    pub titles: Vec<Title>,
}

/// Shared title defaults hoisted in schema v3. The manifest carries these once at
/// the top level; each [`Title`] only repeats a field when it deviates. The parser
/// applies the same values via `#[serde(default)]`, and [`super::validate`] asserts
/// the manifest's `defaults` agree with those Rust defaults (a contract that keeps
/// the two from silently drifting).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Defaults {
    /// Default risk assessment applied to every title that omits its own.
    pub risk: Risk,
    /// Default minimum app version a title requires.
    pub min_app_version: String,
    /// Default release channel.
    pub channel: Channel,
}

/// Upstream wiki provenance recorded on the default risk assessment.
const DEFAULT_RISK_SOURCE: &str = "https://github.com/clshortfuse/renodx/wiki/Mods";

/// i18n key for the default single-player-safe risk message.
const DEFAULT_RISK_MESSAGE_KEY: &str = "renodx.risk.sp_safe";

/// Default minimum app version a title requires when it omits its own.
const DEFAULT_MIN_APP_VERSION: &str = "1.0.0";

/// `#[serde(default)]` backing for [`Title::min_app_version`].
fn default_min_app_version() -> String {
    DEFAULT_MIN_APP_VERSION.to_owned()
}

impl Default for Risk {
    /// Manifest risk defaults (schema v3): a single-player game with no anti-cheat
    /// and an informational severity. Kept in sync with the generator's `DEFAULT_RISK`
    /// and asserted by [`super::validate::validate_defaults`].
    fn default() -> Self {
        Self {
            anticheat_engine: AnticheatEngine::None,
            online: OnlineKind::Singleplayer,
            severity: RiskSeverity::Info,
            message_key: DEFAULT_RISK_MESSAGE_KEY.to_owned(),
            confidence: AssessmentConfidence::Medium,
            source: Some(DEFAULT_RISK_SOURCE.to_owned()),
        }
    }
}

/// The manifest's shared title defaults, as built by the generator (schema v3).
/// Used by [`super::validate`] to assert `manifest.defaults` matches the Rust-side
/// `#[serde(default)]` values, and by test fixtures.
#[must_use]
pub fn manifest_defaults() -> Defaults {
    Defaults {
        risk: Risk::default(),
        min_app_version: DEFAULT_MIN_APP_VERSION.to_owned(),
        channel: Channel::default(),
    }
}

// ---------------------------------------------------------------------------
// ReShade host sources
// ---------------------------------------------------------------------------

/// Global add-on-enabled ReShade host configuration.
///
/// The host can be the manifest-current stable reshade.me add-on installer or the
/// crosire CI build proxied by nightly.link.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReshadeConfig {
    /// Manifest-current stable ReShade add-on installer. This is a versioned
    /// reshade.me URL, not a latest alias; new stable builds become visible only
    /// when the manifest refreshes this URL.
    #[serde(default)]
    pub stable: Option<ReshadeStable>,
    /// Nightly ReShade build (a plain zip per architecture).
    pub nightly: ReshadeNightly,
}

impl ReshadeConfig {
    /// Whether the manifest can provide a source for `channel`.
    #[must_use]
    pub fn supports_channel(&self, channel: ReshadeChannel) -> bool {
        match channel {
            ReshadeChannel::Stable => self.stable.is_some(),
            ReshadeChannel::Nightly => true,
        }
    }

    /// The effective channel used only by install paths. Stable is the default,
    /// but old manifests without a stable URL gracefully fall back to nightly.
    #[must_use]
    pub fn effective_install_channel(&self, requested: ReshadeChannel) -> ReshadeChannel {
        if self.supports_channel(requested) {
            requested
        } else {
            ReshadeChannel::Nightly
        }
    }
}

/// Manifest-current stable ReShade add-on installer URL.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReshadeStable {
    /// Versioned `_Addon.exe` URL from reshade.me.
    pub url: String,
}

/// Nightly ReShade build URLs (zip artifacts containing `ReShade{64,32}.dll`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReshadeNightly {
    /// 64-bit nightly artifact URL.
    pub url64: String,
    /// 32-bit nightly artifact URL.
    pub url32: String,
}

/// ReShade host source channel. Serialized in snake_case for API/record
/// provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReshadeChannel {
    /// Manifest-current stable ReShade add-on installer.
    #[default]
    Stable,
    /// Nightly CI artifact from nightly.link.
    Nightly,
}

impl ReshadeChannel {
    /// Stable wire representation used in records and UI payloads.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
        }
    }

    /// Parses a channel stored in legacy/advisory metadata. Unknown or missing values are
    /// recoverable: callers fall back to their default channel policy.
    #[must_use]
    pub fn parse_recorded(value: Option<&str>) -> RecordedChannelParse {
        let Some(val) = value else {
            return RecordedChannelParse::MissingDefaulted;
        };
        match val.parse() {
            Ok(channel) => RecordedChannelParse::Parsed(channel),
            Err(error) => {
                log::warn!("{error}; falling back to default ReShade channel");
                RecordedChannelParse::InvalidDefaulted {
                    raw: val.to_owned(),
                }
            }
        }
    }
}

/// Result of parsing a channel from legacy/advisory metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedChannelParse {
    /// Successfully parsed into a known channel.
    Parsed(ReshadeChannel),
    /// The record had no channel (legacy). Defaulted.
    MissingDefaulted,
    /// The record had an unknown channel string. Defaulted.
    InvalidDefaulted {
        /// The raw string that failed to parse.
        raw: String,
    },
}

impl RecordedChannelParse {
    /// Returns the parsed channel if valid, or `None` if it was missing/invalid.
    #[must_use]
    pub fn into_parsed(self) -> Option<ReshadeChannel> {
        match self {
            Self::Parsed(c) => Some(c),
            Self::MissingDefaulted | Self::InvalidDefaulted { .. } => None,
        }
    }
}

impl FromStr for ReshadeChannel {
    type Err = ReshadeChannelParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "stable" => Ok(Self::Stable),
            "nightly" => Ok(Self::Nightly),
            _ => Err(ReshadeChannelParseError {
                value: value.to_owned(),
            }),
        }
    }
}

/// Error returned when a user/API supplied ReShade channel is not recognized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReshadeChannelParseError {
    value: String,
}

impl fmt::Display for ReshadeChannelParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid ReShade channel: {}", self.value)
    }
}

impl std::error::Error for ReshadeChannelParseError {}

// ---------------------------------------------------------------------------
// Engine generics
// ---------------------------------------------------------------------------

/// An engine-generic add-on, resolved when a game matches by engine rather than id.
///
/// `slug` is the canonical local add-on identity (`renodx-<slug>.addon*`).
/// Optional explicit `url64`/`url32` override the download host for generics
/// published outside clshortfuse.github.io (e.g. the Unity generic).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Generic {
    /// Engine this generic targets.
    pub engine: Engine,
    /// Compatibility status for this generic add-on. Defaults to unknown for
    /// manifests generated before generic confidence existed.
    #[serde(default)]
    pub status: Status,
    /// Canonical add-on slug (e.g. `_univ`, `unityengine`) used for the local file name.
    #[serde(default)]
    pub slug: Option<String>,
    /// Explicit 64-bit add-on URL, when hosted off the default host.
    #[serde(default)]
    pub url64: Option<String>,
    /// Explicit 32-bit add-on URL, when hosted off the default host.
    #[serde(default)]
    pub url32: Option<String>,
    /// i18n key labelling the generic in the UI.
    #[serde(default)]
    pub label_key: Option<String>,
}

/// Engine a [`Generic`] fallback targets (and that [`super::facts`] detects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    /// Unreal Engine.
    Unreal,
    /// Unreal Engine with the extended (UE-Extended) treatment; curated, not auto-detected.
    UnrealExtended,
    /// Unity.
    Unity,
}

impl Engine {
    /// Stable manifest/local-identity string for this engine.
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Unreal => "unreal",
            Self::UnrealExtended => "unreal_extended",
            Self::Unity => "unity",
        }
    }
}

// ---------------------------------------------------------------------------
// External (off-GitHub) sources
// ---------------------------------------------------------------------------

/// How a matched [`Title`] is routed once its rules match an installed game.
///
/// The default, [`Category::Installable`], drives a normal RenoDX install; the
/// other variants categorize the game instead (off-GitHub link, native HDR, or a
/// known-broken blacklist) and carry the per-category payload the outcome needs.
/// Internally tagged by `kind` so the common installable case omits the field
/// entirely in the manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Category {
    /// A standard RenoDX install (the common case; omitted from the manifest).
    #[default]
    Installable,
    /// The add-on is distributed off-GitHub (Discord/Nexus): the UI links out and
    /// offers a manual file install.
    External {
        /// HTTPS link the UI opens (a Discord invite, Nexus page, …).
        url: String,
        /// i18n key for the link label.
        label_key: String,
    },
    /// The game already has native HDR; RenoDX is not offered.
    NativeHdr,
    /// RenoDX is known-broken / unsupported for this game.
    Blacklist {
        /// i18n key explaining why.
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// Title layer
// ---------------------------------------------------------------------------

/// A matchable game with its slug, status, risk, compatibility, and overrides.
///
/// In schema v3 `risk`, `min_app_version`, and `channel` default from the
/// manifest's top-level [`Defaults`] when a title omits them — the `#[serde(default)]`
/// attributes below are backed by the same values, and [`super::validate`] asserts
/// the manifest's `defaults` agree (so a drift is caught at load time rather than
/// silently changing install behaviour).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Title {
    /// Stable identifier of this title.
    pub id: String,
    /// Display name.
    pub name: String,
    /// How this game is routed once matched (install / external link / native-HDR /
    /// blacklist). Defaults to [`Category::Installable`], so the common case omits it.
    #[serde(default)]
    pub category: Category,
    /// Upstream add-on slug (= renodx `src/games` folder = `renodx-<slug>.addon*`).
    pub slug: String,
    /// CPU architecture the add-on targets.
    pub arch: Architecture,
    /// Wiki test-map status: `working` (verified), `construction` (WIP), `unknown`.
    pub status: Status,
    /// Release channel this title belongs to.
    #[serde(default)]
    pub channel: Channel,
    /// Minimum app version required to install this title.
    #[serde(default = "default_min_app_version")]
    pub min_app_version: String,
    /// Ordered match rules; resolution prefers the highest [`MatchRule::tier`].
    #[serde(rename = "match")]
    pub match_rules: Vec<MatchRule>,
    /// Constraints that must hold for the install to be offered.
    #[serde(default)]
    pub compatibility: Compatibility,
    /// Structured ban/stability risk assessment.
    #[serde(default)]
    pub risk: Risk,
    /// Overrides the proxy DLL name otherwise derived from the game's API.
    #[serde(default)]
    pub proxy_dll_override: Option<String>,
    /// i18n message keys for post-install notes / requirements shown to the user.
    #[serde(default)]
    pub notes_keys: Vec<String>,
    /// Direct download URL for an add-on hosted off the clshortfuse snapshot
    /// (a third-party github.io / GitHub release). When present the installer
    /// fetches this instead of deriving the URL from `slug`. Absent ⇒ the
    /// snapshot/github.io URL derived from `slug` is used.
    #[serde(default)]
    pub download_url: Option<String>,
}

/// Upstream wiki test-map status of a [`Title`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Verified working.
    Working,
    /// Work in progress / experimental.
    Construction,
    /// Untested.
    #[default]
    Unknown,
}

/// Release channel of a [`Title`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Verified, recommended for everyone.
    #[default]
    Stable,
    /// Tested but not yet promoted.
    Beta,
    /// Bleeding-edge upstream snapshot.
    Snapshot,
}

/// A single rule used to match an installed game to a [`Title`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchRule {
    /// What the rule matches against.
    pub kind: MatchKind,
    /// The value to match (a Steam AppID, exe-name glob, engine id, …).
    #[serde(default)]
    pub value: String,
    /// Specificity tier; higher wins. Conventionally: id 100, fingerprint 90,
    /// exe-name 70, engine 40, generic 10.
    pub tier: u32,
}

/// Dimension a [`MatchRule`] matches against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// Steam application id.
    SteamAppid,
    /// Epic Games catalog id.
    EpicId,
    /// GOG product id.
    GogId,
    /// SHA-256 fingerprint of the game executable.
    ExeSha256,
    /// Case-insensitive glob over the executable file name.
    ExeName,
    /// Detected engine (for example `unreal`, `unity`).
    Engine,
    /// Lowest-priority catch-all fallback.
    Generic,
}

/// Constraints that gate whether a [`Title`] can be installed.
///
/// `required_arch` is never carried — it always equals the title's `arch`, so the
/// resolver derives it. Only `required_api` and `conflicts` are modelled, and only
/// emitted by the generator when non-empty.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Compatibility {
    /// Graphics APIs this title supports; empty means "no API constraint".
    #[serde(default)]
    pub required_api: Vec<GraphicsApi>,
    /// Known-conflicting mod ids (e.g. `special_k`); empty means "no known conflicts".
    #[serde(default)]
    pub conflicts: Vec<String>,
}

/// Structured ban- and stability-risk assessment for a [`Title`].
///
/// Implements [`Default`] with the manifest's shared risk defaults (schema v3):
/// a single-player game with no anti-cheat and an informational severity. The
/// generator only emits a title's `risk` when it differs from these values, and
/// [`super::validate`] asserts the manifest's `defaults.risk` equals this default.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Risk {
    /// Anti-cheat engine known to be present.
    pub anticheat_engine: AnticheatEngine,
    /// Online/multiplayer classification.
    pub online: OnlineKind,
    /// How the installer should treat the risk.
    pub severity: RiskSeverity,
    /// i18n message key describing the risk to the user.
    pub message_key: String,
    /// Confidence in this assessment.
    #[serde(default)]
    pub confidence: AssessmentConfidence,
    /// Optional provenance of the assessment (a URL or note).
    #[serde(default)]
    pub source: Option<String>,
}

/// Anti-cheat engine classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnticheatEngine {
    /// Easy Anti-Cheat.
    Eac,
    /// BattlEye.
    #[serde(rename = "battleye")]
    BattlEye,
    /// No anti-cheat present.
    None,
    /// Presence not determined.
    #[default]
    Unknown,
}

/// Online/multiplayer classification of a title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnlineKind {
    /// Single-player only.
    Singleplayer,
    /// Cooperative multiplayer.
    Coop,
    /// Competitive multiplayer.
    Pvp,
    /// Not determined.
    #[default]
    Unknown,
}

/// How the installer should act on a title's risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    /// Safe; informational only.
    Info,
    /// Risky; require explicit user confirmation before installing.
    Warn,
    /// Do not install.
    Block,
}

/// Confidence in a [`Risk`] assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentConfidence {
    /// High confidence.
    High,
    /// Medium confidence.
    #[default]
    Medium,
    /// Low confidence.
    Low,
}

/// `reshade.ini` adjustments requested for a RenoDX add-on to behave correctly.
///
/// The install flow starts from [`ReshadeIniTweaks::renodx_defaults`] rather than
/// carrying these values in the manifest, then filters the default disabled-addons
/// list when the target folder already has user ReShade effects/presets. The
/// optional [`DlssFixIniTweaks`] is populated only when a DLSS-Fix companion add-on
/// is installed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReshadeIniTweaks {
    /// Bundled ReShade add-ons to disable.
    pub disabled_addons: Vec<String>,
    /// Add-on search path to set, overriding ReShade's default (the ReShade DLL
    /// folder, i.e. the game folder). `None` leaves the default untouched —
    /// RenoDX places its add-on next to the proxy DLL, so the default search
    /// path already finds it and an explicit `AddonPath=.` would be redundant.
    pub addon_path: Option<String>,
    /// DLSS-Fix INI configuration, present only when the DLSS-Fix add-on is
    /// installed alongside the main add-on.
    pub dlss_fix: Option<DlssFixIniTweaks>,
}

/// INI keys the DLSS-Fix companion add-on needs under `[RENODX-DLSSFIX]`, plus the
/// `LoadFromDllMain` entry it adds to `[ADDON]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlssFixIniTweaks {
    /// DLSS-Fix add-on file name placed in the game folder (e.g.
    /// `renodx-dlssfix.addon64`), used as the `LoadFromDllMain` value in `[ADDON]`.
    pub addon_file_name: String,
    /// Windows-native (backslash) path to `nvngx_dlss.dll`.
    pub dlss_path: String,
    /// Windows-native (backslash) path to `sl.interposer.dll`.
    pub streamline_path: String,
}

impl ReshadeIniTweaks {
    /// The default tweaks a RenoDX install requests before folder-specific
    /// filtering. `AddonPath` is left unset — ReShade already defaults its add-on
    /// search path to the ReShade DLL folder (the game folder), where the RenoDX
    /// add-on is placed, so an explicit `AddonPath=.` would be redundant.
    #[must_use]
    pub fn renodx_defaults() -> Self {
        Self {
            disabled_addons: vec!["Generic Depth".to_owned(), "Effect Runtime Sync".to_owned()],
            addon_path: None,
            dlss_fix: None,
        }
    }
}
