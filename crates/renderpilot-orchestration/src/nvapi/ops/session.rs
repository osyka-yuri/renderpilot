//! DRS session open and NVAPI error mapping.

use renderpilot_nvapi::{DrsSession, Nvapi, NvapiError};

use super::super::dto::NvapiWarningDto;
use crate::ServiceError;

pub(super) fn map_nvapi_write_error(error: NvapiError, label: &'static str) -> ServiceError {
    match error {
        NvapiError::InvalidUserPrivilege => ServiceError::NvapiRequiresElevation,
        other => ServiceError::command_failed(format!("{label}: {other}")),
    }
}

/// Opens an NVAPI DRS session, classifying each failure step as the
/// [`NvapiWarningDto`] the UI surfaces. `Nvapi::get()` returns a `&'static`
/// handle, so the borrowed session is itself `'static`.
///
/// Read paths match on the warning directly; the write path maps it to a
/// [`ServiceError`] via [`warning_to_service_error`]. This is the single place
/// the `get -> initialize -> create_session` sequence lives.
pub(super) fn open_drs_session() -> Result<DrsSession<'static>, NvapiWarningDto> {
    let nvapi = Nvapi::get().ok_or(NvapiWarningDto::NvapiUnavailable)?;
    nvapi
        .initialize()
        .map_err(|_| NvapiWarningDto::NvapiInitFailed)?;
    nvapi
        .create_session()
        .map_err(|_| NvapiWarningDto::DrsFailed)
}

/// Maps a session-open warning to the user-facing [`ServiceError`] used on the
/// write path, where an unopenable session is a hard failure.
pub(super) fn warning_to_service_error(warning: NvapiWarningDto) -> ServiceError {
    let message = match warning {
        NvapiWarningDto::NvapiUnavailable => "NVAPI unavailable (non-NVIDIA driver or missing dll)",
        NvapiWarningDto::NvapiInitFailed => "NVAPI initialize failed",
        NvapiWarningDto::DrsFailed => "DRS session failed",
        // Not produced by `open_drs_session`, but keep the mapping total.
        other => return ServiceError::command_failed(format!("DRS session failed: {other:?}")),
    };
    ServiceError::command_failed(message)
}
