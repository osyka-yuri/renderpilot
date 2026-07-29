//! Transport contract for automatic library scans.

#[cfg(windows)]
use crate::utils::to_json;
use crate::{ApiError, utils::JsonResult};

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
            .map(|error| ScanErrorOutput {
                root: error.root,
                message: error.message,
            })
            .collect(),
    }
}

/// Runs a checkpoint-aware background scan and returns its typed delta for the
/// startup coordinator. Serialization remains at the transport boundary.
#[cfg(windows)]
pub fn scan_auto_libraries_background_output(
    context: &renderpilot_orchestration::Context,
) -> Result<AutoScanOutput, ApiError> {
    Ok(scan_auto_libraries_with_mode(context, true))
}

/// Structured delta produced by an automatic library scan.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScanOutput {
    /// Games first inserted by this scan.
    pub added_game_ids: Vec<String>,
    /// Existing games whose catalog facts changed.
    pub updated_game_ids: Vec<String>,
    /// Union of added and updated game identifiers.
    pub changed_game_ids: Vec<String>,
    /// Games removed because an authoritative source no longer contains them.
    pub removed_game_ids: Vec<String>,
    /// Per-root failures that did not prevent other sources from completing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ScanErrorOutput>,
}

/// Non-fatal failure for one automatic scan root.
#[derive(Debug, serde::Serialize)]
pub struct ScanErrorOutput {
    /// Root that could not be scanned completely.
    pub root: String,
    /// User-safe diagnostic message.
    pub message: String,
}

/// Reports that automatic discovery is unavailable on non-Windows platforms.
#[cfg(not(windows))]
pub fn scan_auto_libraries(_context: &renderpilot_orchestration::Context) -> JsonResult {
    Err(unsupported_platform_error())
}

/// Reports that background automatic discovery is unavailable on non-Windows platforms.
#[cfg(not(windows))]
pub fn scan_auto_libraries_background_output(
    _context: &renderpilot_orchestration::Context,
) -> Result<AutoScanOutput, ApiError> {
    Err(unsupported_platform_error())
}

#[cfg(not(windows))]
fn unsupported_platform_error() -> ApiError {
    ApiError::Service(renderpilot_orchestration::ServiceError::command_failed(
        "auto-scan is only supported on Windows",
    ))
}
