use std::fs::File;

use serde::{Deserialize, Serialize};

use super::name::{display_id_matches, is_valid_timestamp_utc};
use super::projection::{DiagnosticLevel, FrontendDiagnosticEvent, RustLogEvent};
use super::writer::{
    DiagnosticCloseStatus, DiagnosticEmitStatus, DiagnosticWriter, SealedProfile, WriterMetadata,
};
use crate::diagnostic_event::{BackendDiagnosticEvent, BackendDiagnosticLevel};

const DIAGNOSTIC_SCHEMA: &str = "renderpilot.portable.diagnostics";
const DIAGNOSTIC_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortableRole {
    Supervisor,
    App,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PortableLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortablePhase {
    AdmissionComplete,
    RpuVerify,
    Recovery,
    GenerationSelect,
    ActivationStart,
    ActivationReady,
    ActivationMigration,
    ActivationCommit,
    UpdateService,
    ControlledExit,
    RuntimePathsAuthenticated,
    WebviewRuntime,
    DesktopShell,
    DiagnosticsCapacity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortableCode {
    AdmissionComplete,
    RpuVerified,
    RecoveryComplete,
    GenerationSelected,
    ActivationStart,
    ActivationReady,
    ActivationMigration,
    ActivationCommitted,
    UpdateServiceStarted,
    ControlledExit,
    RuntimePathsAuthenticated,
    WebviewRuntimeReady,
    DesktopShellReady,
    DiagnosticsCapacity,
    Io,
    Contract,
    Integrity,
    Authority,
    Concurrency,
    Process,
    Storage,
    Update,
    RuntimeFailure,
}

/// A single safe lifecycle success/state event.  Callers cannot combine an
/// arbitrary phase and code pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PortableMilestone {
    RpuVerified,
    RecoveryComplete,
    GenerationSelected,
    ActivationStarted,
    ActivationReady,
    ActivationMigration,
    ActivationCommitted,
    UpdateServiceStarted,
    ControlledExit,
    WebviewRuntimeReady,
    DesktopShellReady,
}

impl PortableMilestone {
    fn record(self) -> (PortablePhase, PortableCode) {
        match self {
            Self::RpuVerified => (PortablePhase::RpuVerify, PortableCode::RpuVerified),
            Self::RecoveryComplete => (PortablePhase::Recovery, PortableCode::RecoveryComplete),
            Self::GenerationSelected => (
                PortablePhase::GenerationSelect,
                PortableCode::GenerationSelected,
            ),
            Self::ActivationStarted => (
                PortablePhase::ActivationStart,
                PortableCode::ActivationStart,
            ),
            Self::ActivationReady => (
                PortablePhase::ActivationReady,
                PortableCode::ActivationReady,
            ),
            Self::ActivationMigration => (
                PortablePhase::ActivationMigration,
                PortableCode::ActivationMigration,
            ),
            Self::ActivationCommitted => (
                PortablePhase::ActivationCommit,
                PortableCode::ActivationCommitted,
            ),
            Self::UpdateServiceStarted => (
                PortablePhase::UpdateService,
                PortableCode::UpdateServiceStarted,
            ),
            Self::ControlledExit => (PortablePhase::ControlledExit, PortableCode::ControlledExit),
            Self::WebviewRuntimeReady => (
                PortablePhase::WebviewRuntime,
                PortableCode::WebviewRuntimeReady,
            ),
            Self::DesktopShellReady => {
                (PortablePhase::DesktopShell, PortableCode::DesktopShellReady)
            }
        }
    }
}

/// Safe, fixed failure locations.  Details remain exclusively in console
/// logging and are never serialized into the durable diagnostic profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PortableFailureSite {
    RpuVerify,
    Recovery,
    GenerationSelect,
    ActivationStart,
    ActivationReady,
    ActivationMigration,
    ActivationCommit,
    UpdateService,
    ControlledExit,
    WebviewRuntime,
    DesktopShell,
}

