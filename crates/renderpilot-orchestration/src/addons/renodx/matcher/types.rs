use renderpilot_domain::Architecture;

use crate::addons::CatalogMessage;
use crate::addons::matching::{IncompatibilityReason, MatchConfidence};
use crate::addons::renodx::types::RenoDxGenericProfile;
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
    /// Present when this plan came from an engine-level generic profile rather
    /// than a dedicated per-game title. The UI uses this for the generic badge.
    pub generic_profile: Option<RenoDxGenericProfile>,
}

impl ResolvedInstall {
    /// Projects the user-facing fields of a plan into an [`ExternalInstall`] for a
    /// file-installable external title.
    pub(crate) fn into_external_install(self) -> ExternalInstall {
        ExternalInstall {
            arch: self.arch,
            proxy_dll_name: self.proxy_dll_name,
            confidence: self.confidence,
            host_kind: self.host_kind,
            generic_profile: self.generic_profile,
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
    /// How RenoDX would hook into this game (proxy DLL or the shared Vulkan layer),
    /// so the file-install path drives the right one.
    pub host_kind: HostKind,
    /// Engine generic this external file-install path came from, when applicable.
    pub generic_profile: Option<RenoDxGenericProfile>,
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
        /// Localizable link label supplied by the catalogue.
        message: CatalogMessage,
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
    Blacklisted {
        /// Localizable explanation supplied by the catalogue.
        message: CatalogMessage,
    },
    /// Nothing matched the game.
    NoMatch,
}
