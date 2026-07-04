use renderpilot_domain::Architecture;

use crate::addons::matching::{IncompatibilityReason, MatchConfidence};
use crate::addons::reshade::proxy::HostKind;

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
    /// Confidence shown to the user.
    pub confidence: MatchConfidence,
    /// i18n note/requirement keys (a generic install carries its engine label here).
    pub notes_keys: Vec<String>,
}

impl ResolvedInstall {
    /// Projects the user-facing fields of a plan into an [`ExternalInstall`] for a
    /// file-installable external title.
    pub(crate) fn into_external_install(self) -> ExternalInstall {
        ExternalInstall {
            arch: self.arch,
            proxy_dll_name: self.proxy_dll_name,
            confidence: self.confidence,
            notes_keys: self.notes_keys,
            host_kind: self.host_kind,
        }
    }
}

/// What a file-installable external title offers the UI alongside the link, so a
/// user who downloaded the add-on can install it locally. Built only when the game
/// is compatible.
#[derive(Debug, Clone)]
pub struct ExternalInstall {
    /// Architecture of the add-on / executable.
    pub arch: Architecture,
    /// Proxy DLL file name to install ReShade as.
    pub proxy_dll_name: String,
    /// Confidence shown to the user.
    pub confidence: MatchConfidence,
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
