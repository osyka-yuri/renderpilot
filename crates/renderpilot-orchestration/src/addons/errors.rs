//! Shared constructors for addon-tool [`ServiceError`]s, so every tool reports
//! failures with one consistent vocabulary:
//!
//! * [`failed`] — an internal or integrity fault ([`ServiceError::CommandFailed`]).
//! * [`invalid`] — a user-actionable problem ([`ServiceError::InvalidInput`]).
//! * [`io`] — a filesystem operation failure, formatted with the path.
//!
//! Tool-specific constructors (a `not_installed` naming the tool, RenoDX's
//! Vulkan/channel messages) stay in each tool's own `errors` module, which
//! re-exports these three so call sites keep addressing one module.

use std::io;
use std::path::Path;

use crate::ServiceError;

/// An internal or integrity fault (manifest, serialization, verification).
pub(crate) fn failed(message: impl Into<String>) -> ServiceError {
    ServiceError::CommandFailed(message.into())
}

/// A user-actionable problem (unsupported game, needs confirmation, blocked).
pub(crate) fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError::InvalidInput(message.into())
}

/// A filesystem operation failure, e.g. `failed to back up \`<path>\`: <error>`.
pub(crate) fn io(action: &str, path: &Path, error: &io::Error) -> ServiceError {
    failed(format!("failed to {action} `{}`: {error}", path.display()))
}
