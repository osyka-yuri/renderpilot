use windows_sys::Win32::Storage::FileSystem::{
    FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    win32::object::handle::{
        RelativeDirectoryOpen, RelativeFileOpen, VerifiedDirectory, VerifiedFile, information,
        open_relative_directory, open_relative_file,
    },
};

use super::{
    PortableRoot,
    directory_stream::{DirectoryEntry, is_native_pseudoentry, visit_directory_entries},
};

const FILE_DISPOSITION_INFO_BYTES: u32 = std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32;

#[derive(Debug)]
pub(crate) struct DiagnosticsRoleDirectory {
    directory: VerifiedDirectory,
    role: DiagnosticsRole,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticsRole {
    Supervisor,
    App,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalDiagnosticName(String);

impl CanonicalDiagnosticName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub(crate) struct CompletedDiagnosticCandidate(VerifiedFile);

#[derive(Clone, Debug)]
pub(crate) struct DiagnosticDirectoryEntry {
    pub(crate) name: String,
    pub(crate) record_bytes: usize,
    pub(crate) is_native_pseudoentry: bool,
    pub(crate) is_directory: bool,
    pub(crate) is_reparse: bool,
}

pub(crate) fn open_diagnostics_role_directory(
    root: &PortableRoot,
    role: DiagnosticsRole,
) -> Result<DiagnosticsRoleDirectory> {
    let data = open_relative_directory(&root.0, "data", RelativeDirectoryOpen::CreateBranch)?;
    let logs = open_relative_directory(&data, "logs", RelativeDirectoryOpen::CreateBranch)?;
    let portable = open_relative_directory(&logs, "portable", RelativeDirectoryOpen::CreateBranch)?;
    let role_directory = open_relative_directory(
        &portable,
        match role {
            DiagnosticsRole::Supervisor => "supervisor",
            DiagnosticsRole::App => "app",
        },
        RelativeDirectoryOpen::CreateAndEnumerateFiles,
    )?;
    Ok(DiagnosticsRoleDirectory {
        directory: role_directory,
        role,
    })
}

pub(crate) fn canonical_diagnostic_name(
    directory: &DiagnosticsRoleDirectory,
    name: &str,
) -> Result<CanonicalDiagnosticName> {
    canonical_for_role(directory.role, name)
        .then(|| CanonicalDiagnosticName(name.to_owned()))
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_diagnostics_open",
                "diagnostic filename was not canonical for its role",
            )
        })
}

pub(crate) fn create_active_diagnostic(
    directory: &DiagnosticsRoleDirectory,
    name: &CanonicalDiagnosticName,
) -> Result<std::fs::File> {
    Ok(open_relative_file(
        &directory.directory,
        name.as_str(),
        RelativeFileOpen::SharedCreateWriteAndReadAttributes,
    )?
    .into_file())
}

pub(crate) fn visit_diagnostic_entries(
    directory: &DiagnosticsRoleDirectory,
    mut visit: impl FnMut(DiagnosticDirectoryEntry) -> Result<()>,
) -> Result<()> {
    visit_directory_entries(&directory.directory, |entry: DirectoryEntry| {
        visit(DiagnosticDirectoryEntry {
            is_native_pseudoentry: is_native_pseudoentry(&entry.name),
            name: entry.name,
            record_bytes: entry.record_bytes,
            is_directory: entry.is_directory,
            is_reparse: entry.is_reparse,
        })
    })
}

/// Opens one exact canonical completed leaf.  Callers cannot supply an
/// arbitrary filename and this is the only diagnostics capability with DELETE.
pub(crate) fn open_completed_canonical_diagnostic(
    directory: &DiagnosticsRoleDirectory,
    name: &CanonicalDiagnosticName,
) -> Result<CompletedDiagnosticCandidate> {
    Ok(CompletedDiagnosticCandidate(open_relative_file(
        &directory.directory,
        name.as_str(),
        RelativeFileOpen::ExclusiveReadAndDelete,
    )?))
}

fn canonical_for_role(role: DiagnosticsRole, name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".log") else {
        return false;
    };
    match role {
        DiagnosticsRole::Supervisor => lower_hex_64(stem),
        DiagnosticsRole::App => stem.split_once('-').is_some_and(|(session, transaction)| {
            lower_hex_64(session) && lower_hex_64(transaction)
        }),
    }
}

fn lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

impl CompletedDiagnosticCandidate {
    pub(crate) fn read_first_record(&mut self) -> Result<Vec<u8>> {
        self.0.read_first_record()
    }

    pub(crate) fn last_write(&self) -> Result<u64> {
        let time = information(self.0.handle())?.ftLastWriteTime;
        Ok((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime))
    }

    pub(crate) fn delete_exact(&self) -> Result<()> {
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: only this completed exact handle carries DELETE access.
        if unsafe {
            SetFileInformationByHandle(
                self.0.handle(),
                FileDispositionInfo,
                (&raw const disposition).cast(),
                FILE_DISPOSITION_INFO_BYTES,
            )
        } == 0
        {
            return Err(PortableRuntimeError::new(
                "portable_diagnostics_retention",
                "exact completed diagnostic could not be deleted",
            ));
        }
        Ok(())
    }
}
