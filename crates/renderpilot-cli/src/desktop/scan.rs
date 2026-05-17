use std::path::PathBuf;

use super::catalog::GameDetailsOutput;
use super::utils::{to_json, JsonResult};
use crate::catalog;

#[cfg(windows)]
mod auto;
#[cfg(windows)]
pub use auto::scan_auto_libraries;

/// Discovers and catalogs all games from auto-detected library sources.
///
/// On non-Windows platforms this returns an error because auto-scan
/// relies on Windows-specific game library discovery.
#[cfg(not(windows))]
pub fn scan_auto_libraries() -> JsonResult {
    Err(crate::CliError::CommandFailed(
        "auto-scan is only supported on Windows".into(),
    ))
}

/// Scans a manually chosen folder.
///
/// JSON payload:
/// `{ "games": [ ... ] }`
pub fn scan_manual_folder(path: PathBuf) -> JsonResult {
    let games = catalog::scan_folder(path)?
        .into_iter()
        .map(|result| GameDetailsOutput::load(result.game.id().clone()))
        .collect::<Result<Vec<_>, _>>()?;

    to_json(GamesOutput { games })
}

#[derive(Debug, serde::Serialize)]
struct GamesOutput {
    games: Vec<GameDetailsOutput>,
}
