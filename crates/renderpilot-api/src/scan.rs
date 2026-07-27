use super::utils::{JsonResult, to_json};
use renderpilot_orchestration::catalog;
use std::path::PathBuf;

/// Scans auto-detected library sources.
#[cfg(windows)]
pub fn scan_auto_libraries(context: &renderpilot_orchestration::Context) -> JsonResult {
    to_json(scan_auto_libraries_with_mode(context, false))
}

#[cfg(windows)]
fn scan_auto_libraries_with_mode(
    context: &renderpilot_orchestration::Context,
    background: bool,
) -> AutoScanOutput {
    let result = if background {
        renderpilot_orchestration::catalog::scan::discovery::scan_auto_libraries_background(context)
    } else {
        renderpilot_orchestration::catalog::scan::discovery::scan_auto_libraries(context)
    };

    let changed_game_ids = result.delta.changed_game_ids();
    AutoScanOutput {
        added_game_ids: result
            .delta
            .added_game_ids
            .into_iter()
            .map(|game_id| game_id.as_str().to_owned())
            .collect(),
        updated_game_ids: result
            .delta
            .updated_game_ids
            .into_iter()
            .map(|game_id| game_id.as_str().to_owned())
            .collect(),
        changed_game_ids: changed_game_ids
            .into_iter()
            .map(|game_id| game_id.as_str().to_owned())
            .collect(),
        removed_game_ids: result
            .delta
            .removed_game_ids
            .into_iter()
            .map(|game_id| game_id.as_str().to_owned())
            .collect(),
        errors: result
            .errors
            .into_iter()
            .map(|e| ScanErrorOutput {
                root: e.root,
                message: e.message,
            })
            .collect(),
    }
}

/// Runs a checkpoint-aware background scan and returns its typed delta for the
/// startup coordinator. Serialization remains at the transport boundary.
#[cfg(windows)]
pub fn scan_auto_libraries_background_output(
    context: &renderpilot_orchestration::Context,
) -> Result<AutoScanOutput, crate::ApiError> {
    Ok(scan_auto_libraries_with_mode(context, true))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
/// Structured delta produced by an automatic library scan.
pub struct AutoScanOutput {
    /// Games first inserted by this scan.
    pub added_game_ids: Vec<String>,
    /// Existing games whose catalog facts changed.
    pub updated_game_ids: Vec<String>,
    /// Union of added and updated game identifiers.
    pub changed_game_ids: Vec<String>,
    /// Games removed because an authoritative source no longer contains them.
    pub removed_game_ids: Vec<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Per-root failures that did not prevent other sources from completing.
    pub errors: Vec<ScanErrorOutput>,
}

#[derive(Debug, serde::Serialize)]
/// Non-fatal failure for one automatic scan root.
pub struct ScanErrorOutput {
    /// Root that could not be scanned completely.
    pub root: String,
    /// User-safe diagnostic message.
    pub message: String,
}

/// Discovers and catalogs all games from auto-detected library sources.
///
/// Returns an error on non-Windows platforms, as the auto-scan
/// functionality relies on Windows-specific game library discovery.
#[cfg(not(windows))]
pub fn scan_auto_libraries(_context: &renderpilot_orchestration::Context) -> JsonResult {
    Err(crate::ApiError::Service(
        renderpilot_orchestration::ServiceError::command_failed(
            "auto-scan is only supported on Windows",
        ),
    ))
}

#[cfg(not(windows))]
/// Returns the typed background-scan result on supported platforms.
///
/// Auto-discovery is Windows-specific, so this adapter reports a stable
/// unsupported-platform error without attempting any catalog mutation.
pub fn scan_auto_libraries_background_output(
    _context: &renderpilot_orchestration::Context,
) -> Result<AutoScanOutput, crate::ApiError> {
    Err(crate::ApiError::Service(
        renderpilot_orchestration::ServiceError::command_failed(
            "auto-scan is only supported on Windows",
        ),
    ))
}

/// Scans a manually chosen folder.
pub fn scan_manual_folder(
    context: &renderpilot_orchestration::Context,
    path: PathBuf,
) -> JsonResult {
    let delta = catalog::scan_folder_delta(context, path)?;
    let changed_game_ids = delta
        .changed_game_ids()
        .into_iter()
        .map(|game_id| game_id.as_str().to_owned())
        .collect();
    to_json(ManualScanOutput {
        added_game_ids: delta
            .added_game_ids
            .into_iter()
            .map(|game_id| game_id.as_str().to_owned())
            .collect(),
        updated_game_ids: delta
            .updated_game_ids
            .into_iter()
            .map(|game_id| game_id.as_str().to_owned())
            .collect(),
        changed_game_ids,
        removed_game_ids: delta
            .removed_game_ids
            .into_iter()
            .map(|game_id| game_id.as_str().to_owned())
            .collect(),
    })
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ManualScanOutput {
    added_game_ids: Vec<String>,
    updated_game_ids: Vec<String>,
    changed_game_ids: Vec<String>,
    removed_game_ids: Vec<String>,
}