impl PortableFailureSite {
    fn phase(self) -> PortablePhase {
        match self {
            Self::RpuVerify => PortablePhase::RpuVerify,
            Self::Recovery => PortablePhase::Recovery,
            Self::GenerationSelect => PortablePhase::GenerationSelect,
            Self::ActivationStart => PortablePhase::ActivationStart,
            Self::ActivationReady => PortablePhase::ActivationReady,
            Self::ActivationMigration => PortablePhase::ActivationMigration,
            Self::ActivationCommit => PortablePhase::ActivationCommit,
            Self::UpdateService => PortablePhase::UpdateService,
            Self::ControlledExit => PortablePhase::ControlledExit,
            Self::WebviewRuntime => PortablePhase::WebviewRuntime,
            Self::DesktopShell => PortablePhase::DesktopShell,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PortableFailureClass {
    Io,
    Contract,
    Integrity,
    Authority,
    Concurrency,
    Process,
    Storage,
    Update,
    RuntimeFailure,
}

impl PortableFailureClass {
    fn code(self) -> PortableCode {
        match self {
            Self::Io => PortableCode::Io,
            Self::Contract => PortableCode::Contract,
            Self::Integrity => PortableCode::Integrity,
            Self::Authority => PortableCode::Authority,
            Self::Concurrency => PortableCode::Concurrency,
            Self::Process => PortableCode::Process,
            Self::Storage => PortableCode::Storage,
            Self::Update => PortableCode::Update,
            Self::RuntimeFailure => PortableCode::RuntimeFailure,
        }
    }
}

/// Canonical SHA-256 identity admitted to the portable diagnostic schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct Sha256Id(String);

impl Sha256Id {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        (value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')))
        .then(|| Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct PackageVersion(String);

impl PackageVersion {
    fn package(value: &'static str) -> Option<Self> {
        Self::parse(value)
    }

    fn parse(value: &str) -> Option<Self> {
        (!value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-')))
        .then(|| Self(value.to_owned()))
    }
}

#[derive(Debug)]
struct PortableIdentity {
    app_version: PackageVersion,
    session: Sha256Id,
}

#[derive(Debug)]
enum PortableContext {
    Supervisor(PortableIdentity),
    App {
        identity: PortableIdentity,
        transaction: Sha256Id,
        segment: u32,
    },
}

impl PortableContext {
    fn supervisor(session: Sha256Id) -> Option<Self> {
        Some(Self::Supervisor(Self::identity(session)?))
    }

    fn app(session: Sha256Id, transaction: Sha256Id, segment: u32) -> Option<Self> {
        Some(Self::App {
            identity: Self::identity(session)?,
            transaction,
            segment,
        })
    }

    fn identity(session: Sha256Id) -> Option<PortableIdentity> {
        let app_version = PackageVersion::package(env!("CARGO_PKG_VERSION"))?;
        Some(PortableIdentity {
            app_version,
            session,
        })
    }

    const fn role(&self) -> PortableRole {
        match self {
            Self::Supervisor(_) => PortableRole::Supervisor,
            Self::App { .. } => PortableRole::App,
        }
    }

    const fn identity_ref(&self) -> &PortableIdentity {
        match self {
            Self::Supervisor(identity) | Self::App { identity, .. } => identity,
        }
    }

    const fn transaction(&self) -> Option<&Sha256Id> {
        match self {
            Self::Supervisor(_) => None,
            Self::App { transaction, .. } => Some(transaction),
        }
    }

    const fn segment(&self) -> Option<u32> {
        match self {
            Self::Supervisor(_) => None,
            Self::App { segment, .. } => Some(*segment),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum PortableSessionEvent {
    First,
    Milestone(PortableMilestone),
    Failure(PortableFailureSite, PortableFailureClass),
    Capacity,
    Backend(BackendDiagnosticEvent),
    RustLog(RustLogEvent),
    Frontend(FrontendDiagnosticEvent),
}

struct PortableProfile;

impl super::writer::sealed::Sealed for PortableProfile {}

impl SealedProfile for PortableProfile {
    type Context = PortableContext;
    type Event = PortableSessionEvent;

    fn encode(
        metadata: WriterMetadata,
        context: &Self::Context,
        event: &Self::Event,
    ) -> Option<Vec<u8>> {
        let (level, phase, code, operation) = match event {
            PortableSessionEvent::First => match context.role() {
                PortableRole::Supervisor => (
                    PortableLevel::Info,
                    PortablePhase::AdmissionComplete,
                    PortableCode::AdmissionComplete,
                    None,
                ),
                PortableRole::App => (
                    PortableLevel::Info,
                    PortablePhase::RuntimePathsAuthenticated,
                    PortableCode::RuntimePathsAuthenticated,
                    None,
                ),
            },
            PortableSessionEvent::Milestone(milestone) => {
                let (phase, code) = milestone.record();
                (PortableLevel::Info, phase, code, None)
            }
            PortableSessionEvent::Failure(site, class) => {
                (PortableLevel::Error, site.phase(), class.code(), None)
            }
            PortableSessionEvent::Capacity => (
                PortableLevel::Info,
                PortablePhase::DiagnosticsCapacity,
                PortableCode::DiagnosticsCapacity,
                None,
            ),
            PortableSessionEvent::Backend(event) => {
                if !matches!(context, PortableContext::App { .. }) {
                    return None;
                }
                let record = event.record();
                let level = match record.level() {
                    BackendDiagnosticLevel::Warning => PortableLevel::Warning,
                    BackendDiagnosticLevel::Error => PortableLevel::Error,
                };
                return serde_json::to_vec(&PortableRecord::backend(
                    metadata, context, level, record,
                ))
                .ok();
            }
            PortableSessionEvent::RustLog(event) => {
                if !matches!(context, PortableContext::App { .. }) {
                    return None;
                }
                return serde_json::to_vec(&PortableRecord::rust_log(metadata, context, event))
                    .ok();
            }
            PortableSessionEvent::Frontend(event) => {
                if !matches!(context, PortableContext::App { .. }) {
                    return None;
                }
                return serde_json::to_vec(&PortableRecord::frontend(metadata, context, event))
                    .ok();
            }
        };
        serde_json::to_vec(&PortableRecord::new(
            metadata, context, level, phase, code, operation,
        ))
        .ok()
    }

    fn encode_capacity(metadata: WriterMetadata, context: &Self::Context) -> Option<Vec<u8>> {
        Self::encode(metadata, context, &PortableSessionEvent::Capacity)
    }
}

#[derive(Debug, Serialize)]
struct PortableRecord<'a> {
    schema: &'static str,
    version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    unix_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp_utc: Option<String>,
    seq: u64,
    role: PortableRole,
    app_version: &'a PackageVersion,
    session: &'a Sha256Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    transaction: Option<&'a Sha256Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    segment: Option<u32>,
    level: PortableLevel,
    phase: &'static str,
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    site_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locale: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_module: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_line: Option<u32>,
}

impl<'a> PortableRecord<'a> {
    fn new(
        metadata: WriterMetadata,
        context: &'a PortableContext,
        level: PortableLevel,
        phase: PortablePhase,
        code: PortableCode,
        operation: Option<&'static str>,
    ) -> Self {
        let identity = context.identity_ref();
        Self {
            schema: DIAGNOSTIC_SCHEMA,
            version: DIAGNOSTIC_SCHEMA_VERSION,
            unix_ms: metadata.unix_ms,
            timestamp_utc: metadata.timestamp_utc,
            seq: metadata.sequence,
            role: context.role(),
            app_version: &identity.app_version,
            session: &identity.session,
            transaction: context.transaction(),
            segment: context.segment(),
            level,
            phase: phase_code(phase),
            code: portable_code(code),
            operation,
            category: None,
            site_id: None,
            locale: None,
            mode: None,
            reason_code: None,
            path: None,
            source_module: None,
            source_line: None,
        }
    }

    fn backend(
        metadata: WriterMetadata,
        context: &'a PortableContext,
        level: PortableLevel,
        record: crate::diagnostic_event::BackendDiagnosticRecord<'a>,
    ) -> Self {
        let identity = context.identity_ref();
        Self {
            schema: DIAGNOSTIC_SCHEMA,
            version: DIAGNOSTIC_SCHEMA_VERSION,
            unix_ms: metadata.unix_ms,
            timestamp_utc: metadata.timestamp_utc,
            seq: metadata.sequence,
            role: context.role(),
            app_version: &identity.app_version,
            session: &identity.session,
            transaction: context.transaction(),
            segment: context.segment(),
            level,
            phase: record.phase(),
            code: record.code(),
            operation: record.operation(),
            category: None,
            site_id: None,
            locale: None,
            mode: None,
            reason_code: record.reason_code(),
            path: record.path(),
            source_module: None,
            source_line: None,
        }
    }

    fn rust_log(
        metadata: WriterMetadata,
        context: &'a PortableContext,
        event: &'a RustLogEvent,
    ) -> Self {
        Self {
            schema: DIAGNOSTIC_SCHEMA,
            version: DIAGNOSTIC_SCHEMA_VERSION,
            unix_ms: metadata.unix_ms,
            timestamp_utc: metadata.timestamp_utc,
            seq: metadata.sequence,
            role: context.role(),
            app_version: &context.identity_ref().app_version,
            session: &context.identity_ref().session,
            transaction: context.transaction(),
            segment: context.segment(),
            level: match event.level {
                DiagnosticLevel::Warning => PortableLevel::Warning,
                DiagnosticLevel::Error => PortableLevel::Error,
            },
            phase: "rust_log",
            code: match event.level {
                DiagnosticLevel::Warning => "rust_warning",
                DiagnosticLevel::Error => "rust_error",
            },
            operation: None,
            category: Some(event.category.code()),
            site_id: Some(event.site_id_hex()),
            locale: None,
            mode: None,
            reason_code: None,
            path: event.path.as_deref(),
            source_module: Some(event.source_module),
            source_line: Some(event.source_line),
        }
    }

    fn frontend(
        metadata: WriterMetadata,
        context: &'a PortableContext,
        event: &FrontendDiagnosticEvent,
    ) -> Self {
        Self {
            schema: DIAGNOSTIC_SCHEMA,
            version: DIAGNOSTIC_SCHEMA_VERSION,
            unix_ms: metadata.unix_ms,
            timestamp_utc: metadata.timestamp_utc,
            seq: metadata.sequence,
            role: context.role(),
            app_version: &context.identity_ref().app_version,
            session: &context.identity_ref().session,
            transaction: context.transaction(),
            segment: context.segment(),
            level: match event.level() {
                DiagnosticLevel::Warning => PortableLevel::Warning,
                DiagnosticLevel::Error => PortableLevel::Error,
            },
            phase: event.phase_code(),
            code: event.event_code(),
            operation: event.operation(),
            category: None,
            site_id: None,
            locale: event.locale(),
            mode: event.mode(),
            reason_code: None,
            path: None,
            source_module: None,
            source_line: None,
        }
    }
}

fn phase_code(phase: PortablePhase) -> &'static str {
    match phase {
        PortablePhase::AdmissionComplete => "admission_complete",
        PortablePhase::RpuVerify => "rpu_verify",
        PortablePhase::Recovery => "recovery",
        PortablePhase::GenerationSelect => "generation_select",
        PortablePhase::ActivationStart => "activation_start",
        PortablePhase::ActivationReady => "activation_ready",
        PortablePhase::ActivationMigration => "activation_migration",
        PortablePhase::ActivationCommit => "activation_commit",
        PortablePhase::UpdateService => "update_service",
        PortablePhase::ControlledExit => "controlled_exit",
        PortablePhase::RuntimePathsAuthenticated => "runtime_paths_authenticated",
        PortablePhase::WebviewRuntime => "webview_runtime",
        PortablePhase::DesktopShell => "desktop_shell",
        PortablePhase::DiagnosticsCapacity => "diagnostics_capacity",
    }
}

fn portable_code(code: PortableCode) -> &'static str {
    match code {
        PortableCode::AdmissionComplete => "admission_complete",
        PortableCode::RpuVerified => "rpu_verified",
        PortableCode::RecoveryComplete => "recovery_complete",
        PortableCode::GenerationSelected => "generation_selected",
        PortableCode::ActivationStart => "activation_start",
        PortableCode::ActivationReady => "activation_ready",
        PortableCode::ActivationMigration => "activation_migration",
        PortableCode::ActivationCommitted => "activation_committed",
        PortableCode::UpdateServiceStarted => "update_service_started",
        PortableCode::ControlledExit => "controlled_exit",
        PortableCode::RuntimePathsAuthenticated => "runtime_paths_authenticated",
        PortableCode::WebviewRuntimeReady => "webview_runtime_ready",
        PortableCode::DesktopShellReady => "desktop_shell_ready",
        PortableCode::DiagnosticsCapacity => "diagnostics_capacity",
        PortableCode::Io => "io",
        PortableCode::Contract => "contract",
        PortableCode::Integrity => "integrity",
        PortableCode::Authority => "authority",
        PortableCode::Concurrency => "concurrency",
        PortableCode::Process => "process",
        PortableCode::Storage => "storage",
        PortableCode::Update => "update",
        PortableCode::RuntimeFailure => "runtime_failure",
    }
}

/// Concrete portable facade.  Its construction enforces the role/transaction
/// relation and emits the exact mandated first record before returning.
pub(crate) struct PortableDiagnosticWriter {
    role: PortableRole,
    inner: DiagnosticWriter<PortableProfile>,
}

impl PortableDiagnosticWriter {
    pub(crate) fn supervisor(file: File, session: Sha256Id) -> Option<Self> {
        Self::new(file, PortableContext::supervisor(session)?)
    }

    pub(crate) fn app(
        file: File,
        session: Sha256Id,
        transaction: Sha256Id,
        segment: u32,
    ) -> Option<Self> {
        Self::new(file, PortableContext::app(session, transaction, segment)?)
    }

    fn new(file: File, context: PortableContext) -> Option<Self> {
        let role = context.role();
        let mut result = Self {
            role,
            inner: DiagnosticWriter::open(file, context),
        };
        matches!(
            result.inner.emit(&PortableSessionEvent::First),
            DiagnosticEmitStatus::Written
        )
        .then_some(result)
    }

    pub(crate) fn emit(&mut self, event: &PortableSessionEvent) -> DiagnosticEmitStatus {
        match event {
            PortableSessionEvent::Milestone(_) | PortableSessionEvent::Failure(_, _) => {
                self.inner.emit(event)
            }
            PortableSessionEvent::Backend(_)
            | PortableSessionEvent::RustLog(_)
            | PortableSessionEvent::Frontend(_)
                if matches!(self.role, PortableRole::App) =>
            {
                self.inner.emit(event)
            }
            PortableSessionEvent::First | PortableSessionEvent::Capacity => {
                DiagnosticEmitStatus::Disabled
            }
            _ => DiagnosticEmitStatus::Disabled,
        }
    }

    pub(crate) fn close(&mut self) -> DiagnosticCloseStatus {
        self.inner.close()
    }
}

/// Strictly verifies the first line of a completed diagnostic file before it
/// can become a retention candidate.  Unknown fields and foreign role/identity
/// combinations remain retained, never deletion candidates.
#[cfg(test)]
pub(crate) fn first_event_matches(
    bytes: &[u8],
    role: PortableRole,
    session: &str,
    transaction: Option<&str>,
) -> bool {
    first_event_matches_segment(bytes, role, session, transaction, None)
}

#[cfg(test)]
fn first_event_matches_segment(
    bytes: &[u8],
    role: PortableRole,
    session: &str,
    transaction: Option<&str>,
    segment: Option<u32>,
) -> bool {
    let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') else {
        return false;
    };
    let Ok(event) = serde_json::from_slice::<RetainedFirstEvent<'_>>(&bytes[..newline]) else {
        return false;
    };
    first_event_matches_record(&event, role, session, transaction, segment)
}

/// Validates a retained header against its filename's compact display ID.
/// App filenames identify the transaction; supervisor filenames identify the session.
pub(crate) fn first_event_matches_display_id(
    bytes: &[u8],
    role: PortableRole,
    display_id: &str,
    segment: Option<u32>,
) -> bool {
    let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') else {
        return false;
    };
    let Ok(event) = serde_json::from_slice::<RetainedFirstEvent<'_>>(&bytes[..newline]) else {
        return false;
    };
    let (identity, transaction) = match role {
        PortableRole::Supervisor => (event.session, None),
        PortableRole::App => {
            let Some(transaction) = event.transaction else {
                return false;
            };
            (transaction, Some(transaction))
        }
    };
    if Sha256Id::parse(event.session).is_none() || !display_id_matches(display_id, identity, 64) {
        return false;
    }
    first_event_matches_record(&event, role, event.session, transaction, segment)
}

fn first_event_matches_record(
    event: &RetainedFirstEvent<'_>,
    role: PortableRole,
    session: &str,
    transaction: Option<&str>,
    segment: Option<u32>,
) -> bool {
    let (identity_matches, phase, code) = match (role, transaction, segment) {
        (PortableRole::Supervisor, None, None) => (
            event.transaction.is_none(),
            phase_code(PortablePhase::AdmissionComplete),
            portable_code(PortableCode::AdmissionComplete),
        ),
        (PortableRole::App, Some(transaction), Some(_)) => (
            event.transaction == Some(transaction),
            phase_code(PortablePhase::RuntimePathsAuthenticated),
            portable_code(PortableCode::RuntimePathsAuthenticated),
        ),
        _ => return false,
    };
    event.schema == DIAGNOSTIC_SCHEMA
        && event.version == DIAGNOSTIC_SCHEMA_VERSION
        && event.seq == 1
        && event.role == role
        && event.session == session
        && identity_matches
        && event.level == PortableLevel::Info
        && event.phase == phase
        && event.code == code
        && event.segment == segment
        && event.operation.is_none()
        && event.category.is_none()
        && event.site_id.is_none()
        && event.locale.is_none()
        && event.mode.is_none()
        && event.reason_code.is_none()
        && event.path.is_none()
        && event.source_module.is_none()
        && event.source_line.is_none()
        && event.timestamp_utc.is_none_or(is_valid_timestamp_utc)
        && PackageVersion::parse(event.app_version).is_some()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedFirstEvent<'a> {
    schema: &'a str,
    version: u8,
    #[serde(default, rename = "unix_ms", deserialize_with = "present_unix_ms")]
    _unix_ms: Option<u64>,
    #[serde(default, borrow, deserialize_with = "present_str")]
    timestamp_utc: Option<&'a str>,
    seq: u64,
    role: PortableRole,
    app_version: &'a str,
    #[serde(borrow)]
    session: &'a str,
    #[serde(default, borrow, deserialize_with = "present_transaction")]
    transaction: Option<&'a str>,
    #[serde(default, deserialize_with = "present_u32")]
    segment: Option<u32>,
    level: PortableLevel,
    phase: &'a str,
    code: &'a str,
    #[serde(default, borrow, deserialize_with = "present_str")]
    operation: Option<&'a str>,
    #[serde(default, borrow, deserialize_with = "present_str")]
    category: Option<&'a str>,
    #[serde(default, borrow, deserialize_with = "present_str")]
    site_id: Option<&'a str>,
    #[serde(default, borrow, deserialize_with = "present_str")]
    locale: Option<&'a str>,
    #[serde(default, borrow, deserialize_with = "present_str")]
    mode: Option<&'a str>,
    #[serde(default, borrow, deserialize_with = "present_str")]
    reason_code: Option<&'a str>,
    #[serde(default, borrow, deserialize_with = "present_str")]
    path: Option<&'a str>,
    #[serde(default, borrow, deserialize_with = "present_str")]
    source_module: Option<&'a str>,
    #[serde(default, deserialize_with = "present_u32")]
    source_line: Option<u32>,
}

fn present_unix_ms<'de, D>(deserializer: D) -> std::result::Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    u64::deserialize(deserializer).map(Some)
}

fn present_transaction<'de, D>(deserializer: D) -> std::result::Result<Option<&'de str>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <&'de str>::deserialize(deserializer).map(Some)
}

fn present_str<'de, D>(deserializer: D) -> std::result::Result<Option<&'de str>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <&'de str>::deserialize(deserializer).map(Some)
}

