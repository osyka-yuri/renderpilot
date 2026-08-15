//! Portable updater backend. The App sends requests; the supervisor owns I/O.

use tauri::ipc::Channel;

use super::{
    AppUpdateDownloadEvent,
    dto::{AppUpdateMetadata, UpdateResult},
    session::{self, AppUpdateState},
};
use crate::commands::{CommandError, error::CommandErrorKind};

pub(super) fn check(session_id: &str) -> UpdateResult<Option<AppUpdateMetadata>> {
    let response = crate::portable_runtime::activation::request_update(
        session_id,
        &crate::portable_runtime::app_protocol::PortableUpdateRequest::check(),
    )
    .map_err(map_error)?;
    let crate::portable_runtime::app_protocol::PortableUpdateResponse::Check(response) = response
    else {
        return Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateCheckFailed,
            "supervisor returned an invalid check response",
        ));
    };
    Ok(response.available.then_some(AppUpdateMetadata {
        current_version: response.current_version,
        version: response.version,
        date: response.date,
        body: response.body,
    }))
}

pub(super) fn download(
    state: &AppUpdateState,
    session_id: &str,
    on_event: &Channel<AppUpdateDownloadEvent>,
) -> UpdateResult<()> {
    session::require_portable(state, session_id, false)?;
    let response = crate::portable_runtime::activation::request_download_update(
        session_id,
        |event| match event {
            crate::portable_runtime::activation::PortableDownloadEvent::Started {
                content_length,
            } => {
                let _ = on_event.send(AppUpdateDownloadEvent::Started { content_length });
            }
            crate::portable_runtime::activation::PortableDownloadEvent::Progress {
                chunk_length,
            } => {
                let _ = on_event.send(AppUpdateDownloadEvent::Progress { chunk_length });
            }
            crate::portable_runtime::activation::PortableDownloadEvent::Finished => {
                let _ = on_event.send(AppUpdateDownloadEvent::Finished);
            }
        },
    )
    .map_err(map_error)?;
    let crate::portable_runtime::app_protocol::PortableUpdateResponse::Downloaded(_) = response
    else {
        return Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateDownloadFailed,
            "supervisor returned an invalid download response",
        ));
    };
    session::complete_portable_download(state, session_id)
}

pub(super) fn apply(state: &AppUpdateState, session_id: &str) -> UpdateResult<()> {
    session::require_portable(state, session_id, true)?;
    match crate::portable_runtime::activation::request_update(
        session_id,
        &crate::portable_runtime::app_protocol::PortableUpdateRequest::apply(),
    )
    .map_err(map_error)?
    {
        crate::portable_runtime::app_protocol::PortableUpdateResponse::ApplyAccepted(_) => Ok(()),
        _ => Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateApplyFailed,
            "supervisor did not accept the staged portable update",
        )),
    }
}

pub(super) fn map_error(
    error: crate::portable_runtime::error::PortableRuntimeError,
) -> CommandError {
    CommandError::with_diagnostic(CommandErrorKind::AppUpdateSupervisorFailed, error)
}
