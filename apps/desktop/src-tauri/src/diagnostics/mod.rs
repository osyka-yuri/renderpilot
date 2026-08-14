//! Privacy-safe, bounded portable diagnostics.
//!
//! The reusable writer is deliberately private to this module.  Portable is
//! the first profile; an installed profile may be added later without widening
//! the writer to arbitrary messages, paths, bytes, or serialization payloads.

mod portable;
mod writer;

pub(crate) use portable::{
    PortableDiagnosticWriter, PortableFailureClass, PortableFailureSite, PortableMilestone,
    PortableRole, Sha256Id, first_event_matches,
};
pub(crate) use writer::{DiagnosticCloseStatus, DiagnosticEmitStatus};
