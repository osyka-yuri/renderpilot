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
    for error in &result.errors {
        log::warn!(
            "Automatic library scan partial failure at {}: {}",
            error.root,
            error.message
        );
    }
    let partial_failure_count = result.errors.len();
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
        partial_failure_count,
    }
}

/// Runs an authoritative background scan and returns its typed delta for the
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
    /// Count of per-root failures that did not prevent other sources from completing.
    pub partial_failure_count: usize,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::AutoScanOutput;

    #[test]
    fn auto_scan_serializes_only_the_partial_failure_count_for_zero_one_and_many() {
        for count in [0, 1, 3] {
            let value = serde_json::to_value(AutoScanOutput {
                added_game_ids: vec![],
                updated_game_ids: vec![],
                changed_game_ids: vec![],
                removed_game_ids: vec![],
                partial_failure_count: count,
            })
            .expect("serialize auto-scan output");

            assert_eq!(value.get("partialFailureCount"), Some(&json!(count)));
            assert!(value.get("errors").is_none());
            assert!(value.get("message").is_none());
            assert!(!value.to_string().contains("root"));
        }
    }
}
