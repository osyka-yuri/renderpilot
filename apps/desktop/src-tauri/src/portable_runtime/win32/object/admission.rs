use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    root_authority::SupervisorRootBinding,
    win32::object::{
        directory_stream::{is_native_pseudoentry, visit_directory_entries},
        handle::{
            RelativeDirectoryOpen, RelativeFileOpen, VerifiedDirectory, VerifiedFile,
            open_relative_directory, open_relative_file,
        },
    },
};

#[derive(Debug)]
pub(crate) struct AdmissionObjects {
    _authority_directory: VerifiedDirectory,
    _lock: VerifiedFile,
}

pub(crate) fn acquire_supervisor_admission(
    binding: &SupervisorRootBinding,
) -> Result<AdmissionObjects> {
    let root = binding.root().object();
    let authority_parent = open_relative_directory(
        &root.0,
        ".renderpilot-runtime-authority",
        RelativeDirectoryOpen::CreateBranch,
    )?;
    let authority = open_relative_directory(
        &authority_parent,
        "v1",
        RelativeDirectoryOpen::CreateAndEnumerateFiles,
    )?;
    validate_namespace(&authority, false)?;
    let lock = open_relative_file(
        &authority,
        "admission.lock",
        RelativeFileOpen::ExclusiveOpenOrCreateReadDataAndAttributes,
    )?;
    validate_namespace(&authority, true)?;
    Ok(AdmissionObjects {
        _authority_directory: authority,
        _lock: lock,
    })
}

fn validate_namespace(directory: &VerifiedDirectory, lock_is_live: bool) -> Result<()> {
    visit_directory_entries(directory, |entry| {
        if is_native_pseudoentry(&entry.name) {
            return (entry.is_directory && !entry.is_reparse)
                .then_some(())
                .ok_or_else(|| {
                    PortableRuntimeError::new(
                        "portable_namespace_unknown",
                        "native pseudoentry had invalid object metadata",
                    )
                });
        }
        let allowed = match entry.name.as_str() {
            "admission.lock" => lock_is_live || (!entry.is_directory && !entry.is_reparse),
            "provenance" | "epochs" => entry.is_directory && !entry.is_reparse,
            _ => false,
        };
        allowed.then_some(()).ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_namespace_unknown",
                "runtime authority namespace contained an unrecognized retained leaf",
            )
        })
    })
}
