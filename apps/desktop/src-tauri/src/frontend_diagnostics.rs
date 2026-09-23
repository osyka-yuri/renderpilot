//! Strict IPC admission for the frontend's low-detail diagnostic events.

use serde::Deserialize;

use crate::diagnostics::{
    BoundaryCode, DiagnosticLevel, FrontendDiagnosticEvent, I18nOperation, LocaleCode, LocaleMode,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FrontendDiagnosticInput {
    source: String,
    operation: String,
    code: String,
    contract_status: String,
    severity: String,
    #[serde(default, deserialize_with = "present_string")]
    locale: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    mode: Option<String>,
}

fn present_string<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    String::deserialize(deserializer).map(Some)
}

impl FrontendDiagnosticInput {
    fn into_event(self) -> Option<FrontendDiagnosticEvent> {
        match self.source.as_str() {
            "i18n" => {
                if self.code != "i18n_locale_load_failed"
                    || self.contract_status != "known"
                    || self.severity != "warning"
                {
                    return None;
                }
                Some(FrontendDiagnosticEvent::I18n {
                    operation: I18nOperation::parse(&self.operation)?,
                    locale: LocaleCode::parse(self.locale.as_deref()?)?,
                    mode: LocaleMode::parse(self.mode.as_deref()?)?,
                })
            }
            "client-boundary" => {
                if self.operation != "client_boundary"
                    || self.locale.is_some()
                    || self.mode.is_some()
                {
                    return None;
                }
                let level = parse_level(&self.severity)?;
                let code = match (self.contract_status.as_str(), self.code.as_str()) {
                    ("known", "known_local_error") => BoundaryCode::KnownLocal,
                    ("unknown", "client_boundary") => BoundaryCode::UnknownContract,
                    ("malformed", "client_boundary") => BoundaryCode::MalformedContract,
                    _ => return None,
                };
                Some(FrontendDiagnosticEvent::ClientBoundary { level, code })
            }
            // Rust already records actual command failures. Only the closed
            // transport-failure fallback represents a frontend-only event.
            "desktop-command"
                if self.operation == "transport"
                    && self.code == "desktop_transport_failed"
                    && self.contract_status == "malformed"
                    && self.severity == "error"
                    && self.locale.is_none()
                    && self.mode.is_none() =>
            {
                Some(FrontendDiagnosticEvent::IpcTransport)
            }
            _ => None,
        }
    }
}

fn parse_level(value: &str) -> Option<DiagnosticLevel> {
    match value {
        "warning" => Some(DiagnosticLevel::Warning),
        "error" => Some(DiagnosticLevel::Error),
        _ => None,
    }
}

#[tauri::command]
pub(crate) fn record_frontend_diagnostic(event: FrontendDiagnosticInput) {
    if let Some(event) = event.into_event() {
        crate::backend_diagnostics::record_frontend(event);
    }
}

#[cfg(test)]
mod tests {
    use super::FrontendDiagnosticInput;
    use crate::diagnostics::{BoundaryCode, DiagnosticLevel, FrontendDiagnosticEvent};

    fn input(
        source: &str,
        operation: &str,
        code: &str,
        status: &str,
        severity: &str,
    ) -> FrontendDiagnosticInput {
        FrontendDiagnosticInput {
            source: source.to_owned(),
            operation: operation.to_owned(),
            code: code.to_owned(),
            contract_status: status.to_owned(),
            severity: severity.to_owned(),
            locale: None,
            mode: None,
        }
    }

    #[test]
    fn accepts_only_closed_i18n_and_boundary_projections() {
        let mut i18n = input(
            "i18n",
            "initialize_locale",
            "i18n_locale_load_failed",
            "known",
            "warning",
        );
        i18n.locale = Some("ru".to_owned());
        i18n.mode = Some("system".to_owned());
        assert!(i18n.into_event().is_some());

        let boundary = input(
            "client-boundary",
            "client_boundary",
            "client_boundary",
            "malformed",
            "warning",
        )
        .into_event()
        .expect("closed client-boundary event");
        let boundary_debug = format!("{boundary:?}");
        assert!(!boundary_debug.contains("private"));
        assert!(!boundary_debug.contains("C:/"));
        assert!(serde_json::from_str::<FrontendDiagnosticInput>(
            r#"{"source":"client-boundary","operation":"x","code":"y","contractStatus":"malformed","severity":"warning","cause":"private"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<FrontendDiagnosticInput>(
            r#"{"source":"client-boundary","operation":"client_boundary","code":"client_boundary","contractStatus":"malformed","severity":"warning","locale":null}"#
        )
        .is_err());
        assert!(
            input("client-boundary", "x", "y", "known", "warning")
                .into_event()
                .is_none()
        );
        assert!(
            input("client-boundary", "x", "y", "malformed", "fatal")
                .into_event()
                .is_none()
        );
        assert!(
            input(
                "client-boundary",
                "C:/private",
                "private cause",
                "malformed",
                "warning",
            )
            .into_event()
            .is_none()
        );
    }

    #[test]
    fn only_the_known_transport_fallback_passes_command_suppression() {
        assert!(
            input(
                "desktop-command",
                "transport",
                "desktop_transport_failed",
                "malformed",
                "error"
            )
            .into_event()
            .is_some()
        );
        assert!(
            input(
                "desktop-command",
                "apply_swap",
                "storage_failed",
                "known",
                "error"
            )
            .into_event()
            .is_none()
        );
        assert!(
            input(
                "desktop-command",
                "unsafe supplied value",
                "desktop_transport_failed",
                "malformed",
                "error",
            )
            .into_event()
            .is_none()
        );
    }

    #[test]
    fn known_client_boundary_requires_the_closed_local_marker() {
        assert_eq!(
            input(
                "client-boundary",
                "client_boundary",
                "known_local_error",
                "known",
                "error",
            )
            .into_event(),
            Some(FrontendDiagnosticEvent::ClientBoundary {
                level: DiagnosticLevel::Error,
                code: BoundaryCode::KnownLocal,
            })
        );
        assert!(
            input(
                "client-boundary",
                "client_boundary",
                "storage_failed",
                "known",
                "error",
            )
            .into_event()
            .is_none()
        );
        assert!(
            input(
                "client-boundary",
                "arbitrary operation",
                "known_local_error",
                "known",
                "error",
            )
            .into_event()
            .is_none()
        );
    }
}
