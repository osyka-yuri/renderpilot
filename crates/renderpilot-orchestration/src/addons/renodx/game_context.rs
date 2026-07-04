//! RenoDX game-context: the shared game loaders plus the manifest-typed
//! analyze-and-resolve step.

use std::path::Path;

use renderpilot_domain::GameInstallation;

use super::matcher::{RenoDxResolution, resolve};
use super::types::RenoDxManifest;
use crate::addons::game_analysis::{GameAnalysis, analyze_game};

pub(crate) use crate::addons::game_context::{executable_override, require_game};

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
