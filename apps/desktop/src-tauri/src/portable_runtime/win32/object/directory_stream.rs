use windows_sys::{
    Wdk::Storage::FileSystem::{
        FILE_DIRECTORY_INFORMATION, FileDirectoryInformation, NtQueryDirectoryFile,
    },
    Win32::{
        Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT},
        System::IO::IO_STATUS_BLOCK,
    },
};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    win32::object::handle::VerifiedDirectory,
};

const DIRECTORY_QUERY_BYTES: usize = 16 * 1024;
const DIRECTORY_QUERY_BUFFER_LENGTH: u32 = DIRECTORY_QUERY_BYTES as u32;

#[derive(Clone, Debug)]
pub(super) struct DirectoryEntry {
    pub(super) name: String,
    pub(super) record_bytes: usize,
    pub(super) is_directory: bool,
    pub(super) is_reparse: bool,
}

pub(super) fn visit_directory_entries(
    directory: &VerifiedDirectory,
    mut visit: impl FnMut(DirectoryEntry) -> Result<()>,
) -> Result<()> {
    let mut restart = true;
    loop {
        let mut bytes = [0u8; DIRECTORY_QUERY_BYTES];
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: retained verified handle plus fixed output storage remain live for this synchronous query.
        let code = unsafe {
            NtQueryDirectoryFile(
                directory.handle(),
                std::ptr::null_mut(),
                None,
                std::ptr::null(),
                &raw mut status,
                bytes.as_mut_ptr().cast(),
                DIRECTORY_QUERY_BUFFER_LENGTH,
                FileDirectoryInformation,
                false,
                std::ptr::null(),
                restart,
            )
        };
        restart = false;
        if code == windows_sys::Win32::Foundation::STATUS_NO_MORE_FILES {
            return Ok(());
        }
        if code != 0 || status.Information > bytes.len() {
            return Err(PortableRuntimeError::new(
                "portable_diagnostics_retention",
                "directory stream ended without clean end-of-directory",
            ));
        }
        visit_buffer(&bytes[..status.Information], &mut visit)?;
    }
}

fn visit_buffer(bytes: &[u8], visit: &mut impl FnMut(DirectoryEntry) -> Result<()>) -> Result<()> {
    let base = std::mem::offset_of!(FILE_DIRECTORY_INFORMATION, FileName);
    let next_offset = std::mem::offset_of!(FILE_DIRECTORY_INFORMATION, NextEntryOffset);
    let name_length = std::mem::offset_of!(FILE_DIRECTORY_INFORMATION, FileNameLength);
    let attributes_offset = std::mem::offset_of!(FILE_DIRECTORY_INFORMATION, FileAttributes);
    let mut offset = 0usize;
    while offset < bytes.len() {
        if bytes.len().saturating_sub(offset) < base {
            return Err(PortableRuntimeError::new(
                "portable_diagnostics_retention",
                "directory stream entry was truncated",
            ));
        }
        let name_length =
            usize::try_from(read_u32(bytes, offset + name_length)?).map_err(|_| {
                PortableRuntimeError::new(
                    "portable_diagnostics_retention",
                    "directory name overflow",
                )
            })?;
        if name_length == 0
            || name_length % 2 != 0
            || name_length > bytes.len().saturating_sub(offset + base)
        {
            return Err(PortableRuntimeError::new(
                "portable_diagnostics_retention",
                "directory stream name was invalid",
            ));
        }
        let name = String::from_utf16le(&bytes[offset + base..offset + base + name_length])
            .map_err(|_| {
                PortableRuntimeError::new(
                    "portable_diagnostics_retention",
                    "directory name was not UTF-16",
                )
            })?;
        // NtQueryDirectoryFile legally exposes these native pseudoentries. They are budgeted and consumers decide whether their type is admissible.
        if !is_native_pseudoentry(&name) && name.contains(['\\', '/']) {
            return Err(PortableRuntimeError::new(
                "portable_diagnostics_retention",
                "directory stream yielded a non-leaf name",
            ));
        }
        let attributes = read_u32(bytes, offset + attributes_offset)?;
        let next = usize::try_from(read_u32(bytes, offset + next_offset)?).map_err(|_| {
            PortableRuntimeError::new(
                "portable_diagnostics_retention",
                "directory next offset overflow",
            )
        })?;
        let record_bytes = if next == 0 {
            if offset + base + name_length > bytes.len() {
                return Err(PortableRuntimeError::new(
                    "portable_diagnostics_retention",
                    "directory stream made no progress",
                ));
            }
            bytes.len() - offset
        } else {
            if next < base || next > bytes.len().saturating_sub(offset) {
                return Err(PortableRuntimeError::new(
                    "portable_diagnostics_retention",
                    "directory stream next offset was invalid",
                ));
            }
            next
        };
        visit(DirectoryEntry {
            name,
            record_bytes,
            is_directory: attributes & FILE_ATTRIBUTE_DIRECTORY != 0,
            is_reparse: attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0,
        })?;
        if next == 0 {
            return Ok(());
        }
        offset += next;
    }
    Err(PortableRuntimeError::new(
        "portable_diagnostics_retention",
        "directory stream ended without terminal entry",
    ))
}

pub(super) fn is_native_pseudoentry(name: &str) -> bool {
    matches!(name, "." | "..")
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset.saturating_add(4))
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_diagnostics_retention",
                "directory header was truncated",
            )
        })?;
    Ok(u32::from_le_bytes(value))
}

#[cfg(test)]
mod tests {
    use super::{is_native_pseudoentry, visit_buffer};
    use crate::portable_runtime::error::Result;

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn native_dot_names_are_distinct_from_authoritative_leaves() {
        assert!(is_native_pseudoentry("."));
        assert!(is_native_pseudoentry(".."));
        assert!(!is_native_pseudoentry("admission.lock"));
        assert!(!is_native_pseudoentry("a/foreign"));
    }

    #[test]
    fn invalid_utf16_name_is_rejected_before_visiting() {
        let base = std::mem::offset_of!(
            windows_sys::Wdk::Storage::FileSystem::FILE_DIRECTORY_INFORMATION,
            FileName
        );
        let name_length_offset = std::mem::offset_of!(
            windows_sys::Wdk::Storage::FileSystem::FILE_DIRECTORY_INFORMATION,
            FileNameLength
        );
        let next_offset = std::mem::offset_of!(
            windows_sys::Wdk::Storage::FileSystem::FILE_DIRECTORY_INFORMATION,
            NextEntryOffset
        );
        let mut bytes = vec![0; base + 2];
        write_u32(&mut bytes, name_length_offset, 2);
        write_u32(&mut bytes, next_offset, 0);
        bytes[base..base + 2].copy_from_slice(&[0x00, 0xD8]);

        let mut visits = 0;
        let error = visit_buffer(&bytes, &mut |_entry| -> Result<()> {
            visits += 1;
            Ok(())
        })
        .expect_err("lone surrogate must be rejected");

        assert_eq!(
            error.to_string(),
            "portable_diagnostics_retention: directory name was not UTF-16"
        );
        assert_eq!(visits, 0);
    }
}
