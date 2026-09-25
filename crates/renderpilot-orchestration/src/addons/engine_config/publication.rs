//! Audited Engine.ini publication boundary.
//!
//! The generic retryable mutation machinery is intentionally not used here:
//! Engine.ini is shared user/game state and must be published only after its
//! pre-image has been revalidated.  The portable implementation uses a stage
//! created in the retained parent directory, `create_new`, flush, digest
//! verification, and a final no-clobber/identity check.

#[cfg(windows)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
#[cfg(windows)]
use std::io::{Read, Seek, SeekFrom};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::Arc;

use sha2::Digest;

/// Read-only preflight image for one target path.
#[derive(Debug, Clone)]
pub struct EngineIniPreflight {
    /// Target path.
    pub path: PathBuf,
    /// Exact bytes observed before publication.
    pub before: Option<Vec<u8>>,
    /// Exact bytes prepared for publication.
    pub after: Vec<u8>,
    /// Whether the target existed during preflight.
    pub target_existed: bool,
    /// Retained no-follow handle for the proven platform directory on Windows.
    /// All later publication work is scoped to this still-open directory
    /// proof; the path is only the compatibility spelling for the final
    /// Windows replacement primitive.
    #[cfg(windows)]
    parent_handle: Arc<File>,
    /// Stable volume/file identity of the existing target, when present.
    #[cfg(windows)]
    target_identity: Option<WindowsFileIdentity>,
}

/// Publication failure preserving the target whenever possible.
#[derive(Debug)]
pub enum EngineIniPublicationError {
    /// Target changed between preflight and publication.
    PreimageChanged,
    /// Parent or target is a reparse point.
    ReparsePoint(PathBuf),
    /// Stage or target I/O failed.
    Io(io::Error),
    /// Published bytes do not match the prepared image.
    VerificationFailed,
}

impl std::fmt::Display for EngineIniPublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreimageChanged => formatter.write_str("Engine.ini changed during publication"),
            Self::ReparsePoint(path) => {
                write!(formatter, "reparse point rejected: {}", path.display())
            }
            Self::Io(error) => write!(formatter, "Engine.ini publication I/O failed: {error}"),
            Self::VerificationFailed => {
                formatter.write_str("Engine.ini publication verification failed")
            }
        }
    }
}

impl std::error::Error for EngineIniPublicationError {}

impl From<io::Error> for EngineIniPublicationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Performs the read-only part of an Engine.ini operation.  It never creates
/// a directory or stage file.
pub fn preflight(
    path: &Path,
    after: Vec<u8>,
) -> Result<EngineIniPreflight, EngineIniPublicationError> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Engine.ini has no parent"))?;
    reject_reparse(parent)?;
    #[cfg(windows)]
    let parent_handle = open_directory_handle(parent)?;
    #[cfg(windows)]
    let leaf = target_leaf(path)?;
    #[cfg(windows)]
    let before = read_target_relative(&parent_handle, &leaf, path)?;
    #[cfg(not(windows))]
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(EngineIniPublicationError::ReparsePoint(path.to_path_buf()));
            }
            Some(fs::read(path)?)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    #[cfg(windows)]
    let target_identity = if before.is_some() {
        let target = open_existing_relative(&parent_handle, &leaf)?;
        Some(file_identity(&target)?)
    } else {
        None
    };
    Ok(EngineIniPreflight {
        target_existed: before.is_some(),
        path: path.to_path_buf(),
        before,
        after,
        #[cfg(windows)]
        parent_handle,
        #[cfg(windows)]
        target_identity,
    })
}

/// Portable publication implementation used on non-Windows development
/// hosts.  Windows uses the handle-relative implementation below.
#[cfg(not(windows))]
fn publish_path(
    preflight: &EngineIniPreflight,
    operation_id: &str,
) -> Result<(), EngineIniPublicationError> {
    let parent = preflight
        .path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Engine.ini has no parent"))?;
    reject_reparse(parent)?;
    let current = read_target(&preflight.path)?;
    if current.as_deref() != preflight.before.as_deref() {
        return Err(EngineIniPublicationError::PreimageChanged);
    }
    let stage_name = format!(
        ".renderpilot-engine-{}.stage",
        sanitize_operation_id(operation_id)
    );
    let stage = parent.join(stage_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)?;
    file.write_all(&preflight.after)?;
    file.sync_all()?;
    drop(file);
    if fs::read(&stage)? != preflight.after {
        let _ = fs::remove_file(&stage);
        return Err(EngineIniPublicationError::VerificationFailed);
    }
    // Stage creation itself can give another process time to edit or replace
    // the target.  Recheck the exact bytes immediately before the final
    // no-clobber/identity-preserving publication.
    if read_target(&preflight.path)?.as_deref() != preflight.before.as_deref() {
        let _ = fs::remove_file(&stage);
        return Err(EngineIniPublicationError::PreimageChanged);
    }
    // `rename` is no-clobber on Windows and fails if a target appeared after
    // the identity check.  A replacement is allowed only after a second exact
    // check, and is delegated to the platform-specific helper below.
    if preflight.target_existed {
        replace_existing(&stage, &preflight.path)?;
    } else if let Err(error) = fs::rename(&stage, &preflight.path) {
        let _ = fs::remove_file(&stage);
        return Err(error.into());
    }
    let published = fs::read(&preflight.path)?;
    if published != preflight.after {
        return Err(EngineIniPublicationError::VerificationFailed);
    }
    Ok(())
}

