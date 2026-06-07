use crate::{
    utils::{to_json, JsonResult},
    ApiError,
};

/// Reads whether the global NVIDIA DLSS indicator overlay is currently enabled.
#[cfg(windows)]
pub fn get_dlss_indicator_state() -> JsonResult {
    let state = renderpilot_orchestration::dlss::indicator::get_dlss_indicator_state()
        .map_err(ApiError::from)?;
    to_json(serde_json::json!({ "enabled": state.enabled, "supported": state.supported }))
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn get_dlss_indicator_state() -> JsonResult {
    let state = renderpilot_orchestration::dlss::indicator::get_dlss_indicator_state()
        .map_err(ApiError::from)?;
    to_json(serde_json::json!({ "enabled": state.enabled, "supported": state.supported }))
}

/// Enables or disables the global NVIDIA DLSS indicator overlay.
#[cfg(windows)]
pub fn set_dlss_indicator_enabled(enabled: bool) -> JsonResult {
    let state = renderpilot_orchestration::dlss::indicator::set_dlss_indicator_enabled(enabled)
        .map_err(ApiError::from)?;
    to_json(serde_json::json!({ "enabled": state.enabled, "supported": state.supported }))
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn set_dlss_indicator_enabled(enabled: bool) -> JsonResult {
    let state = renderpilot_orchestration::dlss::indicator::set_dlss_indicator_enabled(enabled)
        .map_err(ApiError::from)?;
    to_json(serde_json::json!({ "enabled": state.enabled, "supported": state.supported }))
}
