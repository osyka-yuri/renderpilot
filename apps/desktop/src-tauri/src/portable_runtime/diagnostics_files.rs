//! Portable diagnostics file adapter.
//!
//! This module contains no Win32 bindings or unsafe code.  It borrows typed
//! root/object capabilities and feeds only concrete portable profile values to
//! the bounded writer.

use std::{
    io::Write,
    sync::{Mutex, Once, OnceLock},
    time::SystemTime,
};

use crate::diagnostic_event::BackendDiagnosticEvent;
use crate::diagnostics::{
    DiagnosticCloseStatus, DiagnosticEmitStatus, FrontendDiagnosticEvent, PortableDiagnosticName,
    PortableDiagnosticWriter, PortableFailureClass, PortableFailureSite, PortableMilestone,
    PortableRole, PortableSessionEvent, RustLogEvent, Sha256Id, canonical_filename_prefix,
    first_event_matches_display_id, format_filename_timestamp, parse_portable_diagnostic_name,
};

use super::{
    app_protocol::PortableAppSessionV2,
    error::{PortableRuntimeError, Result},
    root_authority::PortableRootAuthority,
    supervisor::authority::SupervisorSessionAuthority,
    win32::object::{
        CanonicalDiagnosticName, CompletedDiagnosticCandidate, DiagnosticDirectoryEntry,
        DiagnosticsRole, DiagnosticsRoleDirectory, canonical_diagnostic_name,
        create_active_diagnostic, open_completed_canonical_diagnostic,
        open_diagnostics_role_directory, visit_diagnostic_entries,
    },
};

const MAX_COMPLETED_FILES: usize = 8;
const MAX_DIRECTORY_ENTRIES: usize = 256;
const MAX_DIRECTORY_BYTES: usize = 128 * 1024;
const MAX_CANONICAL_ATTEMPTS: usize = 64;
const MAX_CANONICAL_BYTES: usize = 256 * 1024;
const CANONICAL_ATTEMPT_RESERVE: usize = 4 * 1024;
const LOG_SUFFIX: &str = ".log";

/// One bounded diagnostic observer and its borrowed retained root/role
/// authority.  Dropping the writer's active leaf before retention is explicit.
pub(super) struct PortableDiagnosticSession {
    writer: Option<PortableDiagnosticWriter>,
    _root: PortableRootAuthority,
    role_directory: DiagnosticsRoleDirectory,
    role: PortableRole,
    active_name: CanonicalDiagnosticName,
    app_identity: Option<(Sha256Id, Sha256Id, u32)>,
    start_timestamp: String,
}

enum SessionIdentity {
    Supervisor {
        session: Sha256Id,
        start_timestamp: String,
    },
    App {
        session: Sha256Id,
        transaction: Sha256Id,
        start_timestamp: String,
        segment: u32,
    },
}

impl SessionIdentity {
    const fn role(&self) -> PortableRole {
        match self {
            Self::Supervisor { .. } => PortableRole::Supervisor,
            Self::App { .. } => PortableRole::App,
        }
    }

    fn canonical_file_name(&self) -> Option<String> {
        match self {
            Self::Supervisor {
                session,
                start_timestamp,
            } => {
                let prefix = canonical_filename_prefix(start_timestamp, session.as_str(), 64)?;
                Some(format!("{prefix}{LOG_SUFFIX}"))
            }
            Self::App {
                transaction,
                start_timestamp,
                segment,
                ..
            } => {
                let prefix = canonical_filename_prefix(start_timestamp, transaction.as_str(), 64)?;
                Some(format!("{prefix}-s{segment:08x}{LOG_SUFFIX}"))
            }
        }
    }

    fn into_writer(self, file: std::fs::File) -> Option<PortableDiagnosticWriter> {
        match self {
            Self::Supervisor { session, .. } => PortableDiagnosticWriter::supervisor(file, session),
            Self::App {
                session,
                transaction,
                segment,
                ..
            } => PortableDiagnosticWriter::app(file, session, transaction, segment),
        }
    }
}

impl PortableDiagnosticSession {
    pub(super) fn milestone(&mut self, milestone: PortableMilestone) -> DiagnosticEmitStatus {
        self.emit(&PortableSessionEvent::Milestone(milestone))
    }

    pub(super) fn failure(
        &mut self,
        site: PortableFailureSite,
        class: PortableFailureClass,
    ) -> DiagnosticEmitStatus {
        self.emit(&PortableSessionEvent::Failure(site, class))
    }

    pub(super) fn backend(&mut self, event: BackendDiagnosticEvent) -> DiagnosticEmitStatus {
        self.emit(&PortableSessionEvent::Backend(event))
    }

    pub(super) fn rust_log(&mut self, event: RustLogEvent) -> DiagnosticEmitStatus {
        self.emit(&PortableSessionEvent::RustLog(event))
    }

    pub(super) fn frontend(&mut self, event: FrontendDiagnosticEvent) -> DiagnosticEmitStatus {
        self.emit(&PortableSessionEvent::Frontend(event))
    }

