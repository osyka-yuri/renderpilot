use std::path::{Path, PathBuf};

use renderpilot_domain::GameId;
use serde::Serialize;

use super::catalog::GameDetailsOutput;
use super::utils::{normalized_path_string, to_json, JsonResult};
use crate::{catalog, CliError};

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

/// Automatically discovers game library paths and scans them.
///
/// The scan is best-effort across roots: one failing root does not prevent
/// other roots from being scanned. Failures are returned in `errors` instead
/// of being silently discarded.
pub fn scan_auto_libraries() -> JsonResult {
    let mut scan = AutoScanAccumulator::default();

    for root in renderpilot_platform_windows::game_libraries::discover_game_library_roots() {
        scan.scan_root(root);
    }

    to_json(scan.into_output())
}

#[derive(Debug, Default)]
struct AutoScanAccumulator {
    games: Vec<GameDetailsOutput>,
    errors: Vec<ScanErrorOutput>,
}

impl AutoScanAccumulator {
    fn scan_root(&mut self, root: PathBuf) {
        match catalog::scan_auto(root.clone()) {
            Ok(results) => {
                for result in results {
                    self.push_game(&root, result.game.id().clone());
                }
            }
            Err(error) => self.push_error(&root, error),
        }
    }

    fn push_game(&mut self, root: &Path, game_id: GameId) {
        match GameDetailsOutput::load(game_id) {
            Ok(details) => self.games.push(details),
            Err(error) => self.push_error(root, error),
        }
    }

    fn push_error(&mut self, root: &Path, error: CliError) {
        self.errors.push(ScanErrorOutput::new(root, error));
    }

    fn into_output(self) -> AutoScanOutput {
        AutoScanOutput {
            games: self.games,
            errors: self.errors,
        }
    }
}

#[derive(Debug, Serialize)]
struct GamesOutput {
    games: Vec<GameDetailsOutput>,
}

#[derive(Debug, Serialize)]
struct AutoScanOutput {
    games: Vec<GameDetailsOutput>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<ScanErrorOutput>,
}

#[derive(Debug, Serialize)]
struct ScanErrorOutput {
    root: String,
    message: String,
}

impl ScanErrorOutput {
    fn new(root: &Path, error: CliError) -> Self {
        Self {
            root: normalized_path_string(root),
            message: error.to_string(),
        }
    }
}
