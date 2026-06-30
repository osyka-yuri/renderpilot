/// Shared game context helpers for RenoDX use cases.
use std::path::{Path, PathBuf};

use renderpilot_application::GameRepository;
use renderpilot_domain::{GameId, GameInstallation};

use crate::{Context, ServiceError};

use super::errors;
use super::facts::{GameAnalysis, analyze_game};
use super::matcher::{RenoDxResolution, resolve};
use super::types::RenoDxManifest;

/// The user's pinned executable for a game, if set. This is the shared
/// game-level override (also honored by NVAPI); the resolver checks it exists. A
/// storage read error degrades to auto-detection rather than failing the preview.
pub(crate) fn executable_override(context: &Context, game_id: &GameId) -> Option<PathBuf> {
    crate::nvapi::resolve::stored_override_path(context, game_id.as_str())
        .ok()
        .flatten()
}

/// Inspects the game on disk and resolves it against the manifest in one step.
pub(crate) fn analyze_and_resolve(
    game: &GameInstallation,
    manifest: &RenoDxManifest,
    override_path: Option<&Path>,
) -> (GameAnalysis, RenoDxResolution) {
    let analysis = analyze_game(game, override_path);
    let resolution = resolve(manifest, &analysis.facts);
    (analysis, resolution)
}

/// Loads a game's installation by id, or fails with a clear "not found" error.
pub(crate) fn require_game(
    context: &Context,
    game_id: &GameId,
) -> Result<GameInstallation, ServiceError> {
    context
        .storage()
        .find_game(game_id)?
        .ok_or_else(|| errors::game_not_found(game_id))
}
