//! Windows no-replace moves and handle-relative publication.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io,
    mem::MaybeUninit,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path, PathBuf},
};

use windows_sys::{
    Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
    },
    Win32::{
        Foundation::{
            ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, GENERIC_READ, GENERIC_WRITE, NTSTATUS,
            RtlNtStatusToDosError, STATUS_PENDING, WAIT_FAILED, WAIT_OBJECT_0,
        },
        Storage::FileSystem::{
            DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
            FileDispositionInfo, SYNCHRONIZE, SetFileInformationByHandle,
        },
        System::{
            IO::IO_STATUS_BLOCK,
            Threading::{INFINITE, WaitForSingleObject},
        },
    },
};

use super::replace::temporary_file_path;
#[cfg(test)]
use super::{NoReplaceTestFault, no_replace_test_fault};
use super::{
    NoReplaceWrite, PreparedNoReplaceWrite, sync_no_replace_temp_file, write_no_replace_temp_bytes,
};
use crate::ServiceError;

const OBJECT_ATTRIBUTES_BYTES: u32 =
    std::mem::size_of::<windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES>() as u32;

pub(super) fn move_windows_no_replace(
    source: &Path,
    destination: &Path,
) -> Result<(), ServiceError> {
    let source_parent = source
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            crate::failed(format!(
                "source `{}` has no parent directory",
                source.display()
            ))
        })?;
    let destination_parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            crate::failed(format!(
                "destination `{}` has no parent directory",
                destination.display()
            ))
        })?;
    let (_source_leaf, source_leaf_wide) = windows_destination_leaf(source)?;
    let (destination_leaf, destination_leaf_wide) = windows_destination_leaf(destination)?;
    let source_parent = open_windows_no_replace_parent(source_parent)?;
    let destination_parent = open_windows_no_replace_parent(destination_parent)?;
    // Open the source leaf relative to the retained, no-follow parent.  The
    // source pathname is never resolved again after the parent handle is
    // acquired, so a parent/leaf swap cannot redirect the native rename.
    let source_file =
        open_windows_entry_relative(&source_parent, &source_leaf_wide).map_err(|error| {
            crate::failed(format!(
                "failed to open source `{}` relative to its retained parent: {error}",
                source.display()
            ))
        })?;
    let source_metadata = source_file.metadata().map_err(|error| {
        crate::failed(format!(
            "failed to inspect retained source `{}`: {error}",
            source.display()
        ))
    })?;
    if source_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(crate::failed(format!(
            "source `{}` is a reparse object",
            source.display()
        )));
    }
    // Keep the source entry and both no-follow parent handles open through
    // the native rename.  No syscall below re-resolves a hostile parent path.
    rename_open_handle_no_replace(&source_file, &destination_parent, &destination_leaf_wide)
        .map_err(|error| {
            crate::failed(format!(
                "failed to move `{}` to `{}` without replacement: {error}",
                source.display(),
                destination_leaf.to_string_lossy()
            ))
        })
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "the retained source handle is renamed through the documented Windows native API"
)]
fn rename_open_handle_no_replace(
    source: &File,
    destination_parent: &File,
    destination_leaf_wide: &[u16],
) -> io::Result<()> {
    fn ntstatus_result(status: NTSTATUS) -> io::Result<()> {
        if status >= 0 {
            return Ok(());
        }
        let dos_error = unsafe { RtlNtStatusToDosError(status) };
        let dos_error = i32::try_from(dos_error).map_err(|_| {
            io::Error::other(format!(
                "NTSTATUS {status:#010x} translated to an invalid Win32 code"
            ))
        })?;
        Err(io::Error::from_raw_os_error(dos_error))
    }
    let name_bytes = destination_leaf_wide
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "destination leaf is too long")
        })?;
    let required_bytes = std::mem::size_of::<FILE_RENAME_INFORMATION>()
        .checked_add(name_bytes as usize)
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "rename buffer is too large"))?;
    let slot_bytes = std::mem::size_of::<FILE_RENAME_INFORMATION>();
    let slots = (required_bytes as usize).div_ceil(slot_bytes);
    let mut storage = Vec::with_capacity(slots);
    storage.resize_with(slots, MaybeUninit::<FILE_RENAME_INFORMATION>::zeroed);
    let rename = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    unsafe {
        (*rename).Anonymous.ReplaceIfExists = false;
        (*rename).RootDirectory = destination_parent.as_raw_handle();
        (*rename).FileNameLength = name_bytes;
        std::ptr::copy_nonoverlapping(
            destination_leaf_wide.as_ptr(),
            std::ptr::addr_of_mut!((*rename).FileName).cast::<u16>(),
            destination_leaf_wide.len(),
        );
        let mut io_status = IO_STATUS_BLOCK::default();
        let status = NtSetInformationFile(
            source.as_raw_handle(),
            &raw mut io_status,
            rename.cast(),
            required_bytes,
            FileRenameInformation,
        );
        if status < 0 {
            return ntstatus_result(status);
        }
        if status == STATUS_PENDING {
            match WaitForSingleObject(source.as_raw_handle(), INFINITE) {
                WAIT_OBJECT_0 => {}
                WAIT_FAILED => return Err(io::Error::last_os_error()),
                result => {
                    return Err(io::Error::other(format!(
                        "unexpected rename wait result {result:#010x}"
                    )));
                }
            }
        }
        ntstatus_result(io_status.Anonymous.Status)
    }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "calling the Windows filesystem API requires a small audited FFI boundary"
)]
impl PreparedNoReplaceWrite {
    pub(super) fn publish_windows(&mut self) -> Result<NoReplaceWrite, ServiceError> {
        let publication = self.publish_windows_exact();
        match publication {
            Ok(()) => {
                // The rename consumed this object's temporary name. Disarm
                // cleanup before the candidate closes over the new name.
                self.close_windows_handles();
                Ok(NoReplaceWrite::Published)
            }
            Err(publication_error) => {
                let cleanup = self.discard_windows_exact();
                self.close_windows_handles();
                if let Err(cleanup_error) = cleanup {
                    return Err(crate::failed(format!(
                        "failed to discard exact publication candidate `{}` after publication error `{publication_error}`: {cleanup_error}",
                        self.destination.display()
                    )));
                }
                if windows_destination_is_occupied(&publication_error) {
                    Ok(NoReplaceWrite::Occupied)
                } else {
                    Err(crate::failed(format!(
                        "failed to publish exact candidate to `{}` without replacement: {publication_error}",
                        self.destination.display()
                    )))
                }
            }
        }
    }

