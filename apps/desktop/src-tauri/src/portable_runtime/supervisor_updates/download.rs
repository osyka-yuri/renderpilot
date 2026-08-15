use std::{io::Read, time::Duration};

use super::{UpdateOffer, http_client};
use crate::portable_runtime::{
    app_protocol::PortableUpdateEvent,
    error::{PortableRuntimeError, Result},
    staging::{StagedVerifiedRpu, stage_verified_rpu_expected},
};

const MAX_RPU_BYTES: u64 = 1024 * 1024 * 1024;
const LOGICAL_PROGRESS_BYTES: u64 = 64 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug)]
pub(in crate::portable_runtime) enum DownloadStageError {
    Operation(PortableRuntimeError),
    EventTransport(PortableRuntimeError),
}

impl From<PortableRuntimeError> for DownloadStageError {
    fn from(error: PortableRuntimeError) -> Self {
        Self::Operation(error)
    }
}

type DownloadResult<T> = std::result::Result<T, DownloadStageError>;

pub(super) fn download_and_stage(
    update_root: &std::path::Path,
    offer: &UpdateOffer,
    emit: &mut impl FnMut(PortableUpdateEvent) -> Result<()>,
) -> DownloadResult<(u64, StagedVerifiedRpu)> {
    let response = http_client(DOWNLOAD_TIMEOUT)?
        .get(&offer.url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| {
            PortableRuntimeError::new("portable_update_download", error.to_string())
        })?;
    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > MAX_RPU_BYTES) {
        return Err(operation_error(
            "portable_update_download",
            "portable RPU exceeded maximum size",
        ));
    }
    let bytes = read_limited_body_with_events(
        response,
        MAX_RPU_BYTES,
        content_length,
        "portable_update_download",
        "portable RPU exceeded maximum size",
        emit,
    )?;
    let content_length = u64::try_from(bytes.len()).map_err(|_| {
        operation_error(
            "portable_update_download",
            "portable RPU length did not fit u64",
        )
    })?;
    let staged =
        stage_verified_rpu_expected(update_root, &bytes, &offer.signature, &offer.version)?;
    Ok((content_length, staged))
}

pub(in crate::portable_runtime) fn read_limited_body_with_events(
    mut body: impl Read,
    maximum: u64,
    content_length: Option<u64>,
    code: &'static str,
    too_large: &'static str,
    emit: &mut impl FnMut(PortableUpdateEvent) -> Result<()>,
) -> DownloadResult<Vec<u8>> {
    if content_length.is_some_and(|length| length > maximum) {
        return Err(operation_error(code, too_large));
    }
    emit(PortableUpdateEvent::download_started(content_length))
        .map_err(DownloadStageError::EventTransport)?;
    let mut bytes = Vec::new();
    let mut progress = LogicalProgress::default();
    let mut buffer = [0_u8; LOGICAL_PROGRESS_BYTES as usize];

    loop {
        let accepted_total = u64::try_from(bytes.len())
            .map_err(|_| operation_error(code, "portable RPU length did not fit u64"))?;
        let maximum_remaining = maximum
            .checked_sub(accepted_total)
            .ok_or_else(|| operation_error(code, "portable RPU length exceeded its maximum"))?;
        let known_remaining = content_length.map_or(maximum_remaining, |length| {
            length.saturating_sub(accepted_total)
        });
        let accepted_capacity = maximum_remaining.min(known_remaining);
        let read_length = usize::try_from(accepted_capacity.min(LOGICAL_PROGRESS_BYTES))
            .map_err(|_| operation_error(code, "portable RPU read range overflow"))?
            .saturating_add(1)
            .min(buffer.len());
        let read = body
            .read(&mut buffer[..read_length])
            .map_err(|error| operation_error(code, error.to_string()))?;
        if read == 0 {
            if content_length.is_some_and(|length| length != accepted_total) {
                return Err(operation_error(
                    code,
                    "portable RPU body did not match its declared content length",
                ));
            }
            progress.finish(emit)?;
            return Ok(bytes);
        }

        let accepted_capacity = usize::try_from(accepted_capacity)
            .map_err(|_| operation_error(code, "portable RPU read range overflow"))?;
        let accepted = read.min(accepted_capacity);
        if accepted > 0 {
            bytes.extend_from_slice(&buffer[..accepted]);
            let accepted = u64::try_from(accepted)
                .map_err(|_| operation_error(code, "portable RPU read length did not fit u64"))?;
            progress.accept(accepted, emit)?;
        }
        let read = u64::try_from(read)
            .map_err(|_| operation_error(code, "portable RPU read length overflow"))?;
        if read > maximum_remaining {
            return Err(operation_error(code, too_large));
        }
        if content_length.is_some_and(|_| read > known_remaining) {
            return Err(operation_error(
                code,
                "portable RPU body exceeded its declared content length",
            ));
        }
    }
}

fn operation_error(code: &'static str, message: impl Into<String>) -> DownloadStageError {
    DownloadStageError::Operation(PortableRuntimeError::new(code, message))
}

#[derive(Default)]
struct LogicalProgress {
    pending: u64,
}

impl LogicalProgress {
    fn accept(
        &mut self,
        bytes: u64,
        emit: &mut impl FnMut(PortableUpdateEvent) -> Result<()>,
    ) -> DownloadResult<()> {
        self.pending = self.pending.checked_add(bytes).ok_or_else(|| {
            operation_error(
                "portable_update_download",
                "portable RPU progress overflowed",
            )
        })?;
        while self.pending >= LOGICAL_PROGRESS_BYTES {
            emit(PortableUpdateEvent::download_progress(
                LOGICAL_PROGRESS_BYTES,
            ))
            .map_err(DownloadStageError::EventTransport)?;
            self.pending -= LOGICAL_PROGRESS_BYTES;
        }
        Ok(())
    }

    fn finish(
        &mut self,
        emit: &mut impl FnMut(PortableUpdateEvent) -> Result<()>,
    ) -> DownloadResult<()> {
        if self.pending > 0 {
            emit(PortableUpdateEvent::download_progress(self.pending))
                .map_err(DownloadStageError::EventTransport)?;
            self.pending = 0;
        }
        emit(PortableUpdateEvent::download_finished()).map_err(DownloadStageError::EventTransport)
    }
}