#[cfg(not(windows))]
/// Publishes through standard path-based stage and rename operations.
pub fn publish(
    preflight: &EngineIniPreflight,
    operation_id: &str,
) -> Result<(), EngineIniPublicationError> {
    publish_path(preflight, operation_id)
}

#[cfg(windows)]
/// Publishes through the retained platform-directory handle and native
/// relative stage/rename operations.
pub fn publish(
    preflight: &EngineIniPreflight,
    operation_id: &str,
) -> Result<(), EngineIniPublicationError> {
    let parent = preflight
        .path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Engine.ini has no parent"))?;
    reject_reparse(parent)?;
    let target_leaf = target_leaf(&preflight.path)?;
    let current = read_target_relative(&preflight.parent_handle, &target_leaf, &preflight.path)?;
    if current.as_deref() != preflight.before.as_deref() {
        return Err(EngineIniPublicationError::PreimageChanged);
    }
    let stage_name = format!(
        ".renderpilot-engine-{}.stage",
        sanitize_operation_id(operation_id)
    );
    let stage_leaf = stage_name.encode_utf16().collect::<Vec<_>>();
    let mut stage = create_stage_relative(&preflight.parent_handle, &stage_leaf)?;
    stage.write_all(&preflight.after)?;
    stage.sync_all()?;
    stage.seek(SeekFrom::Start(0))?;
    let mut staged = Vec::with_capacity(preflight.after.len());
    stage.read_to_end(&mut staged)?;
    if staged != preflight.after {
        discard_stage(&stage)?;
        return Err(EngineIniPublicationError::VerificationFailed);
    }
    if read_target_relative(&preflight.parent_handle, &target_leaf, &preflight.path)?.as_deref()
        != preflight.before.as_deref()
    {
        discard_stage(&stage)?;
        return Err(EngineIniPublicationError::PreimageChanged);
    }
    if preflight.target_existed {
        let target = open_existing_relative(&preflight.parent_handle, &target_leaf)?;
        if Some(file_identity(&target)?) != preflight.target_identity {
            discard_stage(&stage)?;
            return Err(EngineIniPublicationError::PreimageChanged);
        }
    }
    // Windows has no rename primitive conditioned on an arbitrary file ID.
    // The retained-handle checks remove ordinary path TOCTOU; a replacement
    // in the irreducible interval after the final check is detected by the
    // post-image read and leaves the journal in durable recovery state.
    if let Err(error) = set_stage_delete_on_close(&stage, false) {
        let _ = discard_stage(&stage);
        return Err(error.into());
    }
    if let Err(error) = rename_stage_relative(
        &stage,
        &preflight.parent_handle,
        &target_leaf,
        preflight.target_existed,
    ) {
        let _ = discard_stage(&stage);
        return Err(error.into());
    }
    let published = read_target_relative(&preflight.parent_handle, &target_leaf, &preflight.path)?;
    if published.as_deref() != Some(preflight.after.as_slice()) {
        return Err(EngineIniPublicationError::VerificationFailed);
    }
    Ok(())
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "NtCreateFile creates the private stage relative to the retained directory handle"
)]
fn create_stage_relative(parent: &File, leaf: &[u16]) -> io::Result<File> {
    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_CREATE, FILE_NON_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT,
                FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{
                GENERIC_READ, GENERIC_WRITE, HANDLE, OBJ_CASE_INSENSITIVE, UNICODE_STRING,
            },
            Storage::FileSystem::{
                DELETE, FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE, SYNCHRONIZE,
            },
        },
    };
    if leaf.is_empty() || leaf.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid stage leaf",
        ));
    }
    let byte_length = leaf
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "stage leaf is too long"))?;
    let name = UNICODE_STRING {
        Length: byte_length,
        MaximumLength: byte_length,
        Buffer: leaf.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &raw const name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut io_status = windows_sys::Win32::System::IO::IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            DELETE | GENERIC_READ | GENERIC_WRITE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            &raw const attributes,
            &raw mut io_status,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 || handle.is_null() {
        return Err(io::Error::other(format!(
            "stage create failed with NTSTATUS {status:#010x}"
        )));
    }
    let stage = unsafe { File::from_raw_handle(handle) };
    if let Err(error) = set_stage_delete_on_close(&stage, true) {
        let _ = discard_stage(&stage);
        return Err(error);
    }
    Ok(stage)
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowsFileIdentity {
    volume_serial: u32,
    file_index: u64,
}

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[cfg(windows)]
#[must_use]
fn has_reparse_attribute(attributes: u32) -> bool {
    attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "GetFileInformationByHandle captures target identity for publication CAS"
)]
fn file_identity(file: &File) -> io::Result<WindowsFileIdentity> {
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    let success =
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    let information = unsafe { information.assume_init() };
    Ok(WindowsFileIdentity {
        volume_serial: information.dwVolumeSerialNumber,
        file_index: (u64::from(information.nFileIndexHigh) << 32)
            | u64::from(information.nFileIndexLow),
    })
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "GetFileInformationByHandle rejects reparse handles before byte reads"
)]
fn ensure_handle_not_reparse(
    file: &File,
    display_path: &Path,
) -> Result<(), EngineIniPublicationError> {
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    let success =
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) };
    if success == 0 {
        return Err(io::Error::last_os_error().into());
    }
    let information = unsafe { information.assume_init() };
    if has_reparse_attribute(information.dwFileAttributes) {
        return Err(EngineIniPublicationError::ReparsePoint(
            display_path.to_path_buf(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn target_leaf(path: &Path) -> Result<Vec<u16>, EngineIniPublicationError> {
    Ok(path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid Engine.ini leaf"))?
        .encode_utf16()
        .collect())
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "NtCreateFile opens the deletion target relative to the retained directory handle"
)]
fn open_existing_relative(parent: &File, leaf: &[u16]) -> io::Result<File> {
    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
                FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{GENERIC_READ, HANDLE, OBJ_CASE_INSENSITIVE, UNICODE_STRING},
            Storage::FileSystem::{
                DELETE, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
                SYNCHRONIZE,
            },
        },
    };
    if leaf.is_empty() || leaf.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid target leaf",
        ));
    }
    let byte_length = leaf
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "target leaf is too long"))?;
    let name = UNICODE_STRING {
        Length: byte_length,
        MaximumLength: byte_length,
        Buffer: leaf.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &raw const name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut io_status = windows_sys::Win32::System::IO::IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            DELETE | GENERIC_READ | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            &raw const attributes,
            &raw mut io_status,
            std::ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 || handle.is_null() {
        // STATUS_OBJECT_NAME_NOT_FOUND and STATUS_NO_SUCH_FILE.
        if status == -1073741772 || status == -1073741825 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "target does not exist",
            ));
        }
        return Err(io::Error::other(format!(
            "target open failed with NTSTATUS {status:#010x}"
        )));
    }
    let file = unsafe { File::from_raw_handle(handle) };
    use std::os::windows::fs::MetadataExt;
    if has_reparse_attribute(file.metadata()?.file_attributes()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "reparse target rejected",
        ));
    }
    Ok(file)
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "NtSetInformationFile renames the private stage relative to the retained directory handle"
)]
fn rename_stage_relative(
    stage: &File,
    parent: &File,
    leaf: &[u16],
    replace_if_exists: bool,
) -> io::Result<()> {
    use std::mem::MaybeUninit;
    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
    };
    let name_bytes = leaf
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "target leaf is too long"))?;
    let required = std::mem::size_of::<FILE_RENAME_INFORMATION>()
        .checked_add(name_bytes as usize)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "rename buffer is too large"))?;
    let slots = (required as usize).div_ceil(std::mem::size_of::<FILE_RENAME_INFORMATION>());
    let mut storage = Vec::with_capacity(slots);
    storage.resize_with(slots, MaybeUninit::<FILE_RENAME_INFORMATION>::zeroed);
    let rename = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    unsafe {
        (*rename).Anonymous.ReplaceIfExists = replace_if_exists;
        (*rename).RootDirectory = parent.as_raw_handle();
        (*rename).FileNameLength = name_bytes;
        std::ptr::copy_nonoverlapping(
            leaf.as_ptr(),
            std::ptr::addr_of_mut!((*rename).FileName).cast::<u16>(),
            leaf.len(),
        );
        let mut io_status = windows_sys::Win32::System::IO::IO_STATUS_BLOCK::default();
        let status = NtSetInformationFile(
            stage.as_raw_handle(),
            &raw mut io_status,
            rename.cast(),
            required,
            FileRenameInformation,
        );
        if status < 0 || io_status.Anonymous.Status < 0 {
            return Err(io::Error::other(format!(
                "stage rename failed with NTSTATUS {status:#010x}"
            )));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn discard_stage(stage: &File) -> io::Result<()> {
    set_stage_delete_on_close(stage, true)
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "SetFileInformationByHandle controls delete-on-close for the private stage"
)]
fn set_stage_delete_on_close(stage: &File, delete: bool) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
    };
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: delete };
    let deleted = unsafe {
        SetFileInformationByHandle(
            stage.as_raw_handle(),
            FileDispositionInfo,
            (&raw const disposition).cast(),
            u32::try_from(std::mem::size_of::<FILE_DISPOSITION_INFO>()).expect("fits"),
        )
    };
    if deleted == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Deletes a RenderPilot-created empty target after a final exact digest
