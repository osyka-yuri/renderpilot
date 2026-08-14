//! Single backend-to-diagnostics bridge.
//!
//! Installed builds deliberately consume the closed event without opening a
//! file, changing startup, or introducing a second writer authority.

use crate::diagnostic_event::BackendDiagnosticEvent;

#[cfg(all(windows, feature = "portable"))]
pub(crate) fn record(event: BackendDiagnosticEvent) {
    crate::portable_runtime::diagnostics_files::record_app_backend_event(event);
}

#[cfg(not(all(windows, feature = "portable")))]
pub(crate) fn record(_event: BackendDiagnosticEvent) {}
