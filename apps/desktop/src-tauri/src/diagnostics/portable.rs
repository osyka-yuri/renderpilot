use std::fs::File;

use serde::{Deserialize, Serialize};

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
    },
}

impl PortableContext {
    fn supervisor(session: Sha256Id) -> Option<Self> {
        Some(Self::Supervisor(Self::identity(session)?))
    }

    fn app(session: Sha256Id, transaction: Sha256Id) -> Option<Self> {
        Some(Self::App {
            identity: Self::identity(session)?,
            transaction,
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
}

#[derive(Clone, Copy, Debug)]
enum PortableEvent {
    First,
    Milestone(PortableMilestone),
    Failure(PortableFailureSite, PortableFailureClass),
    Capacity,
    Backend(BackendDiagnosticEvent),
}

struct PortableProfile;

impl super::writer::sealed::Sealed for PortableProfile {}

impl SealedProfile for PortableProfile {
    type Context = PortableContext;
    type Event = PortableEvent;

    fn encode(
        metadata: WriterMetadata,
        context: &Self::Context,
        event: Self::Event,
    ) -> Option<Vec<u8>> {
        let (level, phase, code, operation) = match event {
            PortableEvent::First => match context.role() {
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
            PortableEvent::Milestone(milestone) => {
                let (phase, code) = milestone.record();
                (PortableLevel::Info, phase, code, None)
            }
            PortableEvent::Failure(site, class) => {
                (PortableLevel::Error, site.phase(), class.code(), None)
            }
            PortableEvent::Capacity => (
                PortableLevel::Info,
                PortablePhase::DiagnosticsCapacity,
                PortableCode::DiagnosticsCapacity,
                None,
            ),
            PortableEvent::Backend(event) => {
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
        };
        serde_json::to_vec(&PortableRecord::new(
            metadata, context, level, phase, code, operation,
        ))
        .ok()
    }

    fn encode_capacity(metadata: WriterMetadata, context: &Self::Context) -> Option<Vec<u8>> {
        Self::encode(metadata, context, PortableEvent::Capacity)
    }
}

#[derive(Debug, Serialize)]
struct PortableRecord<'a> {
    schema: &'static str,
    version: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    unix_ms: Option<u64>,
    seq: u64,
    role: PortableRole,
    app_version: &'a PackageVersion,
    session: &'a Sha256Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    transaction: Option<&'a Sha256Id>,
    level: PortableLevel,
    phase: &'static str,
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<&'static str>,
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
            seq: metadata.sequence,
            role: context.role(),
            app_version: &identity.app_version,
            session: &identity.session,
            transaction: context.transaction(),
            level,
            phase: phase_code(phase),
            code: portable_code(code),
            operation,
        }
    }

    fn backend(
        metadata: WriterMetadata,
        context: &'a PortableContext,
        level: PortableLevel,
        record: crate::diagnostic_event::BackendDiagnosticRecord,
    ) -> Self {
        let identity = context.identity_ref();
        Self {
            schema: DIAGNOSTIC_SCHEMA,
            version: DIAGNOSTIC_SCHEMA_VERSION,
            unix_ms: metadata.unix_ms,
            seq: metadata.sequence,
            role: context.role(),
            app_version: &identity.app_version,
            session: &identity.session,
            transaction: context.transaction(),
            level,
            phase: record.phase(),
            code: record.code(),
            operation: record.operation(),
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

    pub(crate) fn app(file: File, session: Sha256Id, transaction: Sha256Id) -> Option<Self> {
        Self::new(file, PortableContext::app(session, transaction)?)
    }

    fn new(file: File, context: PortableContext) -> Option<Self> {
        let role = context.role();
        let mut result = Self {
            role,
            inner: DiagnosticWriter::open(file, context),
        };
        matches!(
            result.inner.emit(PortableEvent::First),
            DiagnosticEmitStatus::Written
        )
        .then_some(result)
    }

    pub(crate) fn milestone(&mut self, milestone: PortableMilestone) -> DiagnosticEmitStatus {
        self.inner.emit(PortableEvent::Milestone(milestone))
    }

    pub(crate) fn failure(
        &mut self,
        site: PortableFailureSite,
        class: PortableFailureClass,
    ) -> DiagnosticEmitStatus {
        self.inner.emit(PortableEvent::Failure(site, class))
    }

    pub(crate) fn backend(&mut self, event: BackendDiagnosticEvent) -> DiagnosticEmitStatus {
        matches!(self.role, PortableRole::App)
            .then(|| self.inner.emit(PortableEvent::Backend(event)))
            .unwrap_or(DiagnosticEmitStatus::Disabled)
    }

    pub(crate) fn close(&mut self) -> DiagnosticCloseStatus {
        self.inner.close()
    }
}

/// Strictly verifies the first line of a completed diagnostic file before it
/// can become a retention candidate.  Unknown fields and foreign role/identity
/// combinations remain retained, never deletion candidates.
pub(crate) fn first_event_matches(
    bytes: &[u8],
    role: PortableRole,
    session: &str,
    transaction: Option<&str>,
) -> bool {
    let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') else {
        return false;
    };
    let Ok(event) = serde_json::from_slice::<RetainedFirstEvent<'_>>(&bytes[..newline]) else {
        return false;
    };
    let (identity_matches, phase, code) = match (role, transaction) {
        (PortableRole::Supervisor, None) => (
            event.transaction.is_none(),
            phase_code(PortablePhase::AdmissionComplete),
            portable_code(PortableCode::AdmissionComplete),
        ),
        (PortableRole::App, Some(transaction)) => (
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
        && PackageVersion::parse(event.app_version).is_some()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedFirstEvent<'a> {
    schema: &'a str,
    version: u8,
    #[serde(default, rename = "unix_ms", deserialize_with = "present_unix_ms")]
    _unix_ms: Option<u64>,
    seq: u64,
    role: PortableRole,
    app_version: &'a str,
    #[serde(borrow)]
    session: &'a str,
    #[serde(default, borrow, deserialize_with = "present_transaction")]
    transaction: Option<&'a str>,
    level: PortableLevel,
    phase: &'a str,
    code: &'a str,
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

#[cfg(test)]
mod tests {
    use super::{
        DIAGNOSTIC_SCHEMA, DIAGNOSTIC_SCHEMA_VERSION, PackageVersion, PortableCode,
        PortableContext, PortableFailureClass, PortableFailureSite, PortableLevel,
        PortableMilestone, PortablePhase, PortableRecord, PortableRole, Sha256Id,
        first_event_matches,
    };
    use crate::command_error_contract::CommandErrorKind;
    use crate::diagnostic_event::{BackendDiagnosticEvent, CommandOperation};
    use crate::diagnostics::writer::WriterMetadata;

    #[test]
    fn supervisor_first_event_has_exact_v1_json_field_order() {
        let session = Sha256Id::parse(&"a".repeat(64)).expect("session");
        let context = PortableContext::supervisor(session.clone()).expect("supervisor context");
        let bytes = serde_json::to_vec(&PortableRecord::new(
            WriterMetadata {
                unix_ms: None,
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
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":1,\"role\":\"supervisor\",\"app_version\":\"{}\",\"session\":\"{}\",\"level\":\"info\",\"phase\":\"admission_complete\",\"code\":\"admission_complete\"}}",
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
    fn app_first_event_uses_the_same_v1_schema_and_preserves_fixed_order() {
        let session = Sha256Id::parse(&"a".repeat(64)).expect("session");
        let transaction = Sha256Id::parse(&"b".repeat(64)).expect("transaction");
        let app = PortableContext::app(session.clone(), transaction.clone()).expect("App context");
        let app_first = serde_json::to_vec(&PortableRecord::new(
            WriterMetadata {
                unix_ms: None,
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
                "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":{DIAGNOSTIC_SCHEMA_VERSION},\"seq\":1,\"role\":\"app\",\"app_version\":\"{}\",\"session\":\"{}\",\"transaction\":\"{}\",\"level\":\"info\",\"phase\":\"runtime_paths_authenticated\",\"code\":\"runtime_paths_authenticated\"}}",
                env!("CARGO_PKG_VERSION"),
                session.as_str(),
                transaction.as_str(),
            )
            .into_bytes()
        );
        assert!(first_event_matches(
            &[app_first, b"\n".to_vec()].concat(),
            PortableRole::App,
            app.identity_ref().session.as_str(),
            app.transaction().map(Sha256Id::as_str),
        ));

        let unknown = format!(
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":1,\"seq\":1,\"role\":\"app\",\"app_version\":\"1.9.0\",\"session\":\"{}\",\"transaction\":\"{}\",\"level\":\"info\",\"phase\":\"runtime_paths_authenticated\",\"code\":\"runtime_paths_authenticated\",\"foreign\":true}}\n",
            app.identity_ref().session.as_str(),
            app.transaction().expect("transaction").as_str(),
        );
        assert!(!first_event_matches(
            unknown.as_bytes(),
            PortableRole::App,
            app.identity_ref().session.as_str(),
            app.transaction().map(Sha256Id::as_str),
        ));

        let supervisor = PortableContext::supervisor(session).expect("supervisor context");
        let (milestone_phase, milestone_code) = PortableMilestone::RpuVerified.record();
        let milestone = serde_json::to_vec(&PortableRecord::new(
            WriterMetadata {
                unix_ms: None,
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
    fn app_backend_event_uses_v1_and_has_only_closed_identifiers() {
        let session = Sha256Id::parse(&"a".repeat(64)).expect("session");
        let transaction = Sha256Id::parse(&"b".repeat(64)).expect("transaction");
        let context = PortableContext::app(session.clone(), transaction).expect("App context");
        let bytes = serde_json::to_vec(&PortableRecord::backend(
            WriterMetadata {
                unix_ms: None,
                sequence: 2,
            },
            &context,
            PortableLevel::Error,
            BackendDiagnosticEvent::command_failure(
                CommandOperation::ClearGameCover,
                CommandErrorKind::StorageFailed,
            )
            .record(),
        ))
        .expect("serialize backend event");

        assert_eq!(
            bytes,
            format!(
                "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":1,\"seq\":2,\"role\":\"app\",\"app_version\":\"{}\",\"session\":\"{}\",\"transaction\":\"{}\",\"level\":\"error\",\"phase\":\"command\",\"code\":\"storage_failed\",\"operation\":\"clear_game_cover\"}}",
                env!("CARGO_PKG_VERSION"),
                session.as_str(),
                context.transaction().expect("transaction").as_str(),
            )
            .into_bytes()
        );
        assert!(!String::from_utf8_lossy(&bytes).contains("detail"));
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
        let future_app = format!(
            "{{\"schema\":\"{DIAGNOSTIC_SCHEMA}\",\"version\":2,\"seq\":1,\"role\":\"app\",\"app_version\":\"1.9.0\",\"session\":\"{session}\",\"transaction\":\"{transaction}\",\"level\":\"info\",\"phase\":\"runtime_paths_authenticated\",\"code\":\"runtime_paths_authenticated\"}}\n"
        );
        assert!(!first_event_matches(
            future_app.as_bytes(),
            PortableRole::App,
            &session,
            Some(&transaction),
        ));
        assert!(Sha256Id::parse(&"G".repeat(64)).is_none());
        assert!(Sha256Id::parse(&"A".repeat(64)).is_none());
        assert!(PackageVersion::parse("invalid space").is_none());
    }
}
