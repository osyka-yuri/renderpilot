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

/// The install record carries more than one ReShade host source, so the managed
/// host is ambiguous. The single source of truth for this message, shared by every
/// flow that reads or rewrites the host source.
pub(super) fn duplicate_host_sources() -> ServiceError {
    invalid("RenoDX install record has multiple ReShade host sources".to_owned())
}

/// A filesystem operation failure, e.g. `failed to back up \`<path>\`: <error>`.
pub(super) fn io(action: &str, path: &Path, error: &io::Error) -> ServiceError {
    failed(format!("failed to {action} `{}`: {error}", path.display()))
}
