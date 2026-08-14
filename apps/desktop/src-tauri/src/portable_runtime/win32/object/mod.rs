//! Purpose-typed retained Win32 object capabilities for the v1.2 authority
//! family. Raw handles and native access, share, and disposition masks remain
//! private to the handle adapter; focused child modules select closed profiles.

#![expect(
    unsafe_code,
    reason = "the portable object authority is the narrow owner of synchronous no-follow Windows handles"
)]

mod admission;
mod diagnostics;
mod directory_stream;
mod handle;
mod image;
mod root;

pub(crate) use admission::{AdmissionObjects, acquire_supervisor_admission};
pub(crate) use diagnostics::{
    CanonicalDiagnosticName, CompletedDiagnosticCandidate, DiagnosticDirectoryEntry,
    DiagnosticsRole, DiagnosticsRoleDirectory, canonical_diagnostic_name, create_active_diagnostic,
    open_completed_canonical_diagnostic, open_diagnostics_role_directory, visit_diagnostic_entries,
};
pub(crate) use image::{
    RawSupervisorImageObject, SelectedGenerationObject, open_raw_supervisor_image,
    open_selected_generation, running_app_identity,
};
pub(crate) use root::{ObjectIdentity, PortableRoot, open_portable_root};

#[cfg(test)]
mod tests;
