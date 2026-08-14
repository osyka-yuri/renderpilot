use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
};

use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF,
            FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, NTCREATEFILE_CREATE_DISPOSITION,
            NTCREATEFILE_CREATE_OPTIONS, NtCreateFile,
        },
    },
    Win32::{
        Foundation::{
            GENERIC_ALL, GENERIC_EXECUTE, GENERIC_READ, GENERIC_WRITE, HANDLE, HANDLE_FLAG_INHERIT,
            INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, SetHandleInformation, UNICODE_STRING,
        },
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY,
            FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
            FILE_GENERIC_WRITE, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_READ_DATA,
            FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, GetFileInformationByHandle,
            OPEN_EXISTING, SYNCHRONIZE,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    signature::sha256_hex,
};

use super::ObjectIdentity;

const OBJECT_ATTRIBUTES_BYTES: u32 = std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32;
const GENERIC_ACCESS_MASK: u32 = GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE | GENERIC_ALL;

#[derive(Clone, Copy, Debug)]
pub(super) enum RelativeDirectoryOpen {
    Traverse,
    CreateBranch,
    CreateAndEnumerateFiles,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum RelativeFileOpen {
    SharedRead,
    SharedCreateWriteAndReadAttributes,
    ExclusiveReadAndDelete,
    ExclusiveOpenOrCreateReadDataAndAttributes,
}

#[derive(Clone, Copy)]
struct NativeOpenPolicy {
    desired_access: u32,
    share_access: u32,
    disposition: NTCREATEFILE_CREATE_DISPOSITION,
}

impl RelativeDirectoryOpen {
    const fn native(self) -> NativeOpenPolicy {
        match self {
            Self::Traverse => NativeOpenPolicy {
                desired_access: FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                share_access: FILE_SHARE_READ,
                disposition: FILE_OPEN,
            },
            Self::CreateBranch => NativeOpenPolicy {
                desired_access: FILE_TRAVERSE
                    | FILE_ADD_SUBDIRECTORY
                    | FILE_READ_ATTRIBUTES
                    | SYNCHRONIZE,
                share_access: FILE_SHARE_READ | FILE_SHARE_WRITE,
                disposition: FILE_OPEN_IF,
            },
            Self::CreateAndEnumerateFiles => NativeOpenPolicy {
                desired_access: FILE_LIST_DIRECTORY
                    | FILE_ADD_FILE
                    | FILE_TRAVERSE
                    | FILE_READ_ATTRIBUTES
                    | SYNCHRONIZE,
                share_access: FILE_SHARE_READ | FILE_SHARE_WRITE,
                disposition: FILE_OPEN_IF,
            },
        }
    }
}

impl RelativeFileOpen {
    const fn native(self) -> NativeOpenPolicy {
        match self {
            Self::SharedRead => NativeOpenPolicy {
                desired_access: FILE_GENERIC_READ,
                share_access: FILE_SHARE_READ,
                disposition: FILE_OPEN,
            },
            Self::SharedCreateWriteAndReadAttributes => NativeOpenPolicy {
                desired_access: FILE_GENERIC_WRITE | FILE_READ_ATTRIBUTES,
                share_access: FILE_SHARE_READ,
                disposition: FILE_CREATE,
            },
            Self::ExclusiveReadAndDelete => NativeOpenPolicy {
                desired_access: FILE_GENERIC_READ | DELETE,
                share_access: 0,
                disposition: FILE_OPEN,
            },
            Self::ExclusiveOpenOrCreateReadDataAndAttributes => NativeOpenPolicy {
                desired_access: FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                share_access: 0,
                disposition: FILE_OPEN_IF,
            },
        }
    }
}

#[derive(Debug)]
pub(super) struct VerifiedDirectory {
    file: File,
    identity: ObjectIdentity,
}

impl VerifiedDirectory {
    pub(super) fn handle(&self) -> HANDLE {
        self.file.as_raw_handle().cast()
    }

