use std::sync::atomic::Ordering;

use super::{AppSession, read_control};
use crate::portable_runtime::{
    app_protocol::{
        AppControlMessage, AppStatusMessage, PortableUpdateEvent, PortableUpdateRequest,
        PortableUpdateResponse, write_message,
    },
    error::{PortableRuntimeError, Result},
};

const MAX_UPDATE_BYTES: u64 = 1024 * 1024 * 1024;
const LOGICAL_PROGRESS_BYTES: u64 = 64 * 1024;

/// Presentation-neutral event emitted only after the App has validated the
/// authenticated supervisor frame. The progress length is already safe for
/// the platform-facing Tauri DTO.
#[derive(Debug, Eq, PartialEq)]
pub enum PortableDownloadEvent {
    Started { content_length: Option<u64> },
    Progress { chunk_length: usize },
    Finished,
}

#[derive(Default)]
pub(in crate::portable_runtime) enum DownloadReceiveState {
    #[default]
    AwaitStarted,
    Streaming {
        content_length: Option<u64>,
        cumulative: u64,
        trailing_partial_seen: bool,
    },
    NetworkFinished {
        content_length: u64,
    },
}

impl DownloadReceiveState {
    fn completed_content_length(&self) -> Option<u64> {
        match self {
            Self::NetworkFinished { content_length } => Some(*content_length),
            Self::AwaitStarted | Self::Streaming { .. } => None,
        }
    }

    fn accept(&mut self, event: &PortableUpdateEvent) -> Result<PortableDownloadEvent> {
        match event {
            PortableUpdateEvent::Started { content_length } => self.start(*content_length),
            PortableUpdateEvent::Progress { chunk_length } => self.progress(*chunk_length),
            PortableUpdateEvent::Finished {} => self.finish(),
        }
    }

    fn start(&mut self, content_length: Option<u64>) -> Result<PortableDownloadEvent> {
        if !matches!(self, Self::AwaitStarted)
            || content_length.is_some_and(|length| length > MAX_UPDATE_BYTES)
        {
            return Err(PortableRuntimeError::new(
                "portable_update_protocol",
                "supervisor update stream started more than once or exceeded its bound",
            ));
        }
        *self = Self::Streaming {
            content_length,
            cumulative: 0,
            trailing_partial_seen: false,
        };
        Ok(PortableDownloadEvent::Started { content_length })
    }

    fn progress(&mut self, chunk_length: u64) -> Result<PortableDownloadEvent> {
        let Self::Streaming {
            content_length,
            cumulative,
            trailing_partial_seen,
        } = self
        else {
            return Err(invalid_progress());
        };
        if chunk_length == 0 || chunk_length > LOGICAL_PROGRESS_BYTES || *trailing_partial_seen {
            return Err(invalid_progress());
        }
        let next = cumulative.checked_add(chunk_length).ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_update_protocol",
                "supervisor update progress overflowed",
            )
        })?;
        if next > MAX_UPDATE_BYTES || content_length.is_some_and(|length| next > length) {
            return Err(PortableRuntimeError::new(
                "portable_update_protocol",
                "supervisor update progress exceeded its authenticated bound",
            ));
        }
        let chunk_length = usize::try_from(chunk_length).map_err(|_| {
            PortableRuntimeError::new(
                "portable_update_protocol",
                "validated progress length did not fit the App platform",
            )
        })?;
        *cumulative = next;
        *trailing_partial_seen = chunk_length < LOGICAL_PROGRESS_BYTES as usize;
        Ok(PortableDownloadEvent::Progress { chunk_length })
    }

    fn finish(&mut self) -> Result<PortableDownloadEvent> {
        let Self::Streaming {
            content_length,
            cumulative,
            ..
        } = self
        else {
            return Err(invalid_finish());
        };
        if content_length.is_some_and(|length| length != *cumulative) {
            return Err(invalid_finish());
        }
        let content_length = *cumulative;
        *self = Self::NetworkFinished { content_length };
        Ok(PortableDownloadEvent::Finished)
    }
}

fn invalid_progress() -> PortableRuntimeError {
    PortableRuntimeError::new(
        "portable_update_protocol",
        "supervisor update progress was out of order or empty",
    )
}

fn invalid_finish() -> PortableRuntimeError {
    PortableRuntimeError::new(
        "portable_update_protocol",
        "supervisor update finish was out of order or had a mismatched length",
    )
}

/// A committed App can request one serialized supervisor-owned updater
/// operation at a time. The App owns only this authenticated DTO round-trip;
/// all network, staging, selection, journaling, and process replacement remain
/// in the supervisor.
pub fn request_update(
    request_id: &str,
    request: &PortableUpdateRequest,
) -> Result<PortableUpdateResponse> {
    request_update_with_events(request_id, request, |_| {})
}

/// Download exchanges expose only validated supervisor progress frames. The
/// callback stays presentation-free so activation never depends on Tauri.
pub fn request_download_update(
    request_id: &str,
    on_event: impl FnMut(PortableDownloadEvent),
) -> Result<PortableUpdateResponse> {
    request_update_with_events(request_id, &PortableUpdateRequest::download(), on_event)
}

