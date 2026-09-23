//! Single backend-to-diagnostics bridge.
//!
//! Each runtime profile owns one typed writer adapter and consumes the same
//! closed event value without exposing a general-purpose logging sink.

use crate::diagnostic_event::BackendDiagnosticEvent;

#[cfg(all(windows, feature = "portable"))]
pub(crate) fn record(event: BackendDiagnosticEvent) {
    crate::portable_runtime::diagnostics_files::record_app_backend_event(event);
}

#[cfg(not(all(windows, feature = "portable")))]
pub(crate) fn record(event: BackendDiagnosticEvent) {
    crate::diagnostics::record_backend_event(event);
}

#[cfg(all(windows, feature = "portable"))]
pub(crate) fn record_rust_log(event: crate::diagnostics::RustLogEvent) {
    crate::portable_runtime::diagnostics_files::record_app_log_event(event);
}

#[cfg(not(all(windows, feature = "portable")))]
pub(crate) fn record_rust_log(event: crate::diagnostics::RustLogEvent) {
    crate::diagnostics::rust_log_event(event);
}

#[cfg(all(windows, feature = "portable"))]
pub(crate) fn record_frontend(event: crate::diagnostics::FrontendDiagnosticEvent) {
    crate::portable_runtime::diagnostics_files::record_app_frontend_event(event);
}

#[cfg(not(all(windows, feature = "portable")))]
pub(crate) fn record_frontend(event: crate::diagnostics::FrontendDiagnosticEvent) {
    crate::diagnostics::frontend_event(event);
}
