//! Shared game-context helpers for addon use cases.

use std::path::PathBuf;

use renderpilot_application::GameRepository;
use renderpilot_domain::{GameId, GameInstallation};

use crate::addons::records;
use crate::{Context, ServiceError};

/// The user's pinned executable for a game, if set. This is the shared
/// game-level override (also honored by NVAPI); the resolver checks it exists. A
/// storage read error degrades to auto-detection rather than failing the preview.
pub(crate) fn executable_override(context: &Context, game_id: &GameId) -> Option<PathBuf> {
    crate::nvapi::resolve::stored_override_path(context, game_id.as_str())
        .ok()
        .flatten()
}

/// Loads a game's installation by id, or fails with a clear "not found" error.
pub(crate) fn require_game(
    context: &Context,
    game_id: &GameId,
) -> Result<GameInstallation, ServiceError> {
    context
        .storage()
        .find_game(game_id)?
        .ok_or_else(|| records::game_not_found(game_id))
}
