use std::{
    fs::File,
    io::{BufRead, BufReader, Write},
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use tauri::{AppHandle, Manager};

use super::{
    app_catalog_migration::{
        CatalogClassification, classify_catalog, execute_generation_migration,
    },
    app_protocol::{
        ActivationPermit, AppControlMessage, AppStatusMessage, CommitPermit, PortableAppSessionV2,
        TrialReady, committed_sequence_for_selection, read_message, write_message,
    },
    error::{PortableRuntimeError, Result},
    runtime_paths,
};

pub(super) mod update_exchange;

pub use update_exchange::{PortableDownloadEvent, request_download_update, request_update};

pub(super) struct AppSession {
    control: Mutex<BufReader<File>>,
    status: Mutex<File>,
    exchange: Mutex<()>,
    exchange_fenced: AtomicBool,
    startup: PortableAppSessionV2,
}

impl AppSession {
    pub(super) fn new(control: File, status: File, startup: PortableAppSessionV2) -> Self {
        Self {
            control: Mutex::new(BufReader::new(control)),
            status: Mutex::new(status),
            exchange: Mutex::new(()),
            exchange_fenced: AtomicBool::new(false),
            startup,
        }
    }
}

static SESSION: OnceLock<AppSession> = OnceLock::new();
static COMMITTED: AtomicBool = AtomicBool::new(false);

pub fn install_trial_session(
    control: File,
    status: File,
    startup: PortableAppSessionV2,
) -> Result<()> {
    SESSION
        .set(AppSession::new(control, status, startup))
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
pub fn prove_visible_and_commit(
    app: &AppHandle,
    commit: impl FnOnce(CatalogClassification) -> Result<()>,
) -> Result<()> {
    let session = session()?;
    ensure_committed(&session.exchange, &COMMITTED, || {
        let catalog = query_only_catalog()?;
        let schema_observed = catalog.schema_observed();
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
                &AppStatusMessage::trial_ready(TrialReady {
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
                }),
            )?;
        }
        if let CatalogClassification::Existing { schema } = catalog {
            let mut control = session.control.lock().map_err(|_| {
                PortableRuntimeError::new("portable_activation", "control pipe poisoned")
            })?;
            let mut status = session.status.lock().map_err(|_| {
                PortableRuntimeError::new("portable_activation", "status pipe poisoned")
            })?;
            exchange_catalog_migration(
                &session.startup,
                &paths.catalog_db_path,
                schema,
                &mut *control,
                &mut *status,
            )?;
        }

        let activation_permit = accept_activation_permit(read_control(session)?, &session.startup)?;
        let activation_nonce = activation_permit.activation_nonce;
        let selection_record_sha256 = activation_permit.selection_record_sha256;
        let journal_sequence = activation_permit.journal_sequence;
        {
            let mut status = session.status.lock().map_err(|_| {
                PortableRuntimeError::new("portable_activation", "status pipe poisoned")
            })?;
            write_message(
                &mut *status,
                &AppStatusMessage::activation_ack(
                    activation_nonce,
                    selection_record_sha256.clone(),
                    true,
                    true,
                    session.startup.supervisor_session_transcript_sha256.clone(),
                ),
            )?;
        }
        let commit_permit = accept_commit_permit(
            read_control(session)?,
            &selection_record_sha256,
            journal_sequence,
            &session.startup,
        )?;
        let committed_journal_sequence = commit_permit.committed_journal_sequence;
        let permit_nonce = commit_permit.permit_nonce;

        // The permit authorizes durable App initialization, but observation is
        // truthful only after that initialization succeeds. Ordinary commands
        // remain gated until the exact Context is installed and the ack is flushed.
        commit(catalog)?;
        let mut status = session.status.lock().map_err(|_| {
            PortableRuntimeError::new("portable_activation", "status pipe poisoned")
        })?;
        write_message(
            &mut *status,
            &AppStatusMessage::commit_ack(
                selection_record_sha256,
                committed_journal_sequence,
                permit_nonce,
                session.startup.supervisor_session_transcript_sha256.clone(),
            ),
        )?;
        Ok(())
    })
}

