/// Availability data transfer objects.
use renderpilot_domain::{AddonKind, RenoDxInstallState};
use serde::Serialize;

use crate::addons::anticheat::RiskAssessment;

use super::super::matcher::IncompatibilityReason;
use super::super::matcher::MatchConfidence;
use super::super::reshade::RenoDxAddonState;
use super::vulkan::VulkanLayerReport;
use crate::addons::reshade::proxy::HostKind;

// The observable ReShade host DTOs are shared; re-exported so the RenoDX
// availability report keeps addressing them here.
pub use crate::addons::reshade::dto::{
    HostActions, HostAddonSupport, HostChannelFacts, HostDetection, HostFacts, HostUpdateStatus,
};

/// Backend-derived RenoDX actions (shared host-action wire shape).
pub type RenoDxActions = HostActions;

/// Read-only preview of whether RenoDX can be installed for a game.
#[derive(Debug, Clone, Serialize)]
pub struct AvailabilityReport {
    /// Current install state for the game.
    pub state: RenoDxInstallState,
    /// Detection state of the Direct3D ReShade proxy host.
    pub host_detection: HostDetection,
    /// Observable Direct3D ReShade host facts, without private install records.
    pub host_facts: HostFacts,
    /// Backend-derived actions the UI may render.
    pub actions: RenoDxActions,
    /// Whether the manifest can provide a stable ReShade host source.
    pub reshade_stable_supported: bool,
    /// Read-only state of the RenoDX add-on file/config, when the expected file
    /// name is known.
    pub renodx_addon: Option<RenoDxAddonState>,
    /// Whether and how RenoDX can be installed.
    pub outcome: AvailabilityOutcome,
    /// The manual "install ReShade host + your own add-on file" escape hatch,
    /// present for a DirectX game that has no automatic or curated-external path.
    pub manual_install: Option<ManualFileInstall>,
    /// Shared Vulkan layer report for Vulkan RenoDX flows.
    pub vulkan_layer: VulkanLayerReport,
}

/// The installability verdict for a game.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AvailabilityOutcome {
    /// A compatible game matched; the add-on can be installed.
    Installable {
        /// Confidence shown to the user (verified / experimental / untested).
        confidence: MatchConfidence,
        /// Ban/stability risk and whether explicit confirmation is required.
        risk: RiskAssessment,
        /// i18n note/requirement keys (a generic install carries its engine label here).
        notes_keys: Vec<String>,
        /// How RenoDX would hook in: a per-game proxy DLL or the shared Vulkan layer.
        host_kind: HostKind,
    },
    /// The add-on is distributed off-GitHub; link the user out, and — when the
    /// game is compatible — offer to install a file the user downloaded.
    External {
        /// Where to send the user (Discord/Nexus).
        url: String,
        /// i18n label key for the link.
        label_key: String,
        /// Present when the game is compatible, enabling "install from file".
        file_install: Option<ExternalFileInstall>,
    },
    /// The game already has native HDR; RenoDX is not offered.
    NativeHdr,
    /// A game matched but cannot be installed for it.
    Incompatible {
        /// Why it cannot be installed.
        reason: IncompatibilityReason,
    },
    /// The game is blacklisted / known-broken.
    Blacklisted {
        /// i18n reason key, when the manifest gives one.
        reason: Option<String>,
    },
    /// No RenoDX profile matched the game.
    Unsupported,
    /// A different addon tool (Luma) is already installed — or unmanaged files
    /// belonging to it were found on disk — for this game. RenoDX and Luma are
    /// mutually exclusive per game; uninstall the other one first.
    BlockedByOtherAddon {
        /// The other addon tool occupying this game. Named `other_kind`, not
        /// `kind`, because the latter collides with this enum's own
        /// `#[serde(tag = "kind")]` discriminant field.
        other_kind: AddonKind,
        /// Whether the block came from an unmanaged on-disk install (`true`)
        /// rather than a tracked database record (`false`).
        unmanaged: bool,
    },
}

/// The manual file-install escape hatch when no automatic or curated-external
/// path is available: install the ReShade host and add a user-downloaded add-on.
#[derive(Debug, Clone, Serialize)]
pub struct ManualFileInstall {
    /// Ban/stability risk and whether explicit confirmation is required (assessed).
    pub risk: RiskAssessment,
    /// How RenoDX would hook in: a per-game proxy DLL or the shared Vulkan layer.
    pub host_kind: HostKind,
    /// The catalogue add-on stem (`renodx-<slug>`) when a title matched, for a soft
    /// filename check in the UI; `None` for an unrecognized game.
    pub expected_addon_name: Option<String>,
    /// The game's architecture (`"x64"` / `"x86"`) for an immediate add-on-arch
    /// check in the UI; `None` when detection was inconclusive.
    pub game_arch: Option<String>,
}

/// The file-install offer for a compatible external game, shown alongside the link.
#[derive(Debug, Clone, Serialize)]
pub struct ExternalFileInstall {
    /// Confidence shown to the user (verified / experimental / untested).
    pub confidence: MatchConfidence,
    /// Ban/stability risk and whether explicit confirmation is required.
    pub risk: RiskAssessment,
    /// i18n note/requirement keys.
    pub notes_keys: Vec<String>,
    /// How RenoDX would hook in: a per-game proxy DLL or the shared Vulkan layer.
    pub host_kind: HostKind,
}
