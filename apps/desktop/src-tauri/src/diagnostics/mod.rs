//! Privacy-safe, bounded desktop diagnostic profiles.
//!
//! The reusable writer is deliberately private to this module. Each runtime
//! profile supplies a sealed schema and typed events rather than arbitrary
//! messages, paths, bytes, or serialization payloads. A few approved game
//! paths enter only through validated, explicitly named diagnostic fields.

#[cfg(not(all(windows, feature = "portable")))]
mod installed;
mod name;
#[cfg(all(windows, feature = "portable"))]
mod portable;
mod projection;
mod writer;

#[cfg(all(windows, feature = "portable"))]
pub(crate) use name::{canonical_filename_prefix, format_filename_timestamp};

#[cfg(not(all(windows, feature = "portable")))]
pub(crate) use installed::{
    InstalledLifecycle, frontend_event, install_installed, installed_event, record_backend_event,
    rust_log_event, shutdown_installed,
};
#[cfg(all(windows, feature = "portable"))]
pub(crate) use name::{PortableDiagnosticName, parse_portable_diagnostic_name};
#[cfg(all(windows, feature = "portable"))]
pub(crate) use portable::{
    PortableDiagnosticWriter, PortableFailureClass, PortableFailureSite, PortableMilestone,
    PortableRole, PortableSessionEvent, Sha256Id, first_event_matches_display_id,
};
pub(crate) use projection::{
    BoundaryCode, DiagnosticLevel, FrontendDiagnosticEvent, I18nOperation, LocaleCode, LocaleMode,
    RustLogCategory, RustLogEvent, is_valid_diagnostic_path,
};
#[cfg(all(windows, feature = "portable"))]
pub(crate) use writer::{DiagnosticCloseStatus, DiagnosticEmitStatus};
