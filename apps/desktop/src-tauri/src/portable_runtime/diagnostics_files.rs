//! Portable diagnostics file adapter.
//!
//! This module contains no Win32 bindings or unsafe code.  It borrows typed
//! root/object capabilities and feeds only concrete portable profile values to
//! the bounded writer.

use std::sync::{Mutex, Once, OnceLock};

use crate::diagnostic_event::BackendDiagnosticEvent;
use crate::diagnostics::{
    DiagnosticCloseStatus, DiagnosticEmitStatus, PortableDiagnosticWriter, PortableFailureClass,
    PortableFailureSite, PortableMilestone, PortableRole, Sha256Id, first_event_matches,
};

use super::{
    app_protocol::PortableAppSessionV1,
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
}

enum SessionIdentity {
    Supervisor {
        session: Sha256Id,
    },
    App {
        session: Sha256Id,
        transaction: Sha256Id,
    },
}

impl SessionIdentity {
    const fn role(&self) -> PortableRole {
        match self {
            Self::Supervisor { .. } => PortableRole::Supervisor,
            Self::App { .. } => PortableRole::App,
        }
    }

    const fn session(&self) -> &Sha256Id {
        match self {
            Self::Supervisor { session } | Self::App { session, .. } => session,
        }
    }

    const fn transaction(&self) -> Option<&Sha256Id> {
        match self {
            Self::Supervisor { .. } => None,
            Self::App { transaction, .. } => Some(transaction),
        }
    }

    fn canonical_file_name(&self) -> String {
        match self.transaction() {
            None => format!("{}{LOG_SUFFIX}", self.session().as_str()),
            Some(transaction) => format!(
                "{}-{}{LOG_SUFFIX}",
                self.session().as_str(),
                transaction.as_str()
            ),
        }
    }

    fn into_writer(self, file: std::fs::File) -> Option<PortableDiagnosticWriter> {
        match self {
            Self::Supervisor { session } => PortableDiagnosticWriter::supervisor(file, session),
            Self::App {
                session,
                transaction,
            } => PortableDiagnosticWriter::app(file, session, transaction),
        }
    }
}

impl PortableDiagnosticSession {
    pub(super) fn milestone(&mut self, milestone: PortableMilestone) -> DiagnosticEmitStatus {
        self.writer
            .as_mut()
            .map_or(DiagnosticEmitStatus::Disabled, |writer| {
                writer.milestone(milestone)
            })
    }

    pub(super) fn failure(
        &mut self,
        site: PortableFailureSite,
        class: PortableFailureClass,
    ) -> DiagnosticEmitStatus {
        self.writer
            .as_mut()
            .map_or(DiagnosticEmitStatus::Disabled, |writer| {
                writer.failure(site, class)
            })
    }

    pub(super) fn backend(&mut self, event: BackendDiagnosticEvent) -> DiagnosticEmitStatus {
        self.writer
            .as_mut()
            .map_or(DiagnosticEmitStatus::Disabled, |writer| {
                writer.backend(event)
            })
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
            report_diagnostics_failure();
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
    open_session(root, SessionIdentity::Supervisor { session })
}

/// Opens the App observer only from the atomically authenticated runtime root.
pub(super) fn open_app(startup: &PortableAppSessionV1) -> Result<PortableDiagnosticSession> {
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
    let name = identity.canonical_file_name();
    let active_name = canonical_diagnostic_name(&role_directory, &name)?;
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
    };
    // The only active leaf is explicitly skipped while still consuming its
    // directory budget. Any other canonical uncertainty stops retention.
    if retain_completed(
        &result.role_directory,
        result.role,
        Some(result.active_name.as_str()),
    )
    .is_err()
    {
        report_diagnostics_failure();
    }
    Ok(result)
}