/// recheck.  Pre-existing files are never deleted through this function.
#[cfg(not(windows))]
fn delete_created_empty_path(
    path: &Path,
    expected: &[u8],
) -> Result<bool, EngineIniPublicationError> {
    reject_reparse(
        path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Engine.ini has no parent")
        })?,
    )?;
    let current = read_target(path)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Engine.ini disappeared before deletion",
        )
    })?;
    if current != expected {
        return Ok(false);
    }
    fs::remove_file(path)?;
    Ok(true)
}

#[cfg(not(windows))]
/// Deletes the created empty target through standard path-based operations if its bytes match.
pub fn delete_created_empty(
    path: &Path,
    expected: &[u8],
) -> Result<bool, EngineIniPublicationError> {
    delete_created_empty_path(path, expected)
}

#[cfg(windows)]
/// Deletes the target through a handle opened relative to the retained,
/// non-reparse platform directory.  The bytes are checked while that target
/// handle is live; no path-based remove can race a replacement.
pub fn delete_created_empty(
    path: &Path,
    expected: &[u8],
) -> Result<bool, EngineIniPublicationError> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Engine.ini has no parent"))?;
    reject_reparse(parent)?;
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(EngineIniPublicationError::ReparsePoint(path.to_path_buf()));
    }
    let parent_handle = open_directory_handle(parent)?;
    let leaf = target_leaf(path)?;
    let mut target = open_existing_relative(&parent_handle, &leaf)?;
    target.seek(SeekFrom::Start(0))?;
    let mut current = Vec::new();
    target.read_to_end(&mut current)?;
    if current != expected {
        return Ok(false);
    }
    discard_stage(&target)?;
    Ok(true)
}