    fn emit(&mut self, event: &PortableSessionEvent) -> DiagnosticEmitStatus {
        let Some(writer) = self.writer.as_mut() else {
            return DiagnosticEmitStatus::Disabled;
        };
        let status = writer.emit(event);
        if status != DiagnosticEmitStatus::Capacity {
            return status;
        }
        if self.role != PortableRole::App {
            return DiagnosticEmitStatus::Sealed;
        }
        self.rollover(event)
    }

    fn rollover(&mut self, event: &PortableSessionEvent) -> DiagnosticEmitStatus {
        let Some(mut previous) = self.writer.take() else {
            return DiagnosticEmitStatus::Disabled;
        };
        if matches!(previous.close(), DiagnosticCloseStatus::Failed) {
            report_diagnostics_failure();
            return DiagnosticEmitStatus::Disabled;
        }
        drop(previous);
        if retain_completed(&self.role_directory, self.role, None).is_err() {
            report_retention_failure();
        }
        let Some((session, transaction, segment)) = self.app_identity.take() else {
            report_diagnostics_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let Some(next_segment) = segment.checked_add(1) else {
            report_diagnostics_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let identity = SessionIdentity::App {
            session: session.clone(),
            transaction: transaction.clone(),
            start_timestamp: self.start_timestamp.clone(),
            segment: next_segment,
        };
        let Some(file_name) = identity.canonical_file_name() else {
            report_diagnostics_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let Ok(active_name) = canonical_diagnostic_name(&self.role_directory, &file_name) else {
            report_diagnostics_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let Ok(file) = create_active_diagnostic(&self.role_directory, &active_name) else {
            report_diagnostics_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let Some(mut writer) = identity.into_writer(file) else {
            report_diagnostics_failure();
            return DiagnosticEmitStatus::Disabled;
        };
        let status = writer.emit(event);
        if status != DiagnosticEmitStatus::Written {
            let _ = writer.close();
            report_diagnostics_failure();
            return DiagnosticEmitStatus::Disabled;
        }
        self.active_name = active_name;
        self.app_identity = Some((session, transaction, next_segment));
        self.writer = Some(writer);
        DiagnosticEmitStatus::Written
    }

    /// Closes/syncs and drops the active leaf before exact-handle retention.
    /// Retention uncertainty is deliberately nonfatal and deletes nothing.
    pub(super) fn close(mut self) {
        if let Some(mut writer) = self.writer.take() {
            if matches!(writer.close(), DiagnosticCloseStatus::Failed) {
                report_diagnostics_failure();
            }
            drop(writer);
        }
        if retain_completed(&self.role_directory, self.role, None).is_err() {
            report_retention_failure();
        }
    }
}

/// Opens the supervisor observer after root-bound admission.  The transcript
/// is already a validated protocol SHA-256 and the writer emits its first
/// admission-complete record inside the concrete facade.
pub(super) fn open_supervisor(
    root: PortableRootAuthority,
    authority: &SupervisorSessionAuthority,
) -> Result<PortableDiagnosticSession> {
    let session = Sha256Id::parse(authority.transcript_sha256()).ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_diagnostics_open",
            "supervisor transcript was not a canonical diagnostic identity",
        )
    })?;
    open_session(
        root,
        SessionIdentity::Supervisor {
            session,
            start_timestamp: format_filename_timestamp(SystemTime::now()),
        },
    )
}

/// Opens the App observer only from the atomically authenticated runtime root.
pub(super) fn open_app(startup: &PortableAppSessionV2) -> Result<PortableDiagnosticSession> {
    let runtime = super::runtime_paths::current_runtime()?;
    let root = super::runtime_paths::current_root()?;
    if runtime.paths() != &startup.runtime_paths
        || root.identity().as_str() != startup.portable_root_identity
    {
        return Err(PortableRuntimeError::new(
            "portable_diagnostics_open",
            "authenticated App runtime differed from the startup binding",
        ));
    }
    let session =
        Sha256Id::parse(&startup.supervisor_session_transcript_sha256).ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_diagnostics_open",
                "startup transcript was not a canonical diagnostic identity",
            )
        })?;
    let transaction = Sha256Id::parse(&startup.transaction_id).ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_diagnostics_open",
            "startup transaction was not a canonical diagnostic identity",
        )
    })?;
    open_session(
        root.clone(),
        SessionIdentity::App {
            session,
            transaction,
            start_timestamp: format_filename_timestamp(SystemTime::now()),
            segment: 0,
        },
    )
}

