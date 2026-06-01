//! Desktop-layer facade for the global NVIDIA DLSS indicator overlay toggle.
//!
//! The DLSS indicator is a single machine-wide registry value (read by NGX for
//! every DLSS title), so unlike the per-game NVAPI handlers these functions take
//! no game id. Reads work unprivileged; writes need an elevated process and map
//! `ERROR_ACCESS_DENIED` to [`CliError::NvapiRequiresElevation`] so the frontend
//! can surface its existing "Relaunch as administrator" flow.

use crate::desktop::utils::{to_json, JsonResult};

/// Raw OS error code for `ERROR_ACCESS_DENIED`.
/// Writing to `HKLM\SOFTWARE` requires an elevated process.
#[cfg(windows)]
const ERROR_ACCESS_DENIED: i32 = 5;

/// Reads whether the system-wide DLSS indicator overlay is currently enabled.
///
/// Returns `{ "enabled": bool, "supported": bool }`. `supported` is `false` on
/// non-Windows platforms, where the registry toggle does not exist.
#[cfg(windows)]
pub fn get_dlss_indicator_state() -> JsonResult {
    use crate::error::CliError;
    use renderpilot_nvapi::Nvapi;
    use renderpilot_platform_windows::dlss::read_dlss_indicator_enabled;

    let enabled = read_dlss_indicator_enabled().map_err(|error| {
        CliError::CommandFailed(format!("could not read the DLSS indicator state: {error}"))
    })?;
    // The indicator is an NGX feature; absence of nvapi64.dll means no NVIDIA driver
    // (hence no NGX), so report it unsupported and let the UI hide the toggle.
    to_json(serde_json::json!({ "enabled": enabled, "supported": Nvapi::get().is_some() }))
}

/// Non-Windows stub: the DLSS indicator registry toggle does not exist.
#[cfg(not(windows))]
pub fn get_dlss_indicator_state() -> JsonResult {
    to_json(serde_json::json!({ "enabled": false, "supported": false }))
}

/// Enables or disables the system-wide DLSS indicator overlay, returning the
/// fresh `{ "enabled": bool, "supported": bool }` state.
#[cfg(windows)]
pub fn set_dlss_indicator_enabled(enabled: bool) -> JsonResult {
    use crate::error::CliError;
    use renderpilot_nvapi::Nvapi;
    use renderpilot_platform_windows::dlss::set_dlss_indicator_enabled as write_indicator;

    write_indicator(enabled).map_err(|error| {
        if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
            CliError::NvapiRequiresElevation
        } else {
            CliError::CommandFailed(format!("could not update the DLSS indicator: {error}"))
        }
    })?;
    to_json(serde_json::json!({ "enabled": enabled, "supported": Nvapi::get().is_some() }))
}

/// Non-Windows stub: changing the DLSS indicator is unsupported.
#[cfg(not(windows))]
pub fn set_dlss_indicator_enabled(_enabled: bool) -> JsonResult {
    Err(crate::error::CliError::CommandFailed(
        "the DLSS indicator is only available on Windows".to_owned(),
    ))
}
