//! Desktop-layer facade for the global NVIDIA DLSS indicator overlay toggle.
//!
//! The DLSS indicator is a single machine-wide registry value (read by NGX for
//! every DLSS title), so unlike the per-game NVAPI handlers these functions take
//! no game id. Reads work unprivileged; a denied write maps `ERROR_ACCESS_DENIED`
//! to a typed [`ServiceError`] with backend-only diagnostics.

use crate::ServiceError;

/// Indicator state.
pub struct IndicatorState {
    /// Enabled state.
    pub enabled: bool,
    /// Supported state.
    pub supported: bool,
}

/// Raw OS error code for `ERROR_ACCESS_DENIED`.
/// Writing to `HKLM\SOFTWARE` can fail when the caller lacks access.
#[cfg(windows)]
const ERROR_ACCESS_DENIED: i32 = 5;

/// Reads whether the system-wide DLSS indicator overlay is currently enabled.
///
/// Returns `{ "enabled": bool, "supported": bool }`. `supported` is `false` on
/// non-Windows platforms, where the registry toggle does not exist.
#[cfg(windows)]
pub fn get_dlss_indicator_state() -> Result<IndicatorState, ServiceError> {
    use renderpilot_nvapi::Nvapi;
    use renderpilot_platform_windows::dlss::read_dlss_indicator_enabled;

    let enabled = read_dlss_indicator_enabled().map_err(|error| {
        ServiceError::command_failed(format!("could not read the DLSS indicator state: {error}"))
    })?;
    // The indicator is an NGX feature; absence of nvapi64.dll means no NVIDIA driver
    // (hence no NGX), so report it unsupported and let the UI hide the toggle.
    Ok(IndicatorState {
        enabled,
        supported: Nvapi::get().is_some(),
    })
}

/// Non-Windows stub: the DLSS indicator registry toggle does not exist.
#[cfg(not(windows))]
pub fn get_dlss_indicator_state() -> Result<IndicatorState, ServiceError> {
    Ok(IndicatorState {
        enabled: false,
        supported: false,
    })
}

/// Enables or disables the system-wide DLSS indicator overlay, returning the
/// fresh `{ "enabled": bool, "supported": bool }` state.
#[cfg(windows)]
pub fn set_dlss_indicator_enabled(enabled: bool) -> Result<IndicatorState, ServiceError> {
    use renderpilot_nvapi::Nvapi;
    use renderpilot_platform_windows::dlss::set_dlss_indicator_enabled as write_indicator;

    write_indicator(enabled).map_err(|error| map_dlss_indicator_write_error(&error))?;
    Ok(IndicatorState {
        enabled,
        supported: Nvapi::get().is_some(),
    })
}

#[cfg(windows)]
fn map_dlss_indicator_write_error(error: &std::io::Error) -> ServiceError {
    if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
        ServiceError::AccessDenied {
            operation: "updating the DLSS indicator".to_owned(),
            detail: format!(
                "Windows returned ERROR_ACCESS_DENIED ({ERROR_ACCESS_DENIED}): {error}"
            ),
        }
    } else {
        ServiceError::command_failed(format!("could not update the DLSS indicator: {error}"))
    }
}

/// Non-Windows stub: changing the DLSS indicator is unsupported.
#[cfg(not(windows))]
pub fn set_dlss_indicator_enabled(_enabled: bool) -> Result<IndicatorState, ServiceError> {
    Err(ServiceError::command_failed(
        "the DLSS indicator is only available on Windows",
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn access_denied_from_the_registry_is_preserved_as_typed_error() {
        let source = std::io::Error::from_raw_os_error(ERROR_ACCESS_DENIED);
        let error = map_dlss_indicator_write_error(&source);

        assert!(matches!(
            error,
            ServiceError::AccessDenied { operation, detail }
                if operation == "updating the DLSS indicator"
                    && detail.contains("ERROR_ACCESS_DENIED (5)")
        ));
    }
}