    pub(super) fn identity(&self) -> &ObjectIdentity {
        &self.identity
    }
}

#[derive(Debug)]
pub(super) struct VerifiedFile {
    file: File,
    identity: ObjectIdentity,
}

impl VerifiedFile {
    pub(super) fn identity(&self) -> &ObjectIdentity {
        &self.identity
    }

    pub(super) fn into_identity(self) -> ObjectIdentity {
        self.identity
    }

    pub(super) fn read_all(&mut self) -> Result<Vec<u8>> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        self.file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    pub(super) fn read_first_record(&mut self) -> Result<Vec<u8>> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![0; 4 * 1024];
        let read = self.file.read(&mut bytes)?;
        bytes.truncate(read);
        Ok(bytes)
    }

    pub(super) fn into_file(self) -> File {
        self.file
    }

    pub(super) fn handle(&self) -> HANDLE {
        self.file.as_raw_handle().cast()
    }
}

pub(super) fn open_root(
    path: &Path,
    desired_access: u32,
    share_access: u32,
) -> Result<VerifiedDirectory> {
    let wide = wide_nul(path)?;
    // SAFETY: the NUL-terminated path remains valid for this synchronous no-follow open.
    let handle = unsafe {
        windows_sys::Win32::Storage::FileSystem::CreateFileW(
            wide.as_ptr(),
            desired_access,
            share_access,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(PortableRuntimeError::new(
            "portable_root",
            "portable root could not be opened as a retained no-follow directory",
        ));
    }
    // SAFETY: successful CreateFileW transferred one owned file handle.
    let file = unsafe { File::from_raw_handle(handle.cast()) };
    clear_inheritance(&file)?;
    verified_directory(file)
}

pub(super) fn open_initial_file(path: &Path) -> Result<VerifiedFile> {
    let wide = wide_nul(path)?;
    // SAFETY: the NUL-terminated image path remains valid for this synchronous no-follow open.
    let handle = unsafe {
        windows_sys::Win32::Storage::FileSystem::CreateFileW(
            wide.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(PortableRuntimeError::new(
            "portable_runtime_paths",
            "running App image could not be opened without following links",
        ));
    }
    // SAFETY: successful CreateFileW transferred one owned file handle.
    let file = unsafe { File::from_raw_handle(handle.cast()) };
    clear_inheritance(&file)?;
    verified_file(file)
}

pub(super) fn open_relative_directory(
    parent: &VerifiedDirectory,
    name: &str,
    profile: RelativeDirectoryOpen,
) -> Result<VerifiedDirectory> {
    let policy = profile.native();
    verified_directory(open_relative(
        parent,
        name,
        policy,
        FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
    )?)
}

pub(super) fn open_relative_file(
    parent: &VerifiedDirectory,
    name: &str,
    profile: RelativeFileOpen,
) -> Result<VerifiedFile> {
    let policy = profile.native();
    verified_file(open_relative(
        parent,
        name,
        policy,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
    )?)
}

fn open_relative(
    parent: &VerifiedDirectory,
    name: &str,
    policy: NativeOpenPolicy,
    options: NTCREATEFILE_CREATE_OPTIONS,
) -> Result<File> {
    // NtCreateFile documents generic file rights, but an affected user-mode
    // environment returned STATUS_INVALID_PARAMETER until the equivalent
    // file-specific masks were supplied. Closed profiles make that observed,
    // deterministic behavior an explicit project policy.
    if policy.desired_access & GENERIC_ACCESS_MASK != 0 {
        return Err(PortableRuntimeError::new(
            "portable_object",
            "relative native open policy contained generic access rights",
        ));
    }
    let mut units = checked_leaf(name)?;
    let length =
        u16::try_from(units.len().saturating_mul(std::mem::size_of::<u16>())).map_err(|_| {
            PortableRuntimeError::new("portable_object", "relative object name overflow")
        })?;
    let unicode = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: units.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: OBJECT_ATTRIBUTES_BYTES,
        RootDirectory: parent.handle(),
        ObjectName: &raw const unicode,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut status = IO_STATUS_BLOCK::default();
    // SAFETY: fixed validated leaf UTF-16 and retained parent handle are live throughout this synchronous open.
    let status_code = unsafe {
        NtCreateFile(
            &raw mut handle,
            policy.desired_access,
            &raw const attributes,
            &raw mut status,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            policy.share_access,
            policy.disposition,
            options,
            std::ptr::null(),
            0,
        )
    };
    if status_code < 0 || handle.is_null() {
        return Err(PortableRuntimeError::new(
            "portable_object",
            format!("relative no-follow object open failed with NTSTATUS 0x{status_code:08x}"),
        ));
    }
    // SAFETY: successful NtCreateFile transferred one owned file handle.
    let file = unsafe { File::from_raw_handle(handle) };
    clear_inheritance(&file)?;
    Ok(file)
}

pub(super) fn information(handle: HANDLE) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: caller retains the live handle and supplies writable metadata.
    if unsafe { GetFileInformationByHandle(handle, &raw mut information) } == 0 {
        return Err(PortableRuntimeError::new(
            "portable_object",
            "no-follow object metadata could not be read",
        ));
    }
    Ok(information)
}

fn verified_directory(file: File) -> Result<VerifiedDirectory> {
    let identity = verify_and_identity(&file, true)?;
    Ok(VerifiedDirectory { file, identity })
}

fn verified_file(file: File) -> Result<VerifiedFile> {
    let identity = verify_and_identity(&file, false)?;
    Ok(VerifiedFile { file, identity })
}

fn verify_and_identity(file: &File, directory: bool) -> Result<ObjectIdentity> {
    let information = information(file.as_raw_handle().cast())?;
    let actual_directory = information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || actual_directory != directory
        || (!directory && information.nNumberOfLinks != 1)
    {
        return Err(PortableRuntimeError::new(
            "portable_object",
            "no-follow object was reparse, wrong type, or multi-link file",
        ));
    }
    Ok(ObjectIdentity(sha256_hex(
        format!(
            "renderpilot-portable-file-id-v1\\0{}\\0{}\\0{}",
            information.dwVolumeSerialNumber, information.nFileIndexHigh, information.nFileIndexLow
        )
        .as_bytes(),
    )))
}

fn clear_inheritance(file: &File) -> Result<()> {
    // SAFETY: clearing inheritance does not transfer the retained file handle.
    if unsafe { SetHandleInformation(file.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(PortableRuntimeError::new(
            "portable_object",
            "object handle inheritance could not be cleared",
        ));
    }
    Ok(())
}

fn checked_leaf(name: &str) -> Result<Vec<u16>> {
    if name.is_empty()
        || !name.is_ascii()
        || name.bytes().any(|byte| matches!(byte, b'\\' | b'/' | 0))
    {
        return Err(PortableRuntimeError::new(
            "portable_object",
            "relative object name was not a fixed leaf",
        ));
    }
    Ok(name.encode_utf16().collect())
}

fn wide_nul(path: &Path) -> Result<Vec<u16>> {
    if path.as_os_str().is_empty() {
        return Err(PortableRuntimeError::new(
            "portable_root",
            "portable root path was empty",
        ));
    }
    Ok(path.as_os_str().encode_wide().chain(Some(0)).collect())
}

#[cfg(test)]
mod access_profile_tests {
    use super::{GENERIC_ACCESS_MASK, RelativeDirectoryOpen, RelativeFileOpen};

    #[test]
    fn relative_native_profiles_use_only_preexpanded_access_rights() {
        for desired_access in [
            RelativeDirectoryOpen::Traverse.native().desired_access,
            RelativeDirectoryOpen::CreateBranch.native().desired_access,
            RelativeDirectoryOpen::CreateAndEnumerateFiles
                .native()
                .desired_access,
            RelativeFileOpen::SharedRead.native().desired_access,
            RelativeFileOpen::SharedCreateWriteAndReadAttributes
                .native()
                .desired_access,
            RelativeFileOpen::ExclusiveReadAndDelete
                .native()
                .desired_access,
            RelativeFileOpen::ExclusiveOpenOrCreateReadDataAndAttributes
                .native()
                .desired_access,
        ] {
            assert_eq!(desired_access & GENERIC_ACCESS_MASK, 0);
        }
    }
}
