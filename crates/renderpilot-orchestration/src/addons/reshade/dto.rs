//! Shared, backend-authored wire DTOs for the ReShade host, consumed by each
//! tool's availability report and rendered by the UI.

use std::path::PathBuf;

use serde::Serialize;

use super::types::ReshadeChannel;

/// Public ReShade host detection state.
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

/// Whether the detected ReShade host can load add-ons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostAddonSupport {
    /// Full ReShade add-on API support is available.
    Full,
    /// ReShade is present but does not expose the add-on API a tool needs.
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
    /// The host matches a different known channel than the selected one.
    ChannelMismatch,
}

/// Selected and detected channel facts for a ReShade host.
#[derive(Debug, Clone, Serialize)]
pub struct HostChannelFacts {
    /// User-selected channel for new actions.
    pub selected: ReshadeChannel,
    /// Channel detected from private install metadata, when available.
    pub detected: Option<ReshadeChannel>,
}

/// Observable ReShade host facts.
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
    /// Whether the active slot is a recognized non-ReShade build (e.g. GShade) a
    /// tool never checks for updates or replaces automatically.
    pub is_custom_build: bool,
}

/// Confirmation scope for a backend-authored action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionConfirmationScope {
    /// Action affects all Vulkan RenoDX games.
    AllVulkanRenoDxGames,
}

/// Public disabled reason for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionDisabledReason {
    /// A conflict must be resolved first.
    BlockedByConflict,
    /// Stable is unavailable in the manifest.
    StableUnavailable,
    /// The visible target is read-only in this version.
    ReadOnly,
    /// Unsupported platform or architecture.
    Unsupported,
    /// More validation is needed first.
    ValidationRequired,
}

/// Backend-authored action descriptor. Absence means the UI must not render the action.
#[derive(Debug, Clone, Serialize)]
pub struct ActionDescriptor {
    /// Whether the action can be invoked.
    pub enabled: bool,
    /// Whether confirmation is required before invocation.
    pub requires_confirmation: bool,
    /// Confirmation scope, when required.
    pub confirmation_scope: Option<ActionConfirmationScope>,
    /// Why the action is disabled, when disabled.
    pub disabled_reason: Option<ActionDisabledReason>,
    /// Target channel for channel-switch actions.
    pub target_channel: Option<ReshadeChannel>,
}

impl ActionDescriptor {
    /// Creates an enabled action.
    #[must_use]
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            requires_confirmation: false,
            confirmation_scope: None,
            disabled_reason: None,
            target_channel: None,
        }
    }

    /// Creates a disabled action with a public reason.
    #[must_use]
    pub fn disabled(reason: ActionDisabledReason) -> Self {
        Self {
            enabled: false,
            requires_confirmation: false,
            confirmation_scope: None,
            disabled_reason: Some(reason),
            target_channel: None,
        }
    }

    /// Adds a confirmation requirement.
    #[must_use]
    pub fn with_confirmation(mut self, scope: ActionConfirmationScope) -> Self {
        self.requires_confirmation = true;
        self.confirmation_scope = Some(scope);
        self
    }

    /// Adds a target channel.
    #[must_use]
    pub fn with_target_channel(mut self, channel: ReshadeChannel) -> Self {
        self.target_channel = Some(channel);
        self
    }
}

/// Backend-derived host actions shared by every ReShade-hosted tool.
///
/// `switch_channel` is only set by tools that support channel switching (RenoDX).
/// Luma always leaves it `None` and serde skips it on the wire.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HostActions {
    /// Install action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<ActionDescriptor>,
    /// Use the compatible detected host without claiming host removal rights.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_existing: Option<ActionDescriptor>,
    /// Repair ReShade for add-on support.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Acceptance test: no forbidden ownership/proof terms in the serialized
    /// `ActionDescriptor` (the public wire shape the UI consumes).
    #[test]
    fn action_descriptor_serializes_without_forbidden_terms() {
        let descriptor = ActionDescriptor::enabled()
            .with_confirmation(ActionConfirmationScope::AllVulkanRenoDxGames)
            .with_target_channel(ReshadeChannel::Stable);
        let json = serde_json::to_string(&descriptor).expect("serializes");
        for forbidden in [
            "managed",
            "unmanaged",
            "foreign",
            "owned",
            "ownership",
            "managed_by_us",
            "marker",
            "marker_version",
            "source",
            "digest",
            "sha256",
            "validator",
            "backup_path",
            "rollback_manifest",
            "created_by",
            "installed_by",
            "tracked_source",
            "provenance",
        ] {
            assert!(
                !json.contains(forbidden),
                "ActionDescriptor JSON contains forbidden term `{forbidden}`: {json}"
            );
        }
    }

    #[test]
    fn host_actions_skips_none_slots_on_wire() {
        let actions = HostActions {
            install: Some(ActionDescriptor::enabled()),
            ..HostActions::default()
        };
        let json = serde_json::to_string(&actions).expect("serializes");
        assert!(json.contains("install"));
        assert!(
            !json.contains("switch_channel"),
            "unset switch_channel must be omitted: {json}"
        );
        assert!(
            !json.contains("resolve_conflict"),
            "unset resolve_conflict must be omitted: {json}"
        );
    }
}