fn open_session(
    root: PortableRootAuthority,
    identity: SessionIdentity,
) -> Result<PortableDiagnosticSession> {
    let role = identity.role();
    let role_directory = open_diagnostics_role_directory(
        root.object(),
        match role {
            PortableRole::Supervisor => DiagnosticsRole::Supervisor,
            PortableRole::App => DiagnosticsRole::App,
        },
    )?;
    let file_name = identity.canonical_file_name().ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_diagnostics_open",
            "portable diagnostic identity could not form a canonical filename",
        )
    })?;
    let active_name = canonical_diagnostic_name(&role_directory, &file_name)?;
    let app_identity = match &identity {
        SessionIdentity::App {
            session,
            transaction,
            segment,
            ..
        } => Some((session.clone(), transaction.clone(), *segment)),
        SessionIdentity::Supervisor { .. } => None,
    };
    let start_timestamp = match &identity {
        SessionIdentity::Supervisor {
            start_timestamp, ..
        }
        | SessionIdentity::App {
            start_timestamp, ..
        } => start_timestamp.clone(),
    };
    let file = create_active_diagnostic(&role_directory, &active_name)?;
    let writer = identity.into_writer(file).ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_diagnostics_open",
            "portable diagnostics profile could not emit its first record",
        )
    })?;
    let result = PortableDiagnosticSession {
        writer: Some(writer),
        _root: root,
        role_directory,
        role,
        active_name,
        app_identity,
        start_timestamp,
    };
    // The active leaf is skipped while still consuming its directory budget.
    // Other canonical leaves are retained only after their first records have
    // been verified; bounded but incomplete or mismatching leaves stay in place.
    // Retention is maintenance: failure is reported, but the active writer stays
    // open and usable. A later close or rollover makes another attempt.
    if retain_completed(
        &result.role_directory,
        result.role,
        Some(result.active_name.as_str()),
    )
    .is_err()
    {
        report_retention_failure();
    }
    Ok(result)
}

/// Classification commits only after `visit_diagnostic_entries` reaches clean
/// STATUS_NO_MORE_FILES. Enumeration, authority, and budget errors return
/// before deletion; an error during deletion can follow earlier successful
/// deletions. Bounded leaves with incomplete or mismatching first records are
/// simply left in place.
fn retain_completed(
    role_directory: &DiagnosticsRoleDirectory,
    role: PortableRole,
    active_name: Option<&str>,
) -> Result<()> {
    let mut budget = RetentionBudget::default();
    let mut candidates = Vec::<RetentionCandidate>::new();
    visit_diagnostic_entries(role_directory, |entry: DiagnosticDirectoryEntry| {
        budget.charge_directory_entry(entry.record_bytes)?;
        if entry.is_native_pseudoentry {
            if !entry.is_directory || entry.is_reparse {
                return Err(retention_uncertain(
                    "native pseudoentry metadata was invalid",
                ));
            }
            return Ok(());
        }
        if active_name == Some(entry.name.as_str()) {
            return Ok(());
        }
        let Some(identity) = parse_canonical_filename(role, &entry.name) else {
            return Ok(());
        };
        // Charge the fixed classification reserve before opening the leaf, so
        // busy, malformed, and otherwise unclassifiable canonical leaves
        // consume the same bounded budget as valid ones.
        budget.charge_canonical_attempt()?;
        let candidate_name = canonical_diagnostic_name(role_directory, &entry.name)?;
        let mut candidate = open_completed_canonical_diagnostic(role_directory, &candidate_name)?;
        let first = candidate.read_first_record()?;
        if !first_event_matches_display_id(&first, role, &identity.display_id, identity.segment) {
            // A process may have stopped after creating this canonical leaf
            // but before completing its first record. It is not ours to
            // delete, and it must not prevent a later healthy session.
            drop(candidate);
            return Ok(());
        }
        let modified = candidate.last_write()?;
        candidates.push(RetentionCandidate {
            name: entry.name,
            modified,
            candidate,
        });
        if candidates.len() > MAX_CANONICAL_ATTEMPTS {
            return Err(retention_uncertain(
                "retained candidate count exceeded capacity",
            ));
        }
        Ok(())
    })?;
    candidates.sort_by(|left, right| {
        left.modified
            .cmp(&right.modified)
            .then_with(|| left.name.cmp(&right.name))
    });
    let delete_count = candidates.len().saturating_sub(MAX_COMPLETED_FILES);
    for candidate in candidates.into_iter().take(delete_count) {
        candidate.candidate.delete_exact()?;
    }
    Ok(())
}

#[derive(Default)]
struct RetentionBudget {
    entries: usize,
    directory_bytes: usize,
    canonical_attempts: usize,
    canonical_bytes: usize,
}

impl RetentionBudget {
    fn charge_directory_entry(&mut self, record_bytes: usize) -> Result<()> {
        self.entries = self.entries.saturating_add(1);
        self.directory_bytes = self.directory_bytes.saturating_add(record_bytes);
        if self.entries > MAX_DIRECTORY_ENTRIES || self.directory_bytes > MAX_DIRECTORY_BYTES {
            return Err(retention_uncertain(
                "directory enumeration budget was exhausted",
            ));
        }
        Ok(())
    }

    fn charge_canonical_attempt(&mut self) -> Result<()> {
        self.canonical_attempts = self.canonical_attempts.saturating_add(1);
        self.canonical_bytes = self
            .canonical_bytes
            .saturating_add(CANONICAL_ATTEMPT_RESERVE);
        if self.canonical_attempts > MAX_CANONICAL_ATTEMPTS
            || self.canonical_bytes > MAX_CANONICAL_BYTES
        {
            return Err(retention_uncertain(
                "canonical classification budget was exhausted",
            ));
        }
        Ok(())
    }
}