/// Removes a private stage left by a crashed publication only when it is a
/// regular, non-reparse file with the exact expected post-image digest.
///
/// A missing stage is harmless.  Any unexpected bytes are preserved and
/// reported as `false`, so recovery never destroys a possible foreign file.
pub fn cleanup_owned_stage(
    target: &Path,
    stage_name: &str,
    expected_digest: &str,
) -> Result<bool, EngineIniPublicationError> {
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Engine.ini has no parent"))?;
    reject_reparse(parent)?;
    if stage_name.is_empty()
        || stage_name
            != Path::new(stage_name)
                .file_name()
                .and_then(|leaf| leaf.to_str())
                .unwrap_or_default()
    {
        return Err(EngineIniPublicationError::ReparsePoint(
            parent.join(stage_name),
        ));
    }
    let stage_path = parent.join(stage_name);
    let metadata = match fs::symlink_metadata(&stage_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if has_reparse_attribute(metadata.file_attributes()) {
            return Ok(false);
        }
    }

    #[cfg(not(windows))]
    {
        let bytes = fs::read(&stage_path)?;
        if digest_bytes(&bytes) != expected_digest {
            return Ok(false);
        }
        fs::remove_file(stage_path)?;
        Ok(true)
    }

    #[cfg(windows)]
    {
        let parent_handle = open_directory_handle(parent)?;
        let leaf = stage_name.encode_utf16().collect::<Vec<_>>();
        let mut stage = match open_existing_relative(&parent_handle, &leaf) {
            Ok(stage) => stage,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        stage.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        stage.read_to_end(&mut bytes)?;
        if digest_bytes(&bytes) != expected_digest {
            return Ok(false);
        }
        discard_stage(&stage)?;
        Ok(true)
    }
}

#[cfg(windows)]
fn read_target_relative(
    parent: &File,
    leaf: &[u16],
    display_path: &Path,
) -> Result<Option<Vec<u8>>, EngineIniPublicationError> {
    let mut target = match open_existing_relative(parent, leaf) {
        Ok(target) => target,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure_handle_not_reparse(&target, display_path)?;
    target.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    target.read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

#[cfg(not(windows))]
fn read_target(path: &Path) -> Result<Option<Vec<u8>>, EngineIniPublicationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(EngineIniPublicationError::ReparsePoint(path.to_path_buf()));
            }
            Ok(Some(fs::read(path)?))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn reject_reparse(path: &Path) -> Result<(), EngineIniPublicationError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(EngineIniPublicationError::ReparsePoint(path.to_path_buf()));
    }
    Ok(())
}

