/// Availability data transfer objects.
use renderpilot_domain::RenoDxInstallState;
use serde::Serialize;

use super::super::anticheat::RiskAssessment;
use super::super::matcher::IncompatibilityReason;
use super::super::matcher::MatchConfidence;
use super::super::policy::HostKind;
use super::super::reshade::RenoDxAddonState;
use super::super::types::ReshadeChannel;
use super::actions::ActionDescriptor;
use super::vulkan::VulkanLayerReport;
use std::path::PathBuf;

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

/// Public Direct3D ReShade host detection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostDetection {
    /// No usable proxy host was detected.
    Absent,
    /// A single proxy host was detected.
    Present,
    /// The proxy host state is ambiguous or unsafe.
    Conflict,
}

/// Whether the detected ReShade host can load RenoDX add-ons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostAddonSupport {
    /// Full ReShade add-on API support is available.
    Full,
    /// ReShade is present but does not expose the add-on API RenoDX needs.
    Limited,
    /// The backend could not confirm the add-on API capability.
    Unknown,
}

/// Public update/repair verdict for the ReShade host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostUpdateStatus {
    /// The host is current for the known channel.
    Current,
    /// A host update is available.
    UpdateAvailable,
    /// Repair is available to provide add-on support or replace an incomplete host.
    RepairAvailable,
    /// The backend needs stronger validation before it can claim current/update.
    UnknownNeedsValidation,
    /// The host matches a different known channel than the selected/effective one.
    ChannelMismatch,
}

/// Selected/effective/detected channel facts for a ReShade host.
#[derive(Debug, Clone, Serialize)]
pub struct HostChannelFacts {
    /// User-selected channel for new actions.
    pub selected: ReshadeChannel,
    /// Effective channel after manifest fallback.
    pub effective: ReshadeChannel,
    /// Channel detected from private install metadata, when available.
    pub detected: Option<ReshadeChannel>,
}

/// Observable Direct3D ReShade host facts.
#[derive(Debug, Clone, Serialize)]
pub struct HostFacts {
    /// Proxy slot/file name, when known.
    pub slot: Option<String>,
    /// Whether the detected host is in the active slot.
    pub active: bool,
    /// Host path, when present.
    pub path: Option<PathBuf>,
    /// ReShade file version, when readable.
    pub version: Option<String>,
    /// Add-on API support.
    pub addon_support: HostAddonSupport,
    /// Channel facts.
    pub channel: HostChannelFacts,
    /// Update/repair verdict.
    pub update_status: HostUpdateStatus,
}

/// Backend-derived RenoDX actions.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RenoDxActions {
    /// Install action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<ActionDescriptor>,
    /// Use the compatible detected host without claiming host removal rights.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_existing: Option<ActionDescriptor>,
    /// Repair ReShade for RenoDX add-on support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair: Option<ActionDescriptor>,
    /// Update the detected host when backend validation allows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<ActionDescriptor>,
    /// Switch the host channel when a recorded host artifact allows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_channel: Option<ActionDescriptor>,
    /// Resolve a host conflict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_conflict: Option<ActionDescriptor>,
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