    pub(super) fn discard_windows(mut self) -> Result<(), ServiceError> {
        let cleanup = self.discard_windows_exact();
        self.close_windows_handles();
        cleanup.map_err(|error| {
            crate::failed(format!(
                "failed to discard exact prepared publication candidate `{}`: {error}",
                self.destination.display()
            ))
        })
    }

    pub(super) fn drop_windows(&mut self) {
        if self.file.is_none() {
            self.parent_directory.take();
            self.temp_path.take();
            return;
        }
        let cleanup = self.discard_windows_exact();
        self.close_windows_handles();
        if let Err(error) = cleanup {
            tracing::error!(
                "failed to discard abandoned exact publication candidate `{}`: {error}",
                self.destination.display()
            );
        }
    }

    fn close_windows_handles(&mut self) {
        self.file.take();
        self.parent_directory.take();
        self.temp_path.take();
    }

    fn publish_windows_exact(&self) -> io::Result<()> {
        fn ntstatus_result(status: NTSTATUS) -> io::Result<()> {
            if status >= 0 {
                return Ok(());
            }
            // SAFETY: this pure ntdll translation is called only for an NTSTATUS
            // returned by the audited native publication operation.
            let dos_error = unsafe { RtlNtStatusToDosError(status) };
            let dos_error = i32::try_from(dos_error).map_err(|_| {
                io::Error::other(format!(
                    "NTSTATUS {status:#010x} translated to an out-of-range Win32 error {dos_error:#010x}"
                ))
            })?;
            Err(io::Error::from_raw_os_error(dos_error))
        }

        #[cfg(test)]
        no_replace_test_fault(NoReplaceTestFault::Publish)
            .map_err(|error| io::Error::other(error.to_string()))?;

        let name_bytes = self
            .destination_leaf_wide
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .and_then(|length| u32::try_from(length).ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "destination leaf `{}` is too long",
                        self.destination_leaf.to_string_lossy()
                    ),
                )
            })?;
        let required_bytes = std::mem::size_of::<FILE_RENAME_INFORMATION>()
            .checked_add(name_bytes as usize)
            .and_then(|length| u32::try_from(length).ok())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "rename buffer is too large")
            })?;
        let slot_bytes = std::mem::size_of::<FILE_RENAME_INFORMATION>();
        let slots = (required_bytes as usize).div_ceil(slot_bytes);
        let mut storage = Vec::with_capacity(slots);
        storage.resize_with(slots, MaybeUninit::<FILE_RENAME_INFORMATION>::zeroed);
        let rename = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
        let file = self
            .file
            .as_ref()
            .expect("prepared Windows candidate retains its file handle");
        let parent = self
            .parent_directory
            .as_ref()
            .expect("prepared Windows candidate retains its destination parent handle");

        // SAFETY: `storage` has FILE_RENAME_INFORMATION alignment and reserves
        // the exact FILE_RENAME_INFORMATION-plus-leaf UTF-16 span for the API
        // call. Both retained handles and `io_status` remain live throughout
        // this handle-relative native rename.
        unsafe {
            (*rename).Anonymous.ReplaceIfExists = false;
            (*rename).RootDirectory = parent.as_raw_handle();
            (*rename).FileNameLength = name_bytes;
            std::ptr::copy_nonoverlapping(
                self.destination_leaf_wide.as_ptr(),
                std::ptr::addr_of_mut!((*rename).FileName).cast::<u16>(),
                self.destination_leaf_wide.len(),
            );
            let mut io_status = IO_STATUS_BLOCK::default();
            let direct_status = NtSetInformationFile(
                file.as_raw_handle(),
                &raw mut io_status,
                rename.cast(),
                required_bytes,
                FileRenameInformation,
            );
            if direct_status < 0 {
                return ntstatus_result(direct_status);
            }
            if direct_status == STATUS_PENDING {
                match WaitForSingleObject(file.as_raw_handle(), INFINITE) {
                    WAIT_OBJECT_0 => {}
                    WAIT_FAILED => return Err(io::Error::last_os_error()),
                    result => {
                        return Err(io::Error::other(format!(
                            "waiting for native rename completion returned unexpected result {result:#010x}"
                        )));
                    }
                }
            }
            ntstatus_result(io_status.Anonymous.Status)
        }
    }

    fn discard_windows_exact(&self) -> io::Result<()> {
        #[cfg(test)]
        no_replace_test_fault(NoReplaceTestFault::Cleanup)
            .map_err(|error| io::Error::other(error.to_string()))?;

        let file = self
            .file
            .as_ref()
            .expect("prepared Windows candidate retains its file handle");
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: `disposition` has the exact API layout and the retained
        // handle is the candidate opened with DELETE access.
        let deleted = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                (&raw const disposition).cast(),
                u32::try_from(std::mem::size_of::<FILE_DISPOSITION_INFO>())
                    .expect("FILE_DISPOSITION_INFO fits in a Win32 buffer length"),
            )
        };
        if deleted == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(windows)]