fn retention_uncertain(message: &'static str) -> PortableRuntimeError {
    PortableRuntimeError::new("portable_diagnostics_retention", message)
}

#[derive(Debug)]
struct RetentionCandidate {
    name: String,
    modified: u64,
    candidate: CompletedDiagnosticCandidate,
}

#[derive(Debug, Eq, PartialEq)]
struct FileIdentity {
    display_id: String,
    segment: Option<u32>,
}

fn parse_canonical_filename(role: PortableRole, name: &str) -> Option<FileIdentity> {
    match (role, parse_portable_diagnostic_name(name)?) {
        (PortableRole::Supervisor, PortableDiagnosticName::Supervisor { display_id, .. }) => {
            Some(FileIdentity {
                display_id,
                segment: None,
            })
        }
        (
            PortableRole::App,
            PortableDiagnosticName::App {
                display_id,
                segment,
                ..
            },
        ) => Some(FileIdentity {
            display_id,
            segment: Some(segment),
        }),
        _ => None,
    }
}

pub(super) fn failure_class(error: &PortableRuntimeError) -> PortableFailureClass {
    match error.code() {
        "portable_runtime_io" => PortableFailureClass::Io,
        "portable_generation_contract"
        | "portable_protocol_sequence"
        | "portable_startup_invalid"
        | "portable_startup_paths"
        | "portable_migration_contract" => PortableFailureClass::Contract,
        code if code.starts_with("portable_namespace_")
            || matches!(
                code,
                "portable_stage_identity" | "portable_generation_receipt" | "portable_object"
            ) =>
        {
            PortableFailureClass::Integrity
        }
        "portable_supervisor_session" | "portable_root" | "portable_admission_handle" => {
            PortableFailureClass::Authority
        }
        "portable_process" | "portable_activation" | "portable_app_exit" => {
            PortableFailureClass::Process
        }
        code if code.starts_with("portable_catalog_")
            || code.starts_with("portable_migration_")
            || code == "portable_context" =>
        {
            PortableFailureClass::Storage
        }
        code if code.starts_with("portable_update_") || code.starts_with("portable_stage_") => {
            PortableFailureClass::Update
        }
        "portable_runtime_lock" => PortableFailureClass::Concurrency,
        _ => PortableFailureClass::RuntimeFailure,
    }
}

pub(super) fn report_failure(
    session: &mut Option<PortableDiagnosticSession>,
    site: PortableFailureSite,
    error: &PortableRuntimeError,
) {
    if let Some(session) = session.as_mut() {
        let status = session.failure(site, failure_class(error));
        report_emit_failure(status);
    }
}

enum AppDiagnosticObserver {
    Uninitialized,
    Active(PortableDiagnosticSession),
    /// A failed open has no retained session; a disabled active writer retains
    /// root/directory authority solely so shutdown can complete retention.
    Disabled(Option<PortableDiagnosticSession>),
    Closed,
}

static APP_DIAGNOSTICS: OnceLock<Mutex<AppDiagnosticObserver>> = OnceLock::new();
static DIAGNOSTICS_STDERR_ONCE: Once = Once::new();
static RETENTION_STDERR_ONCE: Once = Once::new();

/// Sink faults are reduced to one fixed safe stderr line; no unsafe detail is
/// persisted into diagnostics or sent to the generic writer.
pub(super) fn report_diagnostics_failure() {
    DIAGNOSTICS_STDERR_ONCE.call_once(|| eprintln!("RenderPilot: portable_diagnostics_disabled"));
}

fn report_retention_failure() {
    RETENTION_STDERR_ONCE.call_once(|| {
        let _ = std::io::stderr()
            .lock()
            .write_all(b"RenderPilot: portable_diagnostics_retention_failed\n");
    });
}

pub(super) fn report_emit_failure(status: DiagnosticEmitStatus) {
    if matches!(status, DiagnosticEmitStatus::Disabled) {
        report_diagnostics_failure();
    }
}

pub(crate) fn install_app(startup: &PortableAppSessionV2) {
    let state = APP_DIAGNOSTICS.get_or_init(|| Mutex::new(AppDiagnosticObserver::Uninitialized));
    let Ok(mut slot) = state.lock() else {
        report_diagnostics_failure();
        return;
    };
    if !matches!(&*slot, AppDiagnosticObserver::Uninitialized) {
        return;
    }
    match open_app(startup) {
        Ok(session) => *slot = AppDiagnosticObserver::Active(session),
        Err(_) => {
            *slot = AppDiagnosticObserver::Disabled(None);
            report_diagnostics_failure();
        }
    }
}

pub(crate) fn app_milestone(milestone: PortableMilestone) {
    emit_app(|session| session.milestone(milestone));
}

pub(crate) fn app_failure(site: PortableFailureSite, error: &PortableRuntimeError) {
    emit_app(|session| session.failure(site, failure_class(error)));
}

pub(crate) fn record_app_backend_event(event: BackendDiagnosticEvent) {
    emit_app(|session| session.backend(event));
}

pub(crate) fn record_app_log_event(event: RustLogEvent) {
    emit_app(|session| session.rust_log(event));
}

