//! Stable, presentation-free error contract for the desktop IPC boundary.
//!
//! The frontend owns localized messages, severity, and suggested actions. Rust
//! serializes only a machine code and explicitly allowlisted structured data.

mod mapping;

use serde::Serialize;

pub(crate) use crate::command_error_contract::{CommandErrorKind, CommandErrorSeverity};
use crate::{backend_diagnostics, diagnostic_event::CommandOperation};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: &'static str,

    #[serde(skip)]
    kind: CommandErrorKind,

    /// Stable validation reason for typed errors such as `invalid_install_root`.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<&'static str>,

    /// Published recovery artifact retained after fail-closed managed cleanup.
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_bundle_path: Option<String>,

    /// Backend-only diagnostic context. Never crosses the IPC boundary.
    #[serde(skip)]
    diagnostic: Option<CommandErrorDiagnostic>,

    /// Guards boundary helpers against accidental duplicate registration.
    #[serde(skip)]
    diagnostic_recorded: bool,
}

#[derive(Debug, Clone)]
struct CommandErrorDiagnostic {
    detail: String,
}

#[derive(Clone, Copy)]
struct CommandDiagnosticRecord<'error> {
    severity: CommandErrorSeverity,
    operation: CommandOperation,
    code: &'static str,
    detail: Option<&'error str>,
}

fn write_command_diagnostic(record: CommandDiagnosticRecord<'_>) {
    match (record.severity, record.detail) {
        (CommandErrorSeverity::Warning, Some(detail)) => log::warn!(
            "Desktop command warning [operation={} code={}]: {detail}",
            record.operation.code(),
            record.code
        ),
        (CommandErrorSeverity::Warning, None) => log::warn!(
            "Desktop command warning [operation={} code={}]",
            record.operation.code(),
            record.code
        ),
        (CommandErrorSeverity::Error, Some(detail)) => log::error!(
            "Desktop command error [operation={} code={}]: {detail}",
            record.operation.code(),
            record.code
        ),
        (CommandErrorSeverity::Error, None) => log::error!(
            "Desktop command error [operation={} code={}]",
            record.operation.code(),
            record.code
        ),
    }
}

impl CommandError {
    pub(crate) const fn new(kind: CommandErrorKind) -> Self {
        Self {
            code: kind.code(),
            kind,
            reason_code: None,
            recovery_bundle_path: None,
            diagnostic: None,
            diagnostic_recorded: false,
        }
    }

    pub(crate) fn with_diagnostic(kind: CommandErrorKind, detail: impl std::fmt::Display) -> Self {
        Self {
            diagnostic: Some(CommandErrorDiagnostic {
                detail: detail.to_string(),
            }),
            ..Self::new(kind)
        }
    }

    /// Registers backend-only context once, at the command boundary where the
    /// stable operation name is known. Mapping and serialization remain pure.
    pub(super) fn recorded(self, operation: CommandOperation) -> Self {
        self.recorded_with(operation, write_command_diagnostic)
    }

