/// Backend-authored action DTOs shared by RenoDX UI reports.
use serde::Serialize;

use super::super::types::ReshadeChannel;

/// Confirmation scope for a backend-authored action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionConfirmationScope {
    /// Anti-cheat risk confirmation.
    Anticheat,
    /// Action affects all Vulkan RenoDX games.
    AllVulkanRenoDxGames,
}

/// Public disabled reason for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionDisabledReason {
    /// A conflict must be resolved first.
    BlockedByConflict,
    /// Risk policy blocks the action.
    BlockedByRisk,
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
}
