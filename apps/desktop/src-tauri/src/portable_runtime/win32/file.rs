#![expect(
    unsafe_code,
    reason = "portable handle ownership and atomic publication use narrow Win32 boundaries"
)]

use std::{
    fs::{File, OpenOptions},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, GENERIC_READ, GENERIC_WRITE, GetLastError,
    },
    Storage::FileSystem::{
        DELETE, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FileDispositionInfo, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        SetFileInformationByHandle,
    },
};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    win32::process::path_wide_nul,
};

const FILE_DISPOSITION_INFO_BYTES: u32 = std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoReplacePublication {
    Published,
    Occupied,
}

/// Creates a fresh publication candidate whose retained handle is sufficient
/// both for no-replace rename and for exact-object cleanup after a failed or
/// occupied publication attempt.
pub fn create_pending_file(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(Into::into)
}

/// Marks the exact retained candidate object for deletion. No path lookup is
/// performed, so a concurrent name replacement cannot redirect cleanup.
pub fn discard_exact_file(file: &File) -> Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: `disposition` has the layout required by the Win32 API and the
    // retained candidate handle was opened with DELETE access.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&raw const disposition).cast(),
            FILE_DISPOSITION_INFO_BYTES,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

/// Atomically makes a fully prepared same-volume file or directory visible
/// without ever replacing an existing authority object.
pub fn publish_no_replace(source: &Path, destination: &Path) -> Result<NoReplacePublication> {
    let source_wide = path_wide_nul(source);
    let destination_wide = path_wide_nul(destination);
    // SAFETY: both NUL-terminated paths remain live for the call. Omitting
    // MOVEFILE_REPLACE_EXISTING is the no-replace publication boundary.
    if unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    } != 0
    {
        return Ok(NoReplacePublication::Published);
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_ALREADY_EXISTS || error == ERROR_FILE_EXISTS || destination.try_exists()? {
        Ok(NoReplacePublication::Occupied)
    } else {
        Err(PortableRuntimeError::new(
            "portable_publication",
            format!(
                "atomic no-replace publication failed: {error}; source={}, destination={}",
                source.display(),
                destination.display()
            ),
        ))
    }
}