pub(crate) fn record_app_frontend_event(event: FrontendDiagnosticEvent) {
    emit_app(|session| session.frontend(event));
}

fn emit_app(emit: impl FnOnce(&mut PortableDiagnosticSession) -> DiagnosticEmitStatus) {
    let Some(state) = APP_DIAGNOSTICS.get() else {
        return;
    };
    let Ok(mut slot) = state.lock() else {
        report_diagnostics_failure();
        return;
    };
    let status = match &mut *slot {
        AppDiagnosticObserver::Active(session) => emit(session),
        AppDiagnosticObserver::Uninitialized
        | AppDiagnosticObserver::Disabled(_)
        | AppDiagnosticObserver::Closed => return,
    };
    let transition = app_emit_transition(status);
    match transition {
        AppEmitTransition::KeepActive => {}
        AppEmitTransition::DisableWithoutReport | AppEmitTransition::DisableAndReport => {
            let prior = std::mem::replace(&mut *slot, AppDiagnosticObserver::Disabled(None));
            if let AppDiagnosticObserver::Active(session) = prior {
                *slot = AppDiagnosticObserver::Disabled(Some(session));
            }
            if matches!(transition, AppEmitTransition::DisableAndReport) {
                report_diagnostics_failure();
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppEmitTransition {
    KeepActive,
    DisableWithoutReport,
    DisableAndReport,
}

const fn app_emit_transition(status: DiagnosticEmitStatus) -> AppEmitTransition {
    match status {
        DiagnosticEmitStatus::Written => AppEmitTransition::KeepActive,
        DiagnosticEmitStatus::Sealed => AppEmitTransition::DisableWithoutReport,
        DiagnosticEmitStatus::Capacity => AppEmitTransition::DisableAndReport,
        DiagnosticEmitStatus::Disabled => AppEmitTransition::DisableAndReport,
    }
}

pub(crate) fn shutdown_app() {
    let Some(state) = APP_DIAGNOSTICS.get() else {
        return;
    };
    let Ok(mut slot) = state.lock() else {
        return;
    };
    let observer = std::mem::replace(&mut *slot, AppDiagnosticObserver::Closed);
    drop(slot);
    match observer {
        AppDiagnosticObserver::Active(session) | AppDiagnosticObserver::Disabled(Some(session)) => {
            session.close()
        }
        AppDiagnosticObserver::Uninitialized
        | AppDiagnosticObserver::Disabled(None)
        | AppDiagnosticObserver::Closed => {}
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::diagnostics::{PortableFailureClass, PortableRole};

    use super::{
        AppEmitTransition, FileIdentity, MAX_CANONICAL_ATTEMPTS, RetentionBudget, SessionIdentity,
        app_emit_transition, failure_class, open_session, parse_canonical_filename,
        retain_completed,
    };
    use crate::diagnostics::{
        DiagnosticCloseStatus, DiagnosticEmitStatus, DiagnosticLevel, PortableDiagnosticWriter,
        PortableMilestone, RustLogEvent, Sha256Id, canonical_filename_prefix,
    };
    use crate::portable_runtime::error::PortableRuntimeError;
    use crate::portable_runtime::win32::object::{
        DiagnosticsRole, canonical_diagnostic_name, create_active_diagnostic,
        open_diagnostics_role_directory,
    };

    fn app_identity(session: char, transaction: char, segment: u32) -> SessionIdentity {
        SessionIdentity::App {
            session: Sha256Id::parse(&session.to_string().repeat(64)).expect("session identity"),
            transaction: Sha256Id::parse(&transaction.to_string().repeat(64))
                .expect("transaction identity"),
            start_timestamp: "2026-09-23_14-35-12Z".to_owned(),
            segment,
        }
    }

    fn test_app_tree() -> (
        tempfile::TempDir,
        super::super::root_authority::PortableRootAuthority,
        super::super::win32::object::DiagnosticsRoleDirectory,
    ) {
        let directory = tempfile::tempdir().expect("temporary portable data root");
        fs::create_dir_all(directory.path().join("data/logs/portable/app"))
            .expect("create fixture directories");
        let root = super::super::root_authority::PortableRootAuthority::open(directory.path())
            .expect("open retained fixture root");
        let role_directory = open_diagnostics_role_directory(root.object(), DiagnosticsRole::App)
            .expect("open app diagnostics directory");
        (directory, root, role_directory)
    }

    fn app_filename(_session: char, transaction: char, segment: u32) -> String {
        let transaction = transaction.to_string().repeat(64);
        let prefix = canonical_filename_prefix("2026-09-23_14-35-12Z", &transaction, 64)
            .expect("canonical filename prefix");
        format!("{prefix}-s{segment:08x}.log")
    }

    fn app_filename_for_index(index: u32, segment: u32) -> String {
        let transaction = format!("{index:016x}{}", "0".repeat(48));
        let prefix = canonical_filename_prefix("2026-09-23_14-35-12Z", &transaction, 64)
            .expect("canonical filename prefix");
        format!("{prefix}-s{segment:08x}.log")
    }

    fn write_app_record(
        role_directory: &super::super::win32::object::DiagnosticsRoleDirectory,
        file_session: char,
        file_transaction: char,
        file_segment: u32,
        record_session: char,
        record_transaction: char,
        record_segment: u32,
    ) -> String {
        let name = app_filename(file_session, file_transaction, file_segment);
        let canonical =
            canonical_diagnostic_name(role_directory, &name).expect("canonical fixture file name");
        let file = create_active_diagnostic(role_directory, &canonical)
            .expect("create fixture diagnostic");
        let mut writer = PortableDiagnosticWriter::app(
            file,
            Sha256Id::parse(&record_session.to_string().repeat(64)).expect("record session"),
            Sha256Id::parse(&record_transaction.to_string().repeat(64))
                .expect("record transaction"),
            record_segment,
        )
        .expect("write fixture first record");
        assert_eq!(writer.close(), DiagnosticCloseStatus::Synced);
        name
    }

    #[test]
    fn canonical_filename_parser_rejects_foreign_and_wrong_role_names() {
        let session = "a".repeat(64);
        let transaction = "b".repeat(64);
        let supervisor_name = format!("2026-09-23_14-35-12Z-{}.log", &session[..16]);
        assert_eq!(
            parse_canonical_filename(PortableRole::Supervisor, &supervisor_name),
            Some(FileIdentity {
                display_id: session[..16].to_owned(),
                segment: None,
            })
        );
        assert!(parse_canonical_filename(PortableRole::App, &supervisor_name).is_none());
        assert_eq!(
            parse_canonical_filename(
                PortableRole::App,
                &format!("2026-09-23_14-35-12Z-{}-s0000000f.log", &transaction[..16])
            ),
            Some(FileIdentity {
                display_id: transaction[..16].to_owned(),
                segment: Some(15)
            })
        );
        assert!(parse_canonical_filename(PortableRole::App, "untrusted.log").is_none());
        assert!(
            parse_canonical_filename(
                PortableRole::App,
                &format!("{session}-{transaction}-s00000000.log")
            )
            .is_none()
        );
    }

    #[test]
    fn portable_errors_map_only_to_closed_failure_classes() {
        assert_eq!(
            failure_class(&PortableRuntimeError::new("portable_runtime_io", "detail")),
            PortableFailureClass::Io
        );
        assert_eq!(
            failure_class(&PortableRuntimeError::new("portable_app_exit", "detail")),
            PortableFailureClass::Process
        );
        assert_eq!(
            failure_class(&PortableRuntimeError::new("unknown_future_code", "detail")),
            PortableFailureClass::RuntimeFailure
        );
    }

    #[test]
    fn retention_budget_and_target_are_fixed_before_any_handle_deletion() {
        let mut directory = RetentionBudget::default();
        for _ in 0..256 {
            directory
                .charge_directory_entry(512)
                .expect("256 entries / 128 KiB stays within the stream budget");
        }
        assert!(directory.charge_directory_entry(1).is_err());

        let mut canonical = RetentionBudget::default();
        for _ in 0..64 {
            canonical
                .charge_canonical_attempt()
                .expect("64 fixed reservations stay within the class budget");
        }
        assert!(canonical.charge_canonical_attempt().is_err());
    }

    #[test]
    fn portable_retention_keeps_eight_verified_and_leaves_unverified_canonical_files() {
        let (directory, _root, role_directory) = test_app_tree();
        let mut verified = Vec::new();
        for index in 0..9_u32 {
            let session = char::from_digit(index, 16).expect("fixture session");
            verified.push(write_app_record(
                &role_directory,
                session,
                'f',
                index,
                session,
                'f',
                index,
            ));
        }

        let empty = app_filename('e', 'd', 0);
        let partial = app_filename('f', 'd', 0);
        let bad_schema = app_filename('d', 'c', 0);
        let wrong_transaction = write_app_record(&role_directory, 'a', 'b', 0, 'a', 'c', 0);
        let wrong_segment = write_app_record(&role_directory, 'b', 'c', 9, 'b', 'c', 8);
        let app_directory = directory.path().join("data/logs/portable/app");
        fs::write(app_directory.join(&empty), b"").expect("seed empty crash orphan");
        let partial_bytes = b"{\"schema\":\"renderpilot.diagnostics\"";
        fs::write(app_directory.join(&partial), partial_bytes).expect("seed partial crash orphan");
        fs::write(
            app_directory.join(&bad_schema),
            b"{\"schema\":\"foreign\"}\n",
        )
        .expect("seed mismatching schema");
        let old_dev_name = format!("{}-{}-s00000000.log", "a".repeat(64), "b".repeat(64));
        fs::write(app_directory.join(&old_dev_name), b"old dev log\n")
            .expect("seed a prior development filename");

        retain_completed(&role_directory, PortableRole::App, None)
            .expect("ignore bounded unverified leaves and retain verified records");

        assert_eq!(
            verified
                .iter()
                .filter(|name| app_directory.join(name).exists())
                .count(),
            8
        );
        for name in [
            &empty,
            &partial,
            &bad_schema,
            &wrong_transaction,
            &wrong_segment,
        ] {
            assert!(
                app_directory.join(name.as_str()).exists(),
                "unverified file is preserved"
            );
        }
        assert_eq!(
            fs::read(app_directory.join(partial)).unwrap(),
            partial_bytes
        );
        assert_eq!(
            fs::read(app_directory.join(old_dev_name)).unwrap(),
            b"old dev log\n",
            "legacy development names remain outside retention"
        );
    }

    #[test]
    fn startup_and_rollover_continue_writing_when_retention_budget_is_exceeded() {
        let (directory, root, _role_directory) = test_app_tree();
        let app_directory = directory.path().join("data/logs/portable/app");
        let mut budget_orphans = Vec::new();
        for index in 0..=MAX_CANONICAL_ATTEMPTS as u32 {
            let name = app_filename_for_index(index, 1);
            fs::write(app_directory.join(&name), b"").expect("seed over-budget canonical leaf");
            budget_orphans.push(name);
        }
        let startup_orphan = app_filename('a', 'b', 0);
        let startup_bytes = b"{\"partial\":";
        fs::write(app_directory.join(&startup_orphan), startup_bytes).expect("seed startup orphan");

        let mut startup = open_session(root.clone(), app_identity('c', 'd', 0))
            .expect("startup keeps the active writer when retention exceeds its budget");
        let startup_name = startup.active_name.as_str().to_owned();
        assert_eq!(
            startup.milestone(PortableMilestone::DesktopShellReady),
            DiagnosticEmitStatus::Written
        );
        startup.close();
        let startup_contents = fs::read_to_string(app_directory.join(startup_name))
            .expect("startup segment remains readable");
        let startup_lines = startup_contents.lines().collect::<Vec<_>>();
        assert_eq!(
            startup_lines.len(),
            2,
            "first record and following milestone"
        );
        assert!(startup_lines[0].contains("runtime_paths_authenticated"));
        assert!(startup_lines[1].contains("desktop_shell_ready"));
        assert_eq!(
            fs::read(app_directory.join(startup_orphan)).unwrap(),
            startup_bytes
        );
        assert!(
            budget_orphans
                .iter()
                .all(|name| fs::read(app_directory.join(name)).unwrap().is_empty())
        );

        let rollover_orphan = app_filename('b', 'e', 0);
        let rollover_bytes = b"{\"partial\":\"rollover";
        fs::write(app_directory.join(&rollover_orphan), rollover_bytes)
            .expect("seed rollover orphan");
        let mut rollover =
            open_session(root, app_identity('a', 'c', 0)).expect("open healthy rollover fixture");
        let initial_name = rollover.active_name.as_str().to_owned();
        for _ in 0..20_000 {
            assert_eq!(
                rollover.milestone(PortableMilestone::DesktopShellReady),
                DiagnosticEmitStatus::Written
            );
            if rollover.active_name.as_str() != initial_name {
                break;
            }
        }
        assert_ne!(rollover.active_name.as_str(), initial_name);
        assert_eq!(rollover.active_name.as_str(), app_filename('a', 'c', 1));
        assert_eq!(
            initial_name.get(..20),
            rollover.active_name.as_str().get(..20),
            "rollover preserves the original UTC start timestamp"
        );
        let previous =
            fs::read_to_string(app_directory.join(&initial_name)).expect("read completed segment");
        assert_eq!(
            previous
                .matches("\"code\":\"diagnostics_capacity\"")
                .count(),
            1,
            "capacity marker appears once in the completed segment"
        );
        let next = fs::read_to_string(app_directory.join(rollover.active_name.as_str()))
            .expect("read next segment");
        let next_lines = next.lines().collect::<Vec<_>>();
        assert_eq!(
            next_lines.len(),
            2,
            "new segment header and one replayed trigger"
        );
        assert!(next_lines[0].contains("runtime_paths_authenticated"));
        assert!(next_lines[1].contains("desktop_shell_ready"));
        assert_eq!(
            fs::read(app_directory.join(rollover_orphan)).unwrap(),
            rollover_bytes
        );
        assert_eq!(
            rollover.milestone(PortableMilestone::DesktopShellReady),
            DiagnosticEmitStatus::Written
        );
        rollover.close();
        assert!(
            budget_orphans
                .iter()
                .all(|name| fs::read(app_directory.join(name)).unwrap().is_empty())
        );
    }

    #[test]
    fn app_rollover_replays_owned_structured_path_once() {
        let (directory, root, _) = test_app_tree();
        let mut session =
            open_session(root, app_identity('a', 'c', 0)).expect("open App diagnostics session");
        let initial_name = session.active_name.as_str().to_owned();
        let mut event = RustLogEvent::from_site(
            tracing::Level::WARN,
            "renderpilot_orchestration::addons::luma::install::recovery",
            147,
        )
        .expect("first-party warning callsite");
        event.path = Some("D:/Games/Example/ReShade.ini".to_owned());

        for _ in 0..20_000 {
            assert_eq!(
                session.rust_log(event.clone()),
                DiagnosticEmitStatus::Written
            );
            if session.active_name.as_str() != initial_name {
                break;
            }
        }

        assert_ne!(session.active_name.as_str(), initial_name);
        let next = fs::read_to_string(
            directory
                .path()
                .join("data/logs/portable/app")
                .join(session.active_name.as_str()),
        )
        .expect("read next App segment");
        let lines = next.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2, "header plus one replayed triggering event");
        assert!(lines[1].contains("\"seq\":2"));
        assert!(lines[1].contains("\"path\":\"D:/Games/Example/ReShade.ini\""));
        assert_eq!(lines[1].matches("\"path\"").count(), 1);
        assert_eq!(event.level, DiagnosticLevel::Warning);
        session.close();
    }

    #[test]
    fn portable_retention_io_failure_keeps_all_verified_files() {
        let (directory, _root, role_directory) = test_app_tree();
        let mut verified = Vec::new();
        for index in 0..9_u32 {
            let session = char::from_digit(index, 16).expect("fixture session");
            verified.push(write_app_record(
                &role_directory,
                session,
                'f',
                index,
                session,
                'f',
                index,
            ));
        }
        let app_directory = directory.path().join("data/logs/portable/app");
        let invalid_type = app_filename('a', 'e', 0);
        fs::create_dir(app_directory.join(&invalid_type)).expect("seed invalid canonical type");

        assert!(retain_completed(&role_directory, PortableRole::App, None).is_err());
        assert_eq!(
            verified
                .iter()
                .filter(|name| app_directory.join(name).exists())
                .count(),
            9
        );
        assert!(app_directory.join(invalid_type).is_dir());
    }

    #[test]
    fn canonical_attempt_budget_failure_precedes_every_deletion() {
        let (directory, _root, role_directory) = test_app_tree();
        let mut verified = Vec::new();
        for index in 0..9_u32 {
            let session = char::from_digit(index, 16).expect("fixture session");
            verified.push(write_app_record(
                &role_directory,
                session,
                'f',
                index,
                session,
                'f',
                index,
            ));
        }

        let app_directory = directory.path().join("data/logs/portable/app");
        for index in 0..65_u32 {
            let name = app_filename_for_index(index, 1);
            fs::write(app_directory.join(name), b"").expect("seed canonical orphan");
        }

        assert!(retain_completed(&role_directory, PortableRole::App, None).is_err());
        assert_eq!(
            verified
                .iter()
                .filter(|name| app_directory.join(name).exists())
                .count(),
            9,
            "verified candidates are not removed before classification completes"
        );
    }

    #[test]
    fn app_observer_transitions_seal_without_a_sink_failure_report() {
        assert_eq!(
            app_emit_transition(DiagnosticEmitStatus::Written),
            AppEmitTransition::KeepActive
        );
        assert_eq!(
            app_emit_transition(DiagnosticEmitStatus::Sealed),
            AppEmitTransition::DisableWithoutReport
        );
        assert_eq!(
            app_emit_transition(DiagnosticEmitStatus::Capacity),
            AppEmitTransition::DisableAndReport
        );
        assert_eq!(
            app_emit_transition(DiagnosticEmitStatus::Disabled),
            AppEmitTransition::DisableAndReport
        );
    }

    #[test]
    fn retention_and_rights_contracts_remain_bounded_and_handle_only() {
        let source = include_str!("diagnostics_files.rs");
        for required in [
            "MAX_COMPLETED_FILES: usize = 8",
            "MAX_DIRECTORY_ENTRIES: usize = 256",
            "MAX_DIRECTORY_BYTES: usize = 128 * 1024",
            "MAX_CANONICAL_ATTEMPTS: usize = 64",
            "CANONICAL_ATTEMPT_RESERVE: usize = 4 * 1024",
            "visit_diagnostic_entries",
            "open_completed_canonical_diagnostic",
            "delete_exact",
            "writer.close();",
            "drop(writer);",
            "Some(result.active_name.as_str())",
            "budget.charge_directory_entry(entry.record_bytes)?",
            "budget.charge_canonical_attempt()?",
            "candidate.delete_exact()?",
            "drop(candidate);",
            "unverified canonical diagnostics are left untouched",
        ] {
            assert!(
                source.contains(required),
                "missing retention contract: {required}"
            );
        }
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert!(!production.contains("windows_sys"));
        assert!(!production.contains("unsafe {"));

        let stream = include_str!("win32/object/directory_stream.rs");
        assert!(stream.contains("if code != 0 || status.Information > bytes.len()"));
        assert!(stream.contains("!is_native_pseudoentry(&name)"));

        let enumerate = production
            .find("visit_diagnostic_entries(role_directory")
            .expect("clean enumeration");
        let delete = production
            .find("candidate.delete_exact()?")
            .expect("exact delete");
        assert!(
            enumerate < delete,
            "retention never deletes before clean EOD"
        );

        let charge = production
            .find("budget.charge_directory_entry(entry.record_bytes)?")
            .expect("directory charge");
        let pseudo = production
            .find("if entry.is_native_pseudoentry")
            .expect("pseudoentry admission");
        assert!(charge < pseudo, "native dot entries remain budgeted");
    }
}
