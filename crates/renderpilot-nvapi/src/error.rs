//! Error types for NVAPI operations.

use std::{error::Error, fmt};

// ------------------------------------------------------------------
// Known NVAPI status codes
// ------------------------------------------------------------------

/// An argument's structure version is not supported.
pub const NVAPI_INCOMPATIBLE_STRUCT_VERSION: i32 = -9;
/// The driver rejected the caller's privilege level.
pub const NVAPI_INVALID_USER_PRIVILEGE: i32 = -137;
/// The requested setting is not present in the profile.
pub const NVAPI_SETTING_NOT_FOUND: i32 = -160;
/// The requested application is absent from every driver profile.
pub const NVAPI_EXECUTABLE_NOT_FOUND: i32 = -166;
/// More than one application matches the requested application name.
pub const NVAPI_EXECUTABLE_AMBIGUOUS: i32 = -182;
pub const NVAPI_PROFILE_NOT_FOUND: i32 = -163;
pub const NVAPI_PROFILE_NAME_IN_USE: i32 = -164;

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
    /// The requested profile name is already occupied.
    ProfileNameInUse,
    /// A predefined NVIDIA profile must never be removed as user-owned state.
    ProfileIsPredefined,
    /// The application receipt does not match the current complete witness.
    ApplicationWitnessMismatch,
    /// The global/base driver profile could not be resolved (the
    /// `NvAPI_DRS_GetBaseProfile` entry point is unavailable on this driver,
    /// or the call failed).
    BaseProfileUnavailable,
    /// The requested application was not found in any profile.
    ApplicationNotFound,
    /// The executable is not in any DRS profile. The raw driver status is kept
    /// so the orchestration layer can distinguish a proven absence from errors.
    ExecutableNotFound,
    /// The executable lookup is ambiguous; no profile handle may be mutated.
    ExecutableAmbiguous,
    /// A DRS operation failed, preserving its vendor status code.
    DrsOperationFailed {
        /// DRS operation name reported by RenderPilot.
        operation: &'static str,
        /// Raw status code returned by NVAPI.
        status: i32,
    },
    /// The NVIDIA driver does not expose the requested DRS entry point.
    DrsApiUnavailable(&'static str),
    /// Failed to read a setting value.
    GetSettingFailed(i32),
    /// Failed to write a setting value.
    SetSettingFailed(i32),
    /// Failed to delete a setting from a profile.
    DeleteSettingFailed(i32),
    /// The driver rejected the caller's privilege level for a DRS mutation.
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
            Self::ProfileNameInUse => write!(formatter, "DRS profile name is already in use"),
            Self::ProfileIsPredefined => {
                write!(formatter, "DRS predefined profile cannot be deleted")
            }
            Self::ApplicationWitnessMismatch => {
                write!(formatter, "DRS application witness changed")
            }
            Self::BaseProfileUnavailable => {
                write!(formatter, "DRS global/base profile unavailable")
            }
            Self::ApplicationNotFound => {
                write!(formatter, "DRS application not found in any profile")
            }
            Self::ExecutableNotFound => write!(formatter, "DRS executable not found (status=-166)"),
            Self::ExecutableAmbiguous => write!(
                formatter,
                "DRS executable lookup is ambiguous (status=-182)"
            ),
            Self::DrsOperationFailed { operation, status } => {
                write!(formatter, "DRS {operation} failed (status={status})")
            }
            Self::DrsApiUnavailable(operation) => {
                write!(formatter, "DRS API {operation} is unavailable")
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
                    "NVAPI rejected the caller's DRS mutation privilege"
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