pub(super) fn prepare_windows_no_replace(
    path: &Path,
    bytes: &[u8],
) -> Result<PreparedNoReplaceWrite, ServiceError> {
    let (destination_leaf, destination_leaf_wide) = windows_destination_leaf(path)?;
    let parent = path.parent().ok_or_else(|| {
        crate::failed(format!(
            "cannot write file `{}` because it has no parent directory",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        crate::failed(format!(
            "failed to create directory `{}`: {error}",
            parent.display()
        ))
    })?;
    let parent_directory = open_windows_no_replace_parent(parent)?;
    #[cfg(test)]
    no_replace_test_fault(NoReplaceTestFault::Create)?;
    let temp_path = temporary_file_path(path, "publish");
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .open(&temp_path)
        .map_err(|error| {
            crate::failed(format!(
                "failed to create publication temporary file `{}`: {error}",
                temp_path.display()
            ))
        })?;
    let write = write_no_replace_temp_bytes(&mut file, &temp_path, bytes);
    if let Err(write_error) = write {
        return discard_windows_after_prepare_error(
            file,
            parent_directory,
            destination_leaf,
            destination_leaf_wide,
            temp_path,
            path,
            write_error,
        );
    }
    let sync = sync_no_replace_temp_file(&file, &temp_path);
    if let Err(sync_error) = sync {
        return discard_windows_after_prepare_error(
            file,
            parent_directory,
            destination_leaf,
            destination_leaf_wide,
            temp_path,
            path,
            sync_error,
        );
    }
    Ok(PreparedNoReplaceWrite {
        file: Some(file),
        parent_directory: Some(parent_directory),
        destination_leaf,
        destination_leaf_wide,
        temp_path: Some(temp_path),
        destination: path.to_owned(),
    })
}

#[cfg(windows)]
fn discard_windows_after_prepare_error(
    file: File,
    parent_directory: File,
    destination_leaf: OsString,
    destination_leaf_wide: Vec<u16>,
    temp_path: PathBuf,
    destination: &Path,
    write_error: ServiceError,
) -> Result<PreparedNoReplaceWrite, ServiceError> {
    let prepared = PreparedNoReplaceWrite {
        file: Some(file),
        parent_directory: Some(parent_directory),
        destination_leaf,
        destination_leaf_wide,
        temp_path: Some(temp_path),
        destination: destination.to_owned(),
    };
    match prepared.discard() {
        Ok(()) => Err(write_error),
        Err(cleanup_error) => Err(crate::failed(format!(
            "failed to discard exact publication candidate after write or sync error `{write_error}`: {cleanup_error}"
        ))),
    }
}

#[cfg(windows)]
fn windows_destination_is_occupied(error: &io::Error) -> bool {
    matches!(
        error
            .raw_os_error()
            .and_then(|code| u32::try_from(code).ok()),
        Some(ERROR_FILE_EXISTS | ERROR_ALREADY_EXISTS)
    )
}

#[cfg(windows)]
fn windows_destination_leaf(path: &Path) -> Result<(OsString, Vec<u16>), ServiceError> {
    let leaf = path
        .file_name()
        .filter(|leaf| !leaf.is_empty())
        .ok_or_else(|| {
            crate::failed(format!(
                "cannot publish `{}` because its destination has no leaf name",
                path.display()
            ))
        })?;
    let mut components = Path::new(leaf).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(crate::failed(format!(
            "cannot publish `{}` because its destination leaf was not exactly one normal component",
            path.display()
        )));
    }
    let leaf = leaf.to_owned();
    let wide = leaf.encode_wide().collect::<Vec<_>>();
    if wide.is_empty()
        || wide.contains(&0)
        || wide.iter().any(|unit| {
            *unit == u16::from(b'/') || *unit == u16::from(b'\\') || *unit == u16::from(b':')
        })
        || wide
            .last()
            .is_some_and(|unit| *unit == u16::from(b'.') || *unit == u16::from(b' '))
    {
        return Err(crate::failed(format!(
            "cannot publish `{}` because its destination leaf was unsafe",
            path.display()
        )));
    }
    Ok((leaf, wide))
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "NtCreateFile is the documented handle-relative Windows open primitive"
)]
fn open_windows_entry_relative(parent: &File, leaf: &[u16]) -> io::Result<File> {
    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{HANDLE, OBJ_CASE_INSENSITIVE, UNICODE_STRING},
            Storage::FileSystem::{
                DELETE, FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE, SYNCHRONIZE,
            },
        },
    };

    let byte_length = leaf
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|length| u16::try_from(length).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "entry leaf is too long"))?;
    if leaf.is_empty() || leaf.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "entry leaf is empty or contains NUL",
        ));
    }
    let name = UNICODE_STRING {
        Length: byte_length,
        MaximumLength: byte_length,
        Buffer: leaf.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: OBJECT_ATTRIBUTES_BYTES,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &raw const name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut io_status = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            DELETE | GENERIC_READ | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            &raw const attributes,
            &raw mut io_status,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 || handle.is_null() {
        return Err(io::Error::other(format!(
            "relative entry open failed with NTSTATUS {status:#010x}"
        )));
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}

#[cfg(windows)]
fn open_windows_no_replace_parent(parent: &Path) -> Result<File, ServiceError> {
    let directory = OpenOptions::new()
        .access_mode(FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(parent)
        .map_err(|error| {
            crate::failed(format!(
                "failed to open publication parent directory `{}`: {error}",
                parent.display()
            ))
        })?;
    let metadata = directory.metadata().map_err(|error| {
        crate::failed(format!(
            "failed to inspect publication parent directory `{}`: {error}",
            parent.display()
        ))
    })?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(crate::failed(format!(
            "publication parent directory `{}` was not a non-reparse directory",
            parent.display()
        )));
    }
    Ok(directory)
}
