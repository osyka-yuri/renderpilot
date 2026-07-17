//! Shared ReShade host source configuration and channel vocabulary.
//!
//! These types describe the add-on-enabled ReShade host every tool installs (the
//! stable reshade.me installer and/or the nightly.link CI build) and the channel
//! provenance recorded on an install. They are tool-agnostic: RenoDX offers both
//! channels; Luma ships nightly-only (its manifest simply carries no `stable`), but
//! the model is the same.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Global add-on-enabled ReShade host configuration.
///
/// The host can be the manifest-current stable reshade.me add-on installer or the
/// crosire CI build proxied by nightly.link.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReshadeSourceCatalog {
    /// Manifest-current stable ReShade add-on installer. This is a versioned
    /// reshade.me URL, not a latest alias; new stable builds become visible only
    /// when the manifest refreshes this URL. `None` when the tool ships
    /// nightly-only (e.g. Luma).
    pub stable: Option<ReshadeStable>,
    /// Nightly ReShade build (a plain zip per architecture).
    pub nightly: ReshadeNightly,
}

impl ReshadeSourceCatalog {
    /// Whether the manifest can provide a source for `channel`.
    #[must_use]
    pub fn supports_channel(&self, channel: ReshadeChannel) -> bool {
        match channel {
            ReshadeChannel::Stable => self.stable.is_some(),
            ReshadeChannel::Nightly => true,
        }
    }

    /// Default channel for a new selection. Explicit selections are never
    /// remapped: callers must reject an unavailable requested channel.
    #[must_use]
    pub fn default_install_channel(&self) -> ReshadeChannel {
        if self.stable.is_some() {
            ReshadeChannel::Stable
        } else {
            ReshadeChannel::Nightly
        }
    }
}

impl ReshadeSourceCatalog {
    /// Legacy selection helper retained while RenoDX command callers migrate
    /// from embedded to shared source catalogues.
    #[must_use]
    pub fn effective_install_channel(&self, requested: ReshadeChannel) -> ReshadeChannel {
        if self.supports_channel(requested) {
            requested
        } else {
            ReshadeChannel::Nightly
        }
    }
}

/// Transitional name for callers still carrying embedded ReShade sources.
pub type ReshadeConfig = ReshadeSourceCatalog;

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

/// `reshade.ini` adjustments requested for an add-on to behave correctly.
///
/// A tool's install flow builds these (RenoDX from its `renodx_defaults`, filtered
/// by folder contents; Luma leaves them all empty so no ini op is emitted). The
/// optional [`DlssFixIniTweaks`] is populated only for RenoDX's DLSS-Fix companion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReshadeIniTweaks {
    /// Bundled ReShade add-ons to disable.
    pub disabled_addons: Vec<String>,
    /// Add-on search path to set, overriding ReShade's default (the ReShade DLL
    /// folder, i.e. the game folder). `None` leaves the default untouched — an
    /// add-on placed next to the proxy DLL is already found by the default search
    /// path, so an explicit `AddonPath=.` would be redundant.
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