fn present_u32<'de, D>(deserializer: D) -> std::result::Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    u32::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::{
        DIAGNOSTIC_SCHEMA, DIAGNOSTIC_SCHEMA_VERSION, PackageVersion, PortableCode,
        PortableContext, PortableFailureClass, PortableFailureSite, PortableLevel,
        PortableMilestone, PortablePhase, PortableRecord, PortableRole, Sha256Id,
        first_event_matches, first_event_matches_display_id, first_event_matches_segment,
    };
    use crate::command_error_contract::CommandErrorKind;
    use crate::diagnostic_event::{BackendDiagnosticEvent, CommandOperation};
    use crate::diagnostics::{DiagnosticLevel, RustLogEvent, writer::WriterMetadata};

    #[test]
    fn supervisor_first_event_has_exact_v1_json_field_order() {
        let session = Sha256Id::parse(&"a".repeat(64)).expect("session");
        let context = PortableContext::supervisor(session.clone()).expect("supervisor context");
        let bytes = serde_json::to_vec(&PortableRecord::new(
            WriterMetadata {
                unix_ms: None,
                timestamp_utc: Some("1970-01-01T00:00:00.123Z".to_owned()),
                sequence: 1,
            },
            &context,
            PortableLevel::Info,
            PortablePhase::AdmissionComplete,
            PortableCode::AdmissionComplete,
            None,
        ))
        .expect("serialize v1");
        let expected = format!(
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"timestamp_utc\":\"1970-01-01T00:00:00.123Z\",\"seq\":1,\"role\":\"supervisor\",\"app_version\":\"{}\",\"session\":\"{}\",\"level\":\"info\",\"phase\":\"admission_complete\",\"code\":\"admission_complete\"}}",
            env!("CARGO_PKG_VERSION"),
            session.as_str()
        );
        assert_eq!(bytes.as_slice(), expected.as_bytes());
        assert!(first_event_matches(
            &[bytes, b"\n".to_vec()].concat(),
            PortableRole::Supervisor,
            session.as_str(),
            None
        ));
    }

    #[test]
    fn app_first_event_uses_segmented_v1_schema_and_preserves_fixed_order() {
        let session = Sha256Id::parse(&"a".repeat(64)).expect("session");
        let transaction = Sha256Id::parse(&"b".repeat(64)).expect("transaction");
        let app =
            PortableContext::app(session.clone(), transaction.clone(), 0).expect("App context");
        let app_first = serde_json::to_vec(&PortableRecord::new(
            WriterMetadata {
                unix_ms: None,
                timestamp_utc: None,
                sequence: 1,
            },
            &app,
            PortableLevel::Info,
            PortablePhase::RuntimePathsAuthenticated,
            PortableCode::RuntimePathsAuthenticated,
            None,
        ))
        .expect("serialize App first event");
        assert_eq!(
            app_first,
            format!(
                "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":1,\"role\":\"app\",\"app_version\":\"{}\",\"session\":\"{}\",\"transaction\":\"{}\",\"segment\":0,\"level\":\"info\",\"phase\":\"runtime_paths_authenticated\",\"code\":\"runtime_paths_authenticated\"}}",
                env!("CARGO_PKG_VERSION"),
                session.as_str(),
                transaction.as_str(),
            )
            .into_bytes()
        );
        let app_first_line = [app_first, b"\n".to_vec()].concat();
        assert!(first_event_matches_segment(
            &app_first_line,
            PortableRole::App,
            app.identity_ref().session.as_str(),
            app.transaction().map(Sha256Id::as_str),
            Some(0),
        ));
        assert!(first_event_matches_display_id(
            &app_first_line,
            PortableRole::App,
            &"b".repeat(16),
            Some(0),
        ));
        assert!(!first_event_matches_display_id(
            &app_first_line,
            PortableRole::App,
            &"a".repeat(16),
            Some(0),
        ));
        let unknown = format!(
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":1,\"role\":\"app\",\"app_version\":\"1.9.0\",\"session\":\"{}\",\"transaction\":\"{}\",\"segment\":0,\"level\":\"info\",\"phase\":\"runtime_paths_authenticated\",\"code\":\"runtime_paths_authenticated\",\"foreign\":true}}\n",
            app.identity_ref().session.as_str(),
            app.transaction().expect("transaction").as_str(),
        );
        assert!(!first_event_matches_segment(
            unknown.as_bytes(),
            PortableRole::App,
            app.identity_ref().session.as_str(),
            app.transaction().map(Sha256Id::as_str),
            Some(0),
        ));

        let supervisor = PortableContext::supervisor(session).expect("supervisor context");
        let (milestone_phase, milestone_code) = PortableMilestone::RpuVerified.record();
        let milestone = serde_json::to_vec(&PortableRecord::new(
            WriterMetadata {
                unix_ms: None,
                timestamp_utc: None,
                sequence: 2,
            },
            &supervisor,
            PortableLevel::Info,
            milestone_phase,
            milestone_code,
            None,
        ))
        .expect("serialize milestone");
        assert_eq!(
            milestone,
            format!(
                "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":2,\"role\":\"supervisor\",\"app_version\":\"{}\",\"session\":\"{}\",\"level\":\"info\",\"phase\":\"rpu_verify\",\"code\":\"rpu_verified\"}}",
                env!("CARGO_PKG_VERSION"),
                supervisor.identity_ref().session.as_str(),
            )
            .into_bytes()
        );

        let failure = serde_json::to_vec(&PortableRecord::new(
            WriterMetadata {
                unix_ms: None,
                timestamp_utc: None,
                sequence: 3,
            },
            &supervisor,
            PortableLevel::Error,
            PortableFailureSite::ActivationStart.phase(),
            PortableFailureClass::Integrity.code(),
            None,
        ))
        .expect("serialize failure");
        assert_eq!(
            failure,
            format!(
                "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":3,\"role\":\"supervisor\",\"app_version\":\"{}\",\"session\":\"{}\",\"level\":\"error\",\"phase\":\"activation_start\",\"code\":\"integrity\"}}",
                env!("CARGO_PKG_VERSION"),
                supervisor.identity_ref().session.as_str(),
            )
            .into_bytes()
        );

        let capacity = serde_json::to_vec(&PortableRecord::new(
            WriterMetadata {
                unix_ms: None,
                timestamp_utc: None,
                sequence: 4,
            },
            &supervisor,
            PortableLevel::Info,
            PortablePhase::DiagnosticsCapacity,
            PortableCode::DiagnosticsCapacity,
            None,
        ))
        .expect("serialize capacity marker");
        assert_eq!(
            capacity,
            format!(
                "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":4,\"role\":\"supervisor\",\"app_version\":\"{}\",\"session\":\"{}\",\"level\":\"info\",\"phase\":\"diagnostics_capacity\",\"code\":\"diagnostics_capacity\"}}",
                env!("CARGO_PKG_VERSION"),
                supervisor.identity_ref().session.as_str(),
            )
            .into_bytes()
        );
    }

    #[test]
    fn app_backend_event_uses_v1_segment_and_selected_game_path() {
        let session = Sha256Id::parse(&"a".repeat(64)).expect("session");
        let transaction = Sha256Id::parse(&"b".repeat(64)).expect("transaction");
        let context = PortableContext::app(session.clone(), transaction, 0).expect("App context");
        let bytes = serde_json::to_vec(&PortableRecord::backend(
            WriterMetadata {
                unix_ms: None,
                timestamp_utc: None,
                sequence: 2,
            },
            &context,
            PortableLevel::Error,
            BackendDiagnosticEvent::command_failure(
                CommandOperation::InspectGameInstall,
                CommandErrorKind::StaleInstallInspection,
                None,
                Some("D:/Games/Example".to_owned()),
            )
            .record(),
        ))
        .expect("serialize backend event");

        assert_eq!(
            bytes,
            format!(
                "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":2,\"role\":\"app\",\"app_version\":\"{}\",\"session\":\"{}\",\"transaction\":\"{}\",\"segment\":0,\"level\":\"error\",\"phase\":\"command\",\"code\":\"stale_install_inspection\",\"operation\":\"inspect_game_install\",\"path\":\"D:/Games/Example\"}}",
                env!("CARGO_PKG_VERSION"),
                session.as_str(),
                context.transaction().expect("transaction").as_str(),
            )
            .into_bytes()
        );
        assert!(!String::from_utf8_lossy(&bytes).contains("fingerprint"));
    }

    #[test]
    fn app_backend_event_serializes_only_allowlisted_reason_code() {
        let session = Sha256Id::parse(&"a".repeat(64)).expect("session");
        let transaction = Sha256Id::parse(&"b".repeat(64)).expect("transaction");
        let context = PortableContext::app(session, transaction, 0).expect("App context");
        let bytes = serde_json::to_vec(&PortableRecord::backend(
            WriterMetadata {
                unix_ms: None,
                timestamp_utc: None,
                sequence: 2,
            },
            &context,
            PortableLevel::Warning,
            BackendDiagnosticEvent::command_failure(
                CommandOperation::InspectGameInstall,
                CommandErrorKind::InvalidInstallRoot,
                Some("contains_proven_install"),
                None,
            )
            .record(),
        ))
        .expect("serialize backend event");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON record");
        assert_eq!(value["reason_code"], "contains_proven_install");
        assert!(value.get("path").is_none());
        assert!(!String::from_utf8_lossy(&bytes).contains("private"));
    }

    #[test]
    fn app_rust_log_serializes_static_source_and_allowlisted_path() {
        let session = Sha256Id::parse(&"a".repeat(64)).expect("session");
        let transaction = Sha256Id::parse(&"b".repeat(64)).expect("transaction");
        let context = PortableContext::app(session, transaction, 0).expect("App context");
        let mut event = RustLogEvent::from_site(
            tracing::Level::WARN,
            "renderpilot_orchestration::addons::luma::install::recovery",
            147,
        )
        .expect("first-party warning site");
        event.level = DiagnosticLevel::Warning;
        event.path = Some("D:/Games/Example/ReShade.ini".to_owned());
        let bytes = serde_json::to_vec(&PortableRecord::rust_log(
            WriterMetadata {
                unix_ms: None,
                timestamp_utc: None,
                sequence: 2,
            },
            &context,
            &event,
        ))
        .expect("serialize Rust event");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON record");
        assert_eq!(value["category"], "addon_lifecycle");
        assert_eq!(
            value["source_module"],
            "renderpilot_orchestration::addons::luma::install::recovery"
        );
        assert_eq!(value["source_line"], 147);
        assert_eq!(value["path"], "D:/Games/Example/ReShade.ini");
        assert!(value.get("message").is_none());
    }

    #[test]
    fn first_event_matcher_rejects_unknown_fields_wrong_role_and_bad_ids() {
        let session = "a".repeat(64);
        let record = format!(
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":1,\"seq\":1,\"role\":\"supervisor\",\"app_version\":\"1.9.0\",\"session\":\"{session}\",\"level\":\"info\",\"phase\":\"admission_complete\",\"code\":\"admission_complete\",\"foreign\":true}}\n"
        );
        assert!(!first_event_matches(
            record.as_bytes(),
            PortableRole::Supervisor,
            &session,
            None
        ));
        let null_transaction = format!(
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":1,\"seq\":1,\"role\":\"supervisor\",\"app_version\":\"1.9.0\",\"session\":\"{session}\",\"transaction\":null,\"level\":\"info\",\"phase\":\"admission_complete\",\"code\":\"admission_complete\"}}\n"
        );
        assert!(!first_event_matches(
            null_transaction.as_bytes(),
            PortableRole::Supervisor,
            &session,
            None,
        ));
        let first_operation = format!(
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":1,\"seq\":1,\"role\":\"supervisor\",\"app_version\":\"1.9.0\",\"session\":\"{session}\",\"level\":\"info\",\"phase\":\"admission_complete\",\"code\":\"admission_complete\",\"operation\":null}}\n"
        );
        assert!(!first_event_matches(
            first_operation.as_bytes(),
            PortableRole::Supervisor,
            &session,
            None,
        ));
        let transaction = "b".repeat(64);
        let unsegmented_app = format!(
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":1,\"role\":\"app\",\"app_version\":\"1.9.0\",\"session\":\"{session}\",\"transaction\":\"{transaction}\",\"level\":\"info\",\"phase\":\"runtime_paths_authenticated\",\"code\":\"runtime_paths_authenticated\"}}\n"
        );
        assert!(!first_event_matches_display_id(
            unsegmented_app.as_bytes(),
            PortableRole::App,
            &"b".repeat(16),
            Some(0),
        ));
        let app_first_with_path = format!(
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":1,\"role\":\"app\",\"app_version\":\"1.9.0\",\"session\":\"{session}\",\"transaction\":\"{transaction}\",\"segment\":0,\"level\":\"info\",\"phase\":\"runtime_paths_authenticated\",\"code\":\"runtime_paths_authenticated\",\"path\":\"D:/Games/Example\"}}\n"
        );
        assert!(!first_event_matches_segment(
            app_first_with_path.as_bytes(),
            PortableRole::App,
            &session,
            Some(&transaction),
            Some(0),
        ));
        assert!(Sha256Id::parse(&"G".repeat(64)).is_none());
        assert!(Sha256Id::parse(&"A".repeat(64)).is_none());
        assert!(PackageVersion::parse("invalid space").is_none());
    }
}
