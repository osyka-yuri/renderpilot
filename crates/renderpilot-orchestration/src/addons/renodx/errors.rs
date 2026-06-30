//! Shared constructors for RenoDX [`ServiceError`]s, so every flow reports
//! failures with one consistent vocabulary:
//!
//! * [`failed`] — an internal or integrity fault ([`ServiceError::CommandFailed`]).
//! * [`invalid`] — a user-actionable problem ([`ServiceError::InvalidInput`]).
//! * [`game_not_found`] — the requested game is not in the library.
//! * [`io`] — a filesystem operation failure, formatted with the path.

use std::io;
use std::path::Path;

use renderpilot_domain::GameId;

use crate::ServiceError;
use crate::addons::renodx::types::ReshadeChannel;

/// An internal or integrity fault (manifest, serialization, verification).
pub(super) fn failed(message: impl Into<String>) -> ServiceError {
    ServiceError::CommandFailed(message.into())
}

/// A user-actionable problem (unsupported game, needs confirmation, blocked).
pub(super) fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError::InvalidInput(message.into())
}

/// The requested game is not present in the library.
pub(super) fn game_not_found(game_id: &GameId) -> ServiceError {
    ServiceError::GameNotFound(game_id.as_str().to_owned())
}

/// The install record carries more than one ReShade host entry, so the recorded
/// host artifact is ambiguous. The single source of truth for this message,
/// shared by every flow that reads or rewrites the host entry.
pub(super) fn duplicate_host_sources() -> ServiceError {
    invalid("RenoDX install record has multiple ReShade host sources".to_owned())
}

/// No RenoDX install is on record for the game. Shared by every flow that
/// requires an existing install before it can act.
pub(super) fn not_installed() -> ServiceError {
    invalid("RenoDX is not installed for this game".to_owned())
}

/// The manifest has no ReShade source for the requested channel. Shared by
/// every flow that resolves a channel to a downloadable source.
pub(super) fn channel_unavailable(channel: ReshadeChannel) -> ServiceError {
    invalid(format!(
        "ReShade channel `{}` is not available",
        channel.as_str()
    ))
}

/// Shared Vulkan layer management is Windows-only. Shared by every platform
/// entry point reached on an unsupported OS.
pub(super) fn vulkan_unsupported_platform() -> ServiceError {
    invalid("RenoDX for Vulkan games is only supported on Windows".to_owned())
}

/// A shared Vulkan layer conflict that isn't safely auto-resolvable, blocking
/// the mutation `operation` names (e.g. `"installing"`, `"updating"`). Shared
/// by every flow that rejects a `LayerMutationGate::UnresolvedConflict`.
pub(super) fn vulkan_layer_conflict(operation: &str) -> ServiceError {
    invalid(format!(
        "shared Vulkan layer conflict must be resolved before {operation} RenoDX"
    ))
}

/// A filesystem operation failure, e.g. `failed to back up \`<path>\`: <error>`.
pub(super) fn io(action: &str, path: &Path, error: &io::Error) -> ServiceError {
    failed(format!("failed to {action} `{}`: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vulkan_layer_conflict_names_the_blocked_operation() {
        for operation in ["installing", "updating"] {
            match vulkan_layer_conflict(operation) {
                ServiceError::InvalidInput(message) => {
                    assert_eq!(
                        message,
                        format!(
                            "shared Vulkan layer conflict must be resolved before {operation} RenoDX"
                        )
                    );
                }
                other => panic!("expected InvalidInput, got {other:?}"),
            }
        }
    }
}
