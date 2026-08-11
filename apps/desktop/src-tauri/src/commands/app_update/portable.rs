//! Portable updater backend. The App sends requests; the supervisor owns I/O.

use tauri::ipc::Channel;

use super::{
    AppUpdateDownloadEvent,
    dto::{AppUpdateMetadata, UpdateResult},
    session::{self, AppUpdateState, UpdateSession},
};
use crate::commands::{CommandError, error::CommandErrorKind};

pub(super) fn check(session_id: &str) -> UpdateResult<Option<AppUpdateMetadata>> {
    let response = crate::portable_runtime::activation::request_update(
        session_id,
        crate::portable_runtime::app_protocol::PortableUpdateRequest::Check,
    )
    .map_err(map_error)?;
    let crate::portable_runtime::app_protocol::PortableUpdateResponse::Check {
        available,
        current_version,
        version,
        date,
        body,
    } = response
    else {
        return Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateCheckFailed,
            "supervisor returned an invalid check response",
        ));
    };
    Ok(available.then_some(AppUpdateMetadata {
        current_version,
        version,
        date,
        body,
    }))
}

pub(super) fn download(
    state: &AppUpdateState,
    session_id: String,
    on_event: &Channel<AppUpdateDownloadEvent>,
) -> UpdateResult<()> {
    session::require_portable(state, &session_id, false)?;
    let response = crate::portable_runtime::activation::request_update(
        &session_id,
        crate::portable_runtime::app_protocol::PortableUpdateRequest::Download,
    )
    .map_err(map_error)?;
    let crate::portable_runtime::app_protocol::PortableUpdateResponse::Downloaded {
        content_length,
    } = response
    else {
        return Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateDownloadFailed,
            "supervisor returned an invalid download response",
        ));
    };
    let _ = on_event.send(AppUpdateDownloadEvent::Started {
        content_length: Some(content_length),
    });
    let _ = on_event.send(AppUpdateDownloadEvent::Progress {
        chunk_length: usize::try_from(content_length).unwrap_or(usize::MAX),
    });
    let _ = on_event.send(AppUpdateDownloadEvent::Finished);
    *session::lock(state)? = UpdateSession::Downloaded { id: session_id };
    Ok(())
}

pub(super) fn apply(state: &AppUpdateState, session_id: &str) -> UpdateResult<()> {
    session::require_portable(state, session_id, true)?;
    match crate::portable_runtime::activation::request_update(
        session_id,
        crate::portable_runtime::app_protocol::PortableUpdateRequest::Apply,
    )
    .map_err(map_error)?
    {
        crate::portable_runtime::app_protocol::PortableUpdateResponse::ApplyAccepted => Ok(()),
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