pub(in crate::portable_runtime) fn ensure_committed(
    exchange: &Mutex<()>,
    committed_gate: &AtomicBool,
    perform_handshake: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let _exchange = exchange.lock().map_err(|_| {
        PortableRuntimeError::new("portable_activation", "portable protocol exchange poisoned")
    })?;
    // Another invocation may have completed activation while this one
    // was waiting for the protocol exchange lock. Treat that as a
    // successful idempotent no-op instead of starting a second handshake.
    if committed_gate.load(Ordering::Acquire) {
        return Ok(());
    }
    perform_handshake()?;
    committed_gate.store(true, Ordering::Release);
    Ok(())
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

pub(super) fn accept_activation_permit(
    message: AppControlMessage,
    startup: &PortableAppSessionV2,
) -> Result<ActivationPermit> {
    match message {
        AppControlMessage::ActivationPermit(permit)
            if permit.supervisor_session_transcript_sha256
                == startup.supervisor_session_transcript_sha256 =>
        {
            Ok(permit)
        }
        AppControlMessage::ActivationPermit(_) => Err(PortableRuntimeError::new(
            "portable_activation",
            "ActivationPermit did not carry the startup binding transcript",
        )),
        _ => Err(PortableRuntimeError::new(
            "portable_activation",
            "expected ActivationPermit",
        )),
    }
}

pub(super) fn accept_commit_permit(
    message: AppControlMessage,
    selection_record_sha256: &str,
    selection_journal_sequence: u64,
    startup: &PortableAppSessionV2,
) -> Result<CommitPermit> {
    let expected_committed_journal_sequence =
        committed_sequence_for_selection(selection_journal_sequence)?;
    match message {
        AppControlMessage::CommitPermit(permit)
            if permit.selection_record_sha256 == selection_record_sha256
                && permit.committed_journal_sequence == expected_committed_journal_sequence
                && permit.permit_nonce == startup.commit_permit_nonce
                && permit.supervisor_session_transcript_sha256
                    == startup.supervisor_session_transcript_sha256 =>
        {
            Ok(permit)
        }
        _ => Err(PortableRuntimeError::new(
            "portable_activation",
            "CommitPermit did not match the authenticated activation",
        )),
    }
}

pub(super) fn exchange_catalog_migration(
    startup: &PortableAppSessionV2,
    catalog: &Path,
    schema_observed: u32,
    control: &mut impl BufRead,
    status: &mut impl Write,
) -> Result<()> {
    let (
        operation,
        source_schema,
        target_schema,
        permit_nonce,
        supervisor_session_transcript_sha256,
    ) = match read_message(control)? {
        AppControlMessage::MigrationPermit(permit) => (
            permit.operation,
            permit.source_schema,
            permit.target_schema,
            permit.permit_nonce,
            permit.supervisor_session_transcript_sha256,
        ),
        _ => {
            return Err(PortableRuntimeError::new(
                "portable_migration_contract",
                "expected MigrationPermit for an existing catalog",
            ));
        }
    };
    if source_schema != schema_observed
        || target_schema != startup.maximum_schema
        || permit_nonce != startup.migration_permit_nonce
        || supervisor_session_transcript_sha256 != startup.supervisor_session_transcript_sha256
    {
        return Err(PortableRuntimeError::new(
            "portable_migration_contract",
            "MigrationPermit did not match the authenticated trial",
        ));
    }
    let snapshot_receipt_sha256 = operation.snapshot_receipt_sha256().map(str::to_owned);
    let report = execute_generation_migration(catalog, source_schema, target_schema, &operation)?;
    write_message(
        status,
        &AppStatusMessage::migration_ack(
            report,
            snapshot_receipt_sha256,
            permit_nonce,
            startup.supervisor_session_transcript_sha256.clone(),
        ),
    )
}

fn query_only_catalog() -> Result<CatalogClassification> {
    let paths = runtime_paths::current()?;
    classify_catalog(&paths.catalog_db_path)
}