fn request_update_with_events(
    request_id: &str,
    request: &PortableUpdateRequest,
    mut on_event: impl FnMut(PortableDownloadEvent),
) -> Result<PortableUpdateResponse> {
    super::require_committed()?;
    exchange_update_with_session(super::session()?, request_id, request, &mut on_event)
}

/// Runs the serialized private-pipe exchange for one authenticated App
/// session. The global wrapper above supplies the installed session; keeping
/// the exchange local makes the fence an attribute of the real session rather
/// than of process-global test state.
pub(in crate::portable_runtime) fn exchange_update_with_session(
    session: &AppSession,
    request_id: &str,
    request: &PortableUpdateRequest,
    on_event: &mut impl FnMut(PortableDownloadEvent),
) -> Result<PortableUpdateResponse> {
    if session.exchange_fenced.load(Ordering::Acquire) {
        return Err(fenced_error());
    }
    let _exchange = session.exchange.lock().map_err(|_| {
        PortableRuntimeError::new("portable_activation", "portable protocol exchange poisoned")
    })?;
    if session.exchange_fenced.load(Ordering::Acquire) {
        return Err(fenced_error());
    }
    let result = (|| {
        let mut status = session.status.lock().map_err(|_| {
            PortableRuntimeError::new("portable_activation", "status pipe poisoned")
        })?;
        write_message(
            &mut *status,
            &AppStatusMessage::update_request(request_id, *request),
        )?;
        drop(status);
        receive_update_exchange(session, request_id, request, on_event)
    })();
    if result.is_err() {
        session.exchange_fenced.store(true, Ordering::Release);
    }
    result
}

fn fenced_error() -> PortableRuntimeError {
    PortableRuntimeError::new(
        "portable_update_fenced",
        "portable App update exchange was permanently fenced",
    )
}

fn receive_update_exchange(
    session: &AppSession,
    request_id: &str,
    request: &PortableUpdateRequest,
    on_event: &mut impl FnMut(PortableDownloadEvent),
) -> Result<PortableUpdateResponse> {
    let mut download = DownloadReceiveState::default();
    loop {
        let message = read_control(session)?;
        if let Some(response) =
            accept_update_exchange_message(request_id, request, &mut download, message, on_event)?
        {
            return Ok(response);
        }
    }
}

pub(in crate::portable_runtime) fn accept_update_exchange_message(
    request_id: &str,
    request: &PortableUpdateRequest,
    download: &mut DownloadReceiveState,
    message: AppControlMessage,
    on_event: &mut impl FnMut(PortableDownloadEvent),
) -> Result<Option<PortableUpdateResponse>> {
    match message {
        AppControlMessage::UpdateEvent(update) => {
            if update.request_id.as_ref() != request_id {
                return Err(PortableRuntimeError::new(
                    "portable_update_protocol",
                    "supervisor update event did not match request",
                ));
            }
            if !matches!(request, PortableUpdateRequest::Download(_)) {
                return Err(PortableRuntimeError::new(
                    "portable_update_protocol",
                    "non-download update exchange received a progress event",
                ));
            }
            accept_download_event(download, &update.event, on_event)?;
            Ok(None)
        }
        AppControlMessage::UpdateResponse(response) => {
            if response.request_id.as_ref() != request_id {
                return Err(PortableRuntimeError::new(
                    "portable_update_protocol",
                    "supervisor update response did not match request",
                ));
            }
            accept_update_terminal(request, response.response, download).map(Some)
        }
        _ => Err(PortableRuntimeError::new(
            "portable_update_protocol",
            "supervisor sent an unexpected update exchange frame",
        )),
    }
}

fn accept_download_event(
    state: &mut DownloadReceiveState,
    event: &PortableUpdateEvent,
    on_event: &mut impl FnMut(PortableDownloadEvent),
) -> Result<()> {
    on_event(state.accept(event)?);
    Ok(())
}

pub(in crate::portable_runtime) fn accept_update_terminal(
    request: &PortableUpdateRequest,
    response: PortableUpdateResponse,
    download: &DownloadReceiveState,
) -> Result<PortableUpdateResponse> {
    if matches!(response, PortableUpdateResponse::Rejected(_)) {
        return Ok(response);
    }
    match (request, &response) {
        (PortableUpdateRequest::Check(_), PortableUpdateResponse::Check(_))
        | (PortableUpdateRequest::Apply(_), PortableUpdateResponse::ApplyAccepted(_)) => {
            Ok(response)
        }
        (PortableUpdateRequest::Download(_), PortableUpdateResponse::Downloaded(downloaded))
            if download.completed_content_length() == Some(downloaded.content_length) =>
        {
            Ok(response)
        }
        _ => Err(PortableRuntimeError::new(
            "portable_update_protocol",
            "supervisor update terminal did not match the authenticated exchange",
        )),
    }
}
