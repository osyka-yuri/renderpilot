use super::catalog::GameDetailsOutput;
use super::utils::{to_json, JsonResult};
use renderpilot_orchestration::catalog;
use std::path::PathBuf;

/// Scans auto-detected library sources.
#[cfg(windows)]
pub fn scan_auto_libraries(context: &renderpilot_orchestration::Context) -> JsonResult {
    let result = renderpilot_orchestration::catalog::scan::discovery::scan_auto_libraries(context);

    let games = result
        .games
        .into_iter()
        .map(|game_id| GameDetailsOutput::load(context, &game_id))
        .collect::<Result<Vec<_>, _>>()?;

    let output = AutoScanOutput {
        games,
        errors: result
            .errors
            .into_iter()
            .map(|e| ScanErrorOutput {
                root: e.root,
                message: e.message,
            })
            .collect(),
    };

    to_json(output)
}

#[cfg(windows)]
#[derive(Debug, serde::Serialize)]
struct AutoScanOutput {
    games: Vec<GameDetailsOutput>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<ScanErrorOutput>,
}

#[cfg(windows)]
#[derive(Debug, serde::Serialize)]
struct ScanErrorOutput {
    root: String,
    message: String,
}

/// Discovers and catalogs all games from auto-detected library sources.
///
/// Returns an error on non-Windows platforms, as the auto-scan
/// functionality relies on Windows-specific game library discovery.
#[cfg(not(windows))]
pub fn scan_auto_libraries(_context: &renderpilot_orchestration::Context) -> JsonResult {
    Err(crate::ApiError::Service(
        renderpilot_orchestration::ServiceError::CommandFailed(
            "auto-scan is only supported on Windows".into(),
        ),
    ))
}

/// Scans a manually chosen folder.
pub fn scan_manual_folder(
    context: &renderpilot_orchestration::Context,
    path: PathBuf,
) -> JsonResult {
    let games = catalog::scan_folder(context, path)?
        .into_iter()
        .map(|result| GameDetailsOutput::load(context, result.game.id()))
        .collect::<Result<Vec<_>, _>>()?;

    to_json(GamesOutput { games })
}

#[derive(Debug, serde::Serialize)]
struct GamesOutput {
    games: Vec<GameDetailsOutput>,
}
