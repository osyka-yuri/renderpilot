use std::{
    fs::File,
    io::BufReader,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use tauri::{AppHandle, Manager};

use super::{
    app_protocol::{
        AppControlMessage, AppStatusMessage, PortableStartupV3, PortableUpdateRequest,
        PortableUpdateResponse, committed_sequence_for_selection, read_message, write_message,
    },
    error::{PortableRuntimeError, Result},
    runtime_paths,
};

struct AppSession {
    control: Mutex<BufReader<File>>,
    status: Mutex<File>,
    exchange: Mutex<()>,
    startup: PortableStartupV3,
}

static SESSION: OnceLock<AppSession> = OnceLock::new();
static COMMITTED: AtomicBool = AtomicBool::new(false);

pub fn install_trial_session(
    control: File,
    status: File,
    startup: PortableStartupV3,
) -> Result<()> {
    SESSION
        .set(AppSession {
            control: Mutex::new(BufReader::new(control)),
            status: Mutex::new(status),
            exchange: Mutex::new(()),
            startup,
        })
        .map_err(|_| {
            PortableRuntimeError::new(
                "portable_activation",
                "portable App session was already installed",
            )
        })
}

pub fn is_committed() -> bool {
    COMMITTED.load(Ordering::Acquire)
}

/// The UI calls this only after its compiled bundle has rendered a real visible
/// window. The method checks the authenticated paths and query-only catalog
/// before it emits TrialReady, then the supervisor alone controls Activation
/// and Commit permits.
pub fn prove_visible_and_commit<T>(
    app: &AppHandle,
    commit: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let session = session()?;
    let _exchange = session.exchange.lock().map_err(|_| {
        PortableRuntimeError::new("portable_activation", "portable protocol exchange poisoned")
    })?;
    let schema_observed = query_only_schema()?;
    let paths = runtime_paths::current()?;
    std::fs::create_dir_all(&paths.webview2_root)?;
    let window = app.get_webview_window("main").ok_or_else(|| {
        PortableRuntimeError::new("portable_activation", "main WebView window was absent")
    })?;
    if !window
        .is_visible()
        .map_err(|error| PortableRuntimeError::new("portable_activation", error.to_string()))?
    {
        return Err(PortableRuntimeError::new(
            "portable_activation",
            "main WebView window was not visible",
        ));
    }
    window
        .eval("window.__renderpilotPortableTrialReady = true;")
        .map_err(|error| PortableRuntimeError::new("portable_activation", error.to_string()))?;
    {
        let mut status = session.status.lock().map_err(|_| {
            PortableRuntimeError::new("portable_activation", "status pipe poisoned")
        })?;
        write_message(
            &mut *status,
            &AppStatusMessage::TrialReady {
                transcript_sha256: session.startup.transcript_sha256()?,
                runtime_paths_sha256: session.startup.runtime_paths_sha256()?,
                schema_observed,
                db_query_only: true,
                webview_profile_ready: true,
                ui_bundle_ready: true,
                visible_window_ready: true,
                event_loop_roundtrip: true,
                supervisor_session_transcript_sha256: session
                    .startup
                    .supervisor_session_transcript_sha256
                    .clone(),
            },
        )?;
    }
    let permit = read_control(session)?;
    let (
        activation_nonce,
        selection_record_sha256,
        journal_sequence,
        supervisor_session_transcript_sha256,
    ) = match permit {
        AppControlMessage::ActivationPermit {
            activation_nonce,
            selection_record_sha256,
            journal_sequence,
            supervisor_session_transcript_sha256,
        } => (
            activation_nonce,
            selection_record_sha256,
            journal_sequence,
            supervisor_session_transcript_sha256,
        ),
        _ => {
            return Err(PortableRuntimeError::new(
                "portable_activation",
                "expected ActivationPermit",
            ));
        }
    };
    if supervisor_session_transcript_sha256 != session.startup.supervisor_session_transcript_sha256
    {
        return Err(PortableRuntimeError::new(
            "portable_activation",
            "ActivationPermit did not carry the startup binding transcript",
        ));
    }
    {
        let mut status = session.status.lock().map_err(|_| {
            PortableRuntimeError::new("portable_activation", "status pipe poisoned")
        })?;
        write_message(
            &mut *status,
            &AppStatusMessage::ActivationAck {
                activation_nonce,
                selection_record_sha256: selection_record_sha256.clone(),
                visible_window_ready: true,
                event_loop_roundtrip: true,
                supervisor_session_transcript_sha256: session
                    .startup
                    .supervisor_session_transcript_sha256
                    .clone(),
            },
        )?;
    }
    let expected_committed_journal_sequence = committed_sequence_for_selection(journal_sequence)?;
    let permit = read_control(session)?;
    let (committed_journal_sequence, permit_nonce) = match permit {
        AppControlMessage::CommitPermit {
            selection_record_sha256: selected,
            committed_journal_sequence,
            permit_nonce,
            supervisor_session_transcript_sha256,
        } if selected == selection_record_sha256
            && committed_journal_sequence == expected_committed_journal_sequence
            && permit_nonce == session.startup.commit_permit_nonce
            && supervisor_session_transcript_sha256
                == session.startup.supervisor_session_transcript_sha256 =>
        {
            (committed_journal_sequence, permit_nonce)
        }
        _ => {
            return Err(PortableRuntimeError::new(
                "portable_activation",
                "CommitPermit did not match the authenticated activation",
            ));
        }
    };

    // The permit authorizes durable App initialization, but observation is
    // truthful only after that initialization succeeds. Ordinary commands
    // remain gated until the exact Context is installed and the ack is flushed.
    let committed = commit()?;
    let mut status = session
        .status
        .lock()
        .map_err(|_| PortableRuntimeError::new("portable_activation", "status pipe poisoned"))?;
    write_message(
        &mut *status,
        &AppStatusMessage::CommitAck {
            selection_record_sha256,
            committed_journal_sequence,
            permit_nonce,
            supervisor_session_transcript_sha256: session
                .startup
                .supervisor_session_transcript_sha256
                .clone(),
        },
    )?;
    COMMITTED.store(true, Ordering::Release);
    Ok(committed)
}

/// A committed App can request one serialized supervisor-owned updater
/// operation at a time. The App owns only this authenticated DTO round-trip;
/// all network, staging, selection, journaling, and process replacement remain
/// in the supervisor.
pub fn request_update(
    request_id: &str,
    request: PortableUpdateRequest,
) -> Result<PortableUpdateResponse> {
    require_committed()?;
    let session = session()?;
    let _exchange = session.exchange.lock().map_err(|_| {
        PortableRuntimeError::new("portable_activation", "portable protocol exchange poisoned")
    })?;
    {
        let mut status = session.status.lock().map_err(|_| {
            PortableRuntimeError::new("portable_activation", "status pipe poisoned")
        })?;
        write_message(
            &mut *status,
            &AppStatusMessage::UpdateRequest {
                request_id: request_id.to_owned(),
                request,
            },
        )?;
    }
    match read_control(session)? {
        AppControlMessage::UpdateResponse {
            request_id: received,
            response,
        } if received == request_id => Ok(response),
        _ => Err(PortableRuntimeError::new(
            "portable_update_protocol",
            "supervisor update response did not match request",
        )),
    }
}

pub fn require_committed() -> Result<()> {
    if is_committed() || SESSION.get().is_none() {
        Ok(())
    } else {
        Err(PortableRuntimeError::new(
            "portable_activation_closed",
            "portable App is still TrialReadOnly",
        ))
    }
}

fn session() -> Result<&'static AppSession> {
    SESSION
        .get()
        .ok_or_else(|| PortableRuntimeError::new("portable_activation", "no portable App session"))
}

fn read_control(session: &AppSession) -> Result<AppControlMessage> {
    let mut control = session
        .control
        .lock()
        .map_err(|_| PortableRuntimeError::new("portable_activation", "control pipe poisoned"))?;
    read_message(&mut *control)
}

fn query_only_schema() -> Result<u32> {
    let paths = runtime_paths::current()?;
    if !paths.catalog_db_path.exists() {
        return Ok(0);
    }
    let connection = rusqlite::Connection::open_with_flags(
        &paths.catalog_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))?;
    connection
        .execute_batch("PRAGMA query_only = ON;")
        .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))?;
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| PortableRuntimeError::new("portable_trial_db", error.to_string()))
}
