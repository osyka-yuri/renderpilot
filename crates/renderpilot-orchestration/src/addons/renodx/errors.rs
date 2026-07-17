//! RenoDX [`ServiceError`] constructors: the shared addon vocabulary
//! ([`crate::addons::errors`]) plus RenoDX's own tool-specific messages.

use crate::ServiceError;
use crate::addons::reshade::types::ReshadeChannel;

pub(super) use crate::addons::errors::{failed, invalid, io};

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

/// Install snapshot/revalidation drift while network prepare ran unlocked.
pub(super) fn state_changed_retry_install() -> ServiceError {
    invalid("RenoDX install state changed during preparation; retry the install".to_owned())
}

/// Update snapshot/revalidation drift while network prepare ran unlocked.
pub(super) fn state_changed_retry_update() -> ServiceError {
    invalid("RenoDX install state changed during update preparation; retry the update".to_owned())
}

/// The manifest has no ReShade source for the requested channel. Delegates to the
/// shared constructor ([`crate::addons::reshade::source::channel_unavailable`]) so
/// the message stays identical across tools.
pub(super) fn channel_unavailable(channel: ReshadeChannel) -> ServiceError {
    crate::addons::reshade::source::channel_unavailable(channel)
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