#[cfg(windows)]
fn open_directory_handle(path: &Path) -> Result<Arc<File>, EngineIniPublicationError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, SYNCHRONIZE,
    };

    let directory = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .access_mode(FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = directory.metadata()?;
    use std::os::windows::fs::MetadataExt;
    if !metadata.is_dir() || has_reparse_attribute(metadata.file_attributes()) {
        return Err(EngineIniPublicationError::ReparsePoint(path.to_path_buf()));
    }
    Ok(Arc::new(directory))
}

fn sanitize_operation_id(operation_id: &str) -> String {
    let sanitized = operation_id
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
                byte as char
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "operation".to_owned()
    } else {
        sanitized
    }
}

#[cfg(not(windows))]
fn replace_existing(stage: &Path, target: &Path) -> Result<(), EngineIniPublicationError> {
    fs::rename(stage, target)?;
    Ok(())
}

/// Computes a digest for publication tests and service adapters.
#[must_use]
pub fn digest_bytes(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(windows)]
    #[test]
    fn reparse_attribute_is_always_rejected() {
        assert!(!has_reparse_attribute(0));
        assert!(has_reparse_attribute(FILE_ATTRIBUTE_REPARSE_POINT));
        assert!(has_reparse_attribute(FILE_ATTRIBUTE_REPARSE_POINT | 0x20));
    }

    #[test]
    fn preflight_is_read_only_and_publish_rechecks_preimage() {
        let temp = tempdir().expect("temp");
        let path = temp.path().join("Engine.ini");
        let planned =
            preflight(&path, b"[SystemSettings]\r\nr.X=1\r\n".to_vec()).expect("preflight");
        assert!(!path.exists());
        publish(&planned, "test-operation").expect("publish");
        assert_eq!(fs::read(&path).expect("read"), planned.after);
    }

    #[test]
    fn preflight_rejects_changed_target() {
        let temp = tempdir().expect("temp");
        let path = temp.path().join("Engine.ini");
        fs::write(&path, b"before").expect("write");
        let planned = preflight(&path, b"after".to_vec()).expect("preflight");
        fs::write(&path, b"foreign").expect("foreign");
        assert!(matches!(
            publish(&planned, "test-operation"),
            Err(EngineIniPublicationError::PreimageChanged)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn publish_rejects_a_replaced_target_even_when_bytes_are_unchanged() {
        let temp = tempdir().expect("temp");
        let path = temp.path().join("Engine.ini");
        fs::write(&path, b"before").expect("write");
        let planned = preflight(&path, b"after".to_vec()).expect("preflight");
        fs::remove_file(&path).expect("remove original target");
        fs::write(&path, b"before").expect("replace target");

        assert!(matches!(
            publish(&planned, "replaced-target"),
            Err(EngineIniPublicationError::PreimageChanged)
        ));
        assert_eq!(fs::read(&path).expect("replacement remains"), b"before");
    }

    #[test]
    fn stage_collision_is_never_clobbered() {
        let temp = tempdir().expect("temp");
        let path = temp.path().join("Engine.ini");
        let planned = preflight(&path, b"after".to_vec()).expect("preflight");
        let stage = temp.path().join(".renderpilot-engine-collision.stage");
        fs::write(&stage, b"foreign-stage").expect("foreign stage");

        assert!(publish(&planned, "collision").is_err());
        assert_eq!(fs::read(stage).expect("stage remains"), b"foreign-stage");
    }

    #[cfg(unix)]
    #[test]
    fn preflight_rejects_a_reparse_parent_before_reading_or_staging() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("temp");
        let real = temp.path().join("real");
        let link = temp.path().join("link");
        fs::create_dir(&real).expect("real");
        symlink(&real, &link).expect("link");
        assert!(matches!(
            preflight(&link.join("Engine.ini"), b"after".to_vec()),
            Err(EngineIniPublicationError::ReparsePoint(_))
        ));
        assert!(!link.join("Engine.ini").exists());
    }
}