/// Retention commits only after `visit_diagnostic_entries` reaches clean
/// STATUS_NO_MORE_FILES.  Every uncertainty drops retained candidates and
/// returns before any exact-object deletion is attempted.
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
            return (entry.is_directory && !entry.is_reparse)
                .then_some(())
                .ok_or_else(|| retention_uncertain("native pseudoentry metadata was invalid"));
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
        if !first_event_matches(
            &first,
            role,
            &identity.session,
            identity.transaction.as_deref(),
        ) {
            return Err(retention_uncertain(
                "canonical diagnostic did not have its matching first record",
            ));
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
    let delete_count = candidates
        .len()
        .saturating_sub(retained_candidate_count(candidates.len()));
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
        (self.entries <= MAX_DIRECTORY_ENTRIES && self.directory_bytes <= MAX_DIRECTORY_BYTES)
            .then_some(())
            .ok_or_else(|| retention_uncertain("directory enumeration budget was exhausted"))
    }

    fn charge_canonical_attempt(&mut self) -> Result<()> {
        self.canonical_attempts = self.canonical_attempts.saturating_add(1);
        self.canonical_bytes = self
            .canonical_bytes
            .saturating_add(CANONICAL_ATTEMPT_RESERVE);
        (self.canonical_attempts <= MAX_CANONICAL_ATTEMPTS
            && self.canonical_bytes <= MAX_CANONICAL_BYTES)
            .then_some(())
            .ok_or_else(|| retention_uncertain("canonical classification budget was exhausted"))
    }
}

fn retained_candidate_count(candidates: usize) -> usize {
    candidates.min(MAX_COMPLETED_FILES)
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
    session: String,
    transaction: Option<String>,
}

fn parse_canonical_filename(role: PortableRole, name: &str) -> Option<FileIdentity> {
    let stem = name.strip_suffix(LOG_SUFFIX)?;
    match role {
        PortableRole::Supervisor if is_canonical_hex_64(stem) => Some(FileIdentity {
            session: stem.to_owned(),
            transaction: None,
        }),
        PortableRole::App => {
            let (session, transaction) = stem.split_once('-')?;
            (is_canonical_hex_64(session) && is_canonical_hex_64(transaction)).then(|| {
                FileIdentity {
                    session: session.to_owned(),
                    transaction: Some(transaction.to_owned()),
                }
            })
        }
        PortableRole::Supervisor => None,
    }
}

fn is_canonical_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
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

/// Sink faults are reduced to one fixed safe stderr line; no unsafe detail is
/// persisted into diagnostics or sent to the generic writer.
pub(super) fn report_diagnostics_failure() {
    DIAGNOSTICS_STDERR_ONCE.call_once(|| eprintln!("RenderPilot: portable_diagnostics_disabled"));
}

pub(super) fn report_emit_failure(status: DiagnosticEmitStatus) {
    if matches!(status, DiagnosticEmitStatus::Disabled) {
        report_diagnostics_failure();
    }
}

pub(crate) fn install_app(startup: &PortableAppSessionV1) {
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
    use crate::diagnostics::{PortableFailureClass, PortableRole};

    use super::{
        AppEmitTransition, FileIdentity, RetentionBudget, app_emit_transition, failure_class,
        parse_canonical_filename, retained_candidate_count,
    };
    use crate::diagnostics::DiagnosticEmitStatus;
    use crate::portable_runtime::error::PortableRuntimeError;

    #[test]
    fn canonical_filename_parser_rejects_foreign_and_wrong_role_names() {
        let session = "a".repeat(64);
        let transaction = "b".repeat(64);
        assert_eq!(
            parse_canonical_filename(PortableRole::Supervisor, &format!("{session}.log")),
            Some(FileIdentity {
                session: session.clone(),
                transaction: None,
            })
        );
        assert_eq!(
            parse_canonical_filename(PortableRole::App, &format!("{session}-{transaction}.log")),
            Some(FileIdentity {
                session,
                transaction: Some(transaction),
            })
        );
        assert!(parse_canonical_filename(PortableRole::App, "untrusted.log").is_none());
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

        assert_eq!(retained_candidate_count(17), 8);
        assert_eq!(retained_candidate_count(18), 8);
        assert_eq!(retained_candidate_count(64), 8);
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
            "canonical diagnostic did not have its matching first record",
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
