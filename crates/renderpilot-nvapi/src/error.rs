//! Error types for NVAPI operations.

use std::{error::Error, fmt};

// ------------------------------------------------------------------
// Known NVAPI status codes
// ------------------------------------------------------------------

/// An argument's structure version is not supported.
pub const NVAPI_INCOMPATIBLE_STRUCT_VERSION: i32 = -9;
/// The application requires Administrator privileges.
pub const NVAPI_INVALID_USER_PRIVILEGE: i32 = -137;
/// The requested setting is not present in the profile.
pub const NVAPI_SETTING_NOT_FOUND: i32 = -160;

/// Errors that can occur when interacting with NVAPI.
#[derive(Debug)]
pub enum NvapiError {
    /// The NVIDIA driver / nvapi.dll is not available.
    DriverUnavailable,
    /// NVAPI initialization failed.
    InitializeFailed(i32),
    /// DRS session could not be created.
    SessionCreateFailed(i32),
    /// DRS settings could not be loaded.
    LoadSettingsFailed(i32),
    /// The requested profile was not found.
    ProfileNotFound,
    /// The requested application was not found in any profile.
    ApplicationNotFound,
    /// Failed to read a setting value.
    GetSettingFailed(i32),
    /// Failed to write a setting value.
    SetSettingFailed(i32),
    /// Failed to delete a setting from a profile.
    DeleteSettingFailed(i32),
    /// Administrator privileges are required to modify DRS settings.
    InvalidUserPrivilege,
    /// Failed to save DRS settings.
    SaveSettingsFailed(i32),
    /// The returned setting type does not match the expected type.
    UnexpectedSettingType,
    /// A required string conversion failed.
    StringConversion,
    /// An internal NVAPI call returned an unexpected status code.
    UnexpectedStatus(i32),
}

impl fmt::Display for NvapiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DriverUnavailable => write!(formatter, "NVIDIA driver not available"),
            Self::InitializeFailed(status) => {
                write!(formatter, "NVAPI initialization failed (status={status})")
            }
            Self::SessionCreateFailed(status) => {
                write!(formatter, "DRS session creation failed (status={status})")
            }
            Self::LoadSettingsFailed(status) => {
                write!(formatter, "DRS load settings failed (status={status})")
            }
            Self::ProfileNotFound => write!(formatter, "DRS profile not found"),
            Self::ApplicationNotFound => {
                write!(formatter, "DRS application not found in any profile")
            }
            Self::GetSettingFailed(status) => {
                let label = match *status {
                    NVAPI_SETTING_NOT_FOUND => " (setting not found)",
                    NVAPI_INCOMPATIBLE_STRUCT_VERSION => " (incompatible struct version)",
                    _ => "",
                };
                write!(formatter, "DRS get setting failed (status={status}){label}")
            }
            Self::SetSettingFailed(status) => {
                write!(formatter, "DRS set setting failed (status={status})")
            }
            Self::DeleteSettingFailed(status) => {
                let label = match *status {
                    NVAPI_SETTING_NOT_FOUND => " (setting not found - already absent)",
                    _ => "",
                };
                write!(
                    formatter,
                    "DRS delete setting failed (status={status}){label}"
                )
            }
            Self::InvalidUserPrivilege => {
                write!(
                    formatter,
                    "Administrator privileges are required to modify DRS settings"
                )
            }
            Self::SaveSettingsFailed(status) => {
                write!(formatter, "DRS save settings failed (status={status})")
            }
            Self::UnexpectedSettingType => {
                write!(formatter, "DRS setting type mismatch")
            }
            Self::StringConversion => write!(formatter, "string conversion failed"),
            Self::UnexpectedStatus(status) => {
                write!(formatter, "unexpected NVAPI status: {status}")
            }
        }
    }
}

impl Error for NvapiError {}
