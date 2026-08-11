//! Installed Tauri/NSIS updater backend.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tauri::ipc::Channel;
use tauri_plugin_updater::UpdaterExt;

use super::{
    AppUpdateApplyResponse, AppUpdateDownloadEvent,
    dto::{AppUpdateMetadata, UpdateResult},
    session::{self, AppUpdateState, UpdateSession},
};
use crate::commands::{CommandError, error::CommandErrorKind};

pub(super) struct CheckedUpdate {
    pub(super) metadata: AppUpdateMetadata,
    pub(super) update: tauri_plugin_updater::Update,
}

pub(super) async fn check(app: &tauri::AppHandle) -> UpdateResult<Option<CheckedUpdate>> {
    let updater = app.updater_builder().build().map_err(|error| {
        CommandError::with_diagnostic(CommandErrorKind::AppUpdateCheckFailed, error)
    })?;
    let Some(update) = updater.check().await.map_err(|error| {
        CommandError::with_diagnostic(CommandErrorKind::AppUpdateCheckFailed, error)
    })?
    else {
        return Ok(None);
    };
    let metadata = AppUpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        date: update.date.map(|date| date.to_string()),
        body: update.body.clone().unwrap_or_default(),
    };
    Ok(Some(CheckedUpdate { metadata, update }))
}

pub(super) async fn download(
    state: &AppUpdateState,
    session_id: String,
    on_event: Channel<AppUpdateDownloadEvent>,
) -> UpdateResult<()> {
    let update = take_checked(state, &session_id)?;
    let started = Arc::new(AtomicBool::new(false));
    let chunk_started = Arc::clone(&started);
    let finish_started = Arc::clone(&started);
    let chunk_events = on_event.clone();
    let finish_events = on_event.clone();
    let result = update
        .download(
            move |chunk_length, content_length| {
                if !chunk_started.swap(true, Ordering::AcqRel) {
                    let _ = chunk_events.send(AppUpdateDownloadEvent::Started { content_length });
                }
                let _ = chunk_events.send(AppUpdateDownloadEvent::Progress { chunk_length });
            },
            move || {
                if !finish_started.swap(true, Ordering::AcqRel) {
                    let _ = finish_events.send(AppUpdateDownloadEvent::Started {
                        content_length: None,
                    });
                }
                let _ = finish_events.send(AppUpdateDownloadEvent::Finished);
            },
        )
        .await;
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            *session::lock(state)? = UpdateSession::Checked {
                id: session_id,
                update,
            };
            return Err(CommandError::with_diagnostic(
                CommandErrorKind::AppUpdateDownloadFailed,
                error,
            ));
        }
    };
    *session::lock(state)? = UpdateSession::Downloaded {
        id: session_id,
        update,
        bytes,
    };
    Ok(())
}

pub(super) fn apply(
    state: &AppUpdateState,
    session_id: String,
) -> UpdateResult<AppUpdateApplyResponse> {
    let (update, bytes) = take_downloaded(state, &session_id)?;
    if let Err(error) = update.install(&bytes) {
        *session::lock(state)? = UpdateSession::Downloaded {
            id: session_id,
            update,
            bytes,
        };
        return Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateInstallFailed,
            error,
        ));
    }
    Ok(AppUpdateApplyResponse::Installed)
}

fn take_checked(state: &AppUpdateState, id: &str) -> UpdateResult<tauri_plugin_updater::Update> {
    let mut session = session::lock(state)?;
    match std::mem::replace(&mut *session, UpdateSession::Idle) {
        UpdateSession::Checked { id: actual, update } if actual == id => Ok(update),
        other => {
            *session = other;
            Err(CommandError::with_diagnostic(
                CommandErrorKind::AppUpdateInvalidSession,
                "updater session was not ready to download",
            ))
        }
    }
}

fn take_downloaded(
    state: &AppUpdateState,
    id: &str,
) -> UpdateResult<(tauri_plugin_updater::Update, Vec<u8>)> {
    let mut session = session::lock(state)?;
    match std::mem::replace(&mut *session, UpdateSession::Idle) {
        UpdateSession::Downloaded {
            id: actual,
            update,
            bytes,
        } if actual == id => Ok((update, bytes)),
        other => {
            *session = other;
            Err(CommandError::with_diagnostic(
                CommandErrorKind::AppUpdateInvalidSession,
                "updater session was not ready to apply",
            ))
        }
    }
}