    fn recorded_with(
        mut self,
        operation: CommandOperation,
        record: impl FnOnce(CommandDiagnosticRecord<'_>),
    ) -> Self {
        if self.diagnostic_recorded {
            return self;
        }

        record(CommandDiagnosticRecord {
            severity: self.kind.severity(),
            operation,
            code: self.code,
            detail: self
                .diagnostic
                .as_ref()
                .map(|diagnostic| diagnostic.detail.as_str()),
        });
        backend_diagnostics::record(
            crate::diagnostic_event::BackendDiagnosticEvent::command_failure(operation, self.kind),
        );
        self.diagnostic_recorded = true;
        self
    }

    pub(crate) fn task_failed(error: impl std::fmt::Display) -> Self {
        Self::with_diagnostic(CommandErrorKind::CommandTaskFailed, error)
    }

    pub(crate) fn invalid_argument(name: &'static str, reason: &'static str) -> Self {
        Self::with_diagnostic(
            CommandErrorKind::InvalidArgument,
            format_args!("invalid argument `{name}`: {reason}"),
        )
    }

    pub(crate) fn invalid_id(
        kind: CommandErrorKind,
        debug_label: &'static str,
        raw: impl std::fmt::Display,
    ) -> Self {
        Self::with_diagnostic(kind, format_args!("{debug_label}: {raw}"))
    }

    fn with_reason_code(mut self, reason_code: &'static str) -> Self {
        if self.kind.allows_reason_code(reason_code) {
            self.reason_code = Some(reason_code);
        }
        self
    }

    fn with_recovery_bundle_path(mut self, path: String) -> Self {
        if self.kind.allows_recovery_bundle_path() {
            self.recovery_bundle_path = Some(path);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use renderpilot_api::ApiError;
    use renderpilot_orchestration::{InvalidInstallRootReason, ServiceError};
    use serde_json::json;

    use super::*;
    use crate::diagnostic_event::CommandOperation;

    #[test]
    fn plain_error_serializes_only_the_machine_code() {
        let value = serde_json::to_value(CommandError::new(CommandErrorKind::GameNotFound))
            .expect("serialize CommandError");
        let keys = value
            .as_object()
            .expect("command error object")
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();

        assert_eq!(keys, BTreeSet::from(["code"]));
        assert_eq!(value.get("code"), Some(&json!("game_not_found")));
        for forbidden in [
            "messageKey",
            "details",
            "debugDetails",
            "severity",
            "suggestedActions",
        ] {
            assert!(value.get(forbidden).is_none(), "leaked field {forbidden}");
        }
    }

    #[test]
    fn invalid_install_root_carries_only_an_allowlisted_reason_code() {
        let error = CommandError::from(ApiError::Service(ServiceError::invalid_install_root(
            InvalidInstallRootReason::ContainsProvenInstall,
            "private root D:\\Games\\secret",
        )));
        let value = serde_json::to_value(error).expect("serialize CommandError");

        assert_eq!(
            value,
            json!({
                "code": "invalid_install_root",
                "reasonCode": "contains_proven_install",
            })
        );
    }

    #[test]
    fn recovery_bundle_is_structured_without_technical_details() {
        let error = CommandError::from(ApiError::Service(ServiceError::ManagedCleanupAmbiguous {
            game_id: "private-game".into(),
            targets: vec!["D:\\Games\\secret".into()],
            recovery_bundle_path: "C:\\Recovery\\bundle".into(),
        }));
        let value = serde_json::to_value(error).expect("serialize CommandError");

        assert_eq!(
            value,
            json!({
                "code": "managed_cleanup_ambiguous",
                "recoveryBundlePath": "C:\\Recovery\\bundle",
            })
        );
        assert!(!value.to_string().contains("D:\\\\Games"));
    }

    #[test]
    fn typed_service_errors_do_not_collapse_to_command_failed() {
        let cases = [
            (
                ServiceError::StorageFailed("db locked".into()),
                "storage_failed",
            ),
            (
                ServiceError::ProviderFailed("provider".into()),
                "provider_failed",
            ),
            (
                ServiceError::DetectionFailed("detector".into()),
                "detection_failed",
            ),
            (
                ServiceError::InvalidInput("bad input".into()),
                "invalid_argument",
            ),
            (
                ServiceError::RollbackAlsoFailed {
                    primary: "primary".into(),
                    rollback: "rollback".into(),
                },
                "rollback_also_failed",
            ),
        ];

        for (service_error, expected_code) in cases {
            let value = serde_json::to_value(CommandError::from(ApiError::Service(service_error)))
                .expect("serialize CommandError");
            assert_eq!(value.get("code"), Some(&json!(expected_code)));
            assert_ne!(value.get("code"), Some(&json!("command_failed")));
        }
    }

    #[test]
    fn generic_command_failure_remains_the_explicit_catch_all() {
        let value = serde_json::to_value(CommandError::from(ApiError::Service(
            ServiceError::CommandFailed("opaque provider prose".into()),
        )))
        .expect("serialize CommandError");

        assert_eq!(value, json!({ "code": "command_failed" }));
        assert!(!value.to_string().contains("opaque provider prose"));
    }

    #[test]
    fn access_denied_keeps_backend_diagnostics_off_the_wire() {
        let value = serde_json::to_value(CommandError::from(ApiError::Service(
            ServiceError::AccessDenied {
                operation: "updating private NVAPI setting".into(),
                detail: "NVAPI reported invalid user privilege for C:\\Users\\name".into(),
            },
        )))
        .expect("serialize CommandError");

        assert_eq!(value, json!({ "code": "access_denied" }));
        assert!(!value.to_string().contains("private"));
        assert!(!value.to_string().contains("C:\\\\Users"));
    }

    #[test]
    fn diagnostic_registration_never_changes_the_wire_contract() {
        let error = CommandError::with_diagnostic(
            CommandErrorKind::StorageFailed,
            "private database path C:\\Users\\name\\catalog.db",
        );
        let before = serde_json::to_value(&error).expect("serialize before recording");
        let after = serde_json::to_value(error.recorded(CommandOperation::ClearGameCover))
            .expect("serialize after recording");

        assert_eq!(before, json!({ "code": "storage_failed" }));
        assert_eq!(after, before);
    }

    #[test]
    fn updater_diagnostic_details_never_serialize() {
        let error = CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateSupervisorFailed,
            "private supervisor pipe at C:\\Users\\name",
        );
        let value = serde_json::to_value(error.recorded(CommandOperation::AppUpdateApply))
            .expect("serialize updater error");

        assert_eq!(value, json!({ "code": "app_update_supervisor_failed" }));
        assert!(!value.to_string().contains("private"));
    }

    #[test]
    fn diagnostic_registration_writes_to_the_sink_once() {
        let error = CommandError::new(CommandErrorKind::ConfirmationTokenMismatch);
        assert!(!error.diagnostic_recorded);
        let mut record_count = 0;

        let recorded = error.recorded_with(CommandOperation::ClearGameCover, |_| record_count += 1);
        assert!(recorded.diagnostic_recorded);

        let recorded_again =
            recorded.recorded_with(CommandOperation::ClearGameCover, |_| record_count += 1);
        assert!(recorded_again.diagnostic_recorded);
        assert_eq!(record_count, 1);
    }

    #[test]
    fn structured_fields_are_fail_closed_against_the_generated_allowlist() {
        let invalid = CommandError::new(CommandErrorKind::StorageFailed)
            .with_reason_code("filesystem_root")
            .with_recovery_bundle_path("C:\\private\\bundle".into());

        assert_eq!(
            serde_json::to_value(invalid).expect("serialize fail-closed error"),
            json!({ "code": "storage_failed" })
        );
    }
}
