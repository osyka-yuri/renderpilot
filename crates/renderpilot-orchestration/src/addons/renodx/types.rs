//! Data types for the RenoDX manifest.
//!
//! The manifest is an **overrides + catalogue** document, not a content-addressed
//! artifact list: RenoDX add-ons are rolling per-game snapshots fetched live from
//! upstream at install time, so nothing here is hashed or mirrored. It carries:
//!
//! * [`Title`] — a matchable game: ordered tiered match rules, compatibility
//!   constraints, the upstream add-on `slug`, the wiki test-map [`Status`], and
//!   optional per-game overrides.
//! * [`Generic`] — engine fallbacks (Unreal/Unity/…) used when no per-game title
//!   matches; the add-on is derived from a slug or an explicit upstream URL.
//! * [`ReshadeConfig`] — the global add-on-enabled ReShade host sources (shared).
//! * [`Defaults`] — shared title defaults (`min_app_version` / `channel`) hoisted
//!   in schema v3 so the per-title boilerplate is emitted only on deviation; the
//!   parser merges them via `#[serde(default)]` backed by the same values, and
//!   [`super::validate`] asserts the manifest's `defaults` match.
//!
//! The tool-agnostic match vocabulary is shared and re-exported below from
//! [`crate::addons::matching`] (the wire shapes are identical — serde does not
//! encode module paths). Most ReShade-host types are addressed at their
//! canonical [`crate::addons::reshade::types`] path instead; [`ReshadeChannel`]
//! and [`ReshadeChannelParseError`] stay re-exported here too, since
//! `reshade::types` is crate-private and `renderpilot-api` (outside this
//! crate) needs them. This module owns the RenoDX-shaped wire model (manifest,
//! title, generic, category) on top of all of them.

use renderpilot_domain::{Architecture, GraphicsApi};
use serde::{Deserialize, Serialize};

// Shared matching vocabulary re-exported so `super::types::X` keeps resolving
// across the RenoDX subsystem (the wire shapes are identical — serde does not
// encode module paths).
pub use crate::addons::matching::{Channel, MatchKind, MatchRule, Status};
// `reshade::types` is `pub(crate)`; these two are RenoDX's public wire contract
// (`renderpilot-api` reads a recorded channel / parses one from a legacy URL).
pub use crate::addons::reshade::types::{ReshadeChannel, ReshadeChannelParseError};
use crate::addons::reshade::types::{ReshadeConfig, ReshadeIniTweaks};

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
    /// Default minimum app version a title requires.
    pub min_app_version: String,
    /// Default release channel.
    pub channel: Channel,
}

/// Default minimum app version a title requires when it omits its own.
const DEFAULT_MIN_APP_VERSION: &str = "1.0.0";

/// `#[serde(default)]` backing for [`Title::min_app_version`].
fn default_min_app_version() -> String {
    DEFAULT_MIN_APP_VERSION.to_owned()
}

/// The default `ReShade.ini` tweaks a RenoDX install requests before
/// folder-specific filtering. `AddonPath` is left unset — ReShade already defaults
/// its add-on search path to the ReShade DLL folder (the game folder), where the
/// RenoDX add-on is placed, so an explicit `AddonPath=.` would be redundant.
#[must_use]
pub(crate) fn renodx_ini_defaults() -> ReshadeIniTweaks {
    ReshadeIniTweaks {
        disabled_addons: vec!["Generic Depth".to_owned(), "Effect Runtime Sync".to_owned()],
        addon_path: None,
        dlss_fix: None,
    }
}

/// The manifest's shared title defaults, as built by the generator (schema v3).
/// Used by [`super::validate`] to assert `manifest.defaults` matches the Rust-side
/// `#[serde(default)]` values, and by test fixtures.
#[must_use]
pub fn manifest_defaults() -> Defaults {
    Defaults {
        min_app_version: DEFAULT_MIN_APP_VERSION.to_owned(),
        channel: Channel::default(),
    }
}

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

pub use crate::addons::matching::Engine;

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

/// A matchable game with its slug, status, compatibility, and overrides.
///
/// In schema v3 `min_app_version` and `channel` default from the manifest's
/// top-level [`Defaults`] when a title omits them — the `#[serde(default)]`
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
    /// Optional provenance of a non-empty `conflicts` list (a URL or note).
    #[serde(default)]
    pub source: Option<String>,
}
