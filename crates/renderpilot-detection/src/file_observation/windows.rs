//! Strong file observation for Windows using a held lease, FILE_ID_INFO, and USN.

use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

use renderpilot_application::AppResult;
use renderpilot_domain::Sha256Hash;

use super::common::{read_and_hash, unavailable_or_error, unavailable_probe};
use super::{
    FileIdentityProbeResult, FileObservationResult, StableFileSnapshot, StrongFileCacheKey,
};
#[cfg(test)]
use super::{FileObservationSource, SystemFileObservationSource};

pub(super) fn observe_system_file(path: &Path) -> AppResult<FileObservationResult> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let mut file = match OpenOptions::new()
        .read(true)
        // A held read lease denies concurrent writes and deletes. If a process
        // cannot give this guarantee, the full observation is unavailable.
        .share_mode(FILE_SHARE_READ)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(FileObservationResult::Missing);
        }
        Err(error) => return Ok(unavailable_or_error(path, error)),
    };
    let before = match windows_lease_state(&file) {
        Some(state) => state,
        None => return Ok(FileObservationResult::Unavailable),
    };
    let journal = WindowsUsnWindow::begin(path);
    let before_key = journal
        .as_ref()
        .and_then(|journal| journal.file_material(&file, before.size));
    let (bytes, sha256) = match read_and_hash(&mut file, path) {
        Ok(value) => value,
        Err(error) => return Ok(unavailable_or_error(path, error)),
    };
    let after = match windows_lease_state(&file) {
        Some(state) => state,
        None => return Ok(FileObservationResult::Unavailable),
    };
    let after_key = journal
        .as_ref()
        .and_then(|journal| journal.file_material(&file, after.size));
    let reopened = match OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
    {
        Ok(reopened) => reopened,
        Err(error) => return Ok(unavailable_or_error(path, error)),
    };
    let reopened_state = match windows_lease_state(&reopened) {
        Some(state) => state,
        None => return Ok(FileObservationResult::Unavailable),
    };
    let reopened_key = journal
        .as_ref()
        .and_then(|journal| journal.file_material(&reopened, reopened_state.size));
    let keys = journal.map_or([None, None, None], |journal| {
        journal.finish([before_key, after_key, reopened_key])
    });
    Ok(finish_windows_observation(WindowsObservation {
        before: &before,
        after: &after,
        reopened: &reopened_state,
        keys,
        bytes,
        sha256,
    }))
}

pub(super) fn probe_system_identity(path: &Path) -> AppResult<FileIdentityProbeResult> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let file = match OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(FileIdentityProbeResult::Missing);
        }
        Err(error) => return Ok(unavailable_probe(path, error)),
    };
    let before = match windows_lease_state(&file) {
        Some(state) => state,
        None => return Ok(FileIdentityProbeResult::Unavailable),
    };
    let journal = WindowsUsnWindow::begin(path);
    let before_key = journal
        .as_ref()
        .and_then(|journal| journal.file_material(&file, before.size));
    let after = match windows_lease_state(&file) {
        Some(state) => state,
        None => return Ok(FileIdentityProbeResult::Unavailable),
    };
    let after_key = journal
        .as_ref()
        .and_then(|journal| journal.file_material(&file, after.size));
    let reopened = match OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
    {
        Ok(reopened) => reopened,
        Err(error) => return Ok(unavailable_probe(path, error)),
    };
    let reopened_state = match windows_lease_state(&reopened) {
        Some(state) => state,
        None => return Ok(FileIdentityProbeResult::Unavailable),
    };
    let reopened_key = journal
        .as_ref()
        .and_then(|journal| journal.file_material(&reopened, reopened_state.size));
    if before != after || before != reopened_state {
        return Ok(FileIdentityProbeResult::Unavailable);
    }
    let [before_key, after_key, reopened_key] = journal.map_or([None, None, None], |journal| {
        journal.finish([before_key, after_key, reopened_key])
    });
    match (before_key, after_key, reopened_key) {
        (Some(before), Some(after), Some(reopened)) if before == after && before == reopened => {
            Ok(FileIdentityProbeResult::Available(before))
        }
        _ => Ok(FileIdentityProbeResult::Uncacheable),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsLeaseIdentity {
    volume_serial: u64,
    file_id: [u8; 16],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsLeaseState {
    size: u64,
}

fn windows_lease_state(file: &File) -> Option<WindowsLeaseState> {
    use std::os::windows::fs::MetadataExt;

    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return None;
    }
    Some(WindowsLeaseState {
        size: metadata.len(),
    })
}

#[allow(
    unsafe_code,
    reason = "windows-sys exposes FileIdInfo only as an unsafe handle API"
)]
fn windows_file_identity(file: &File) -> Option<WindowsLeaseIdentity> {
    use std::{mem::size_of, os::windows::io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx},
    };

    windows_lease_state(file)?;
    let mut info = FILE_ID_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as HANDLE,
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            u32::try_from(size_of::<FILE_ID_INFO>()).ok()?,
        )
    } == 0
    {
        return None;
    }
    Some(WindowsLeaseIdentity {
        volume_serial: info.VolumeSerialNumber,
        file_id: info.FileId.Identifier,
    })
}

struct WindowsObservation<'a> {
    before: &'a WindowsLeaseState,
    after: &'a WindowsLeaseState,
    reopened: &'a WindowsLeaseState,
    keys: [Option<StrongFileCacheKey>; 3],
    bytes: Vec<u8>,
    sha256: Sha256Hash,
}

fn finish_windows_observation(observation: WindowsObservation<'_>) -> FileObservationResult {
    let WindowsObservation {
        before,
        after,
        reopened,
        keys: [before_key, after_key, reopened_key],
        bytes,
        sha256,
    } = observation;
    if before != after || before != reopened {
        return FileObservationResult::Unavailable;
    }
    let cache_key = match (before_key, after_key, reopened_key) {
        (Some(before), Some(after), Some(reopened)) if before == after && before == reopened => {
            Some(before)
        }
        // FILE_ID_INFO and USN are optional for a held stable read but
        // mandatory for reuse. Do not manufacture a weak substitute.
        _ => None,
    };
    FileObservationResult::Available(StableFileSnapshot {
        cache_key,
        sha256,
        bytes,
    })
}

#[derive(Debug, Clone, Copy)]
struct UsnJournalBounds {
    id: u64,
    first: i64,
    next: i64,
}

impl UsnJournalBounds {
    fn contains(self, usn: i64) -> bool {
        usn >= self.first && usn < self.next
    }
}

struct WindowsUsnWindow {
    volume: File,
    before: UsnJournalBounds,
}

impl WindowsUsnWindow {
    /// Opens the volume once and brackets every identity probe in this file
    /// observation with one journal-generation window.
    fn begin(path: &Path) -> Option<Self> {
        let volume = open_volume_for(path)?;
        let before = query_usn_journal(&volume)?;
        Some(Self { volume, before })
    }

    fn file_material(&self, file: &File, size: u64) -> Option<WindowsCacheKeyMaterial> {
        Some(WindowsCacheKeyMaterial {
            identity: windows_file_identity(file)?,
            usn: read_file_usn(file)?,
            size,
        })
    }

    fn finish(
        self,
        materials: [Option<WindowsCacheKeyMaterial>; 3],
    ) -> [Option<StrongFileCacheKey>; 3] {
        let Some(after) = query_usn_journal(&self.volume) else {
            return [None, None, None];
        };
        strong_cache_keys_for_window(self.before, after, materials)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsCacheKeyMaterial {
    identity: WindowsLeaseIdentity,
    usn: i64,
    size: u64,
}

fn strong_cache_keys_for_window(
    before: UsnJournalBounds,
    after: UsnJournalBounds,
    materials: [Option<WindowsCacheKeyMaterial>; 3],
) -> [Option<StrongFileCacheKey>; 3] {
    if before.id == 0 || before.id != after.id {
        return [None, None, None];
    }
    materials.map(|material| {
        let material = material?;
        if !before.contains(material.usn) || !after.contains(material.usn) {
            return None;
        }
        Some(StrongFileCacheKey {
            kind: "windows_file_id_usn_v2".to_owned(),
            object_identity: format!(
                "{:016x}:{}",
                material.identity.volume_serial,
                hex::encode(material.identity.file_id)
            ),
            change_token: format!("{:016x}:{:016x}", before.id, material.usn),
            size: material.size,
        })
    })
}

#[allow(
    unsafe_code,
    reason = "windows-sys exposes the read-only journal query only as an unsafe handle API"
)]
fn query_usn_journal(volume: &File) -> Option<UsnJournalBounds> {
    use std::{
        ffi::c_void,
        mem::size_of,
        os::windows::io::AsRawHandle,
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::HANDLE,
        System::{
            IO::DeviceIoControl,
            Ioctl::{FSCTL_QUERY_USN_JOURNAL, USN_JOURNAL_DATA_V2},
        },
    };

    let mut journal = USN_JOURNAL_DATA_V2::default();
    let mut returned = 0_u32;
    let ok = unsafe {
        DeviceIoControl(
            volume.as_raw_handle() as HANDLE,
            FSCTL_QUERY_USN_JOURNAL,
            null(),
            0,
            (&mut journal as *mut USN_JOURNAL_DATA_V2).cast::<c_void>(),
            u32::try_from(size_of::<USN_JOURNAL_DATA_V2>()).ok()?,
            &mut returned,
            null_mut(),
        )
    };
    let min = u32::try_from(size_of::<u64>() + 4 * size_of::<i64>()).ok()?;
    if ok == 0 || returned < min || journal.UsnJournalID == 0 {
        return None;
    }
    let first = journal.FirstUsn.max(journal.LowestValidUsn);
    (first >= 0 && journal.NextUsn > first).then_some(UsnJournalBounds {
        id: journal.UsnJournalID,
        first,
        next: journal.NextUsn,
    })
}

#[allow(
    unsafe_code,
    reason = "windows-sys exposes the read-only per-file USN query only as an unsafe handle API"
)]
fn read_file_usn(file: &File) -> Option<i64> {
    use std::{ffi::c_void, mem::size_of, os::windows::io::AsRawHandle, ptr::null_mut};
    use windows_sys::Win32::{
        Foundation::HANDLE,
        System::{
            IO::DeviceIoControl,
            Ioctl::{FSCTL_READ_FILE_USN_DATA, READ_FILE_USN_DATA},
        },
    };

    let input = READ_FILE_USN_DATA {
        MinMajorVersion: 2,
        MaxMajorVersion: 3,
    };
    let mut output = [0_u8; 128];
    let mut returned = 0_u32;
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as HANDLE,
            FSCTL_READ_FILE_USN_DATA,
            (&input as *const READ_FILE_USN_DATA).cast::<c_void>(),
            u32::try_from(size_of::<READ_FILE_USN_DATA>()).ok()?,
            output.as_mut_ptr().cast::<c_void>(),
            u32::try_from(output.len()).ok()?,
            &mut returned,
            null_mut(),
        )
    };
    usn_output_value(ok != 0, &output, returned)
}

fn open_volume_for(path: &Path) -> Option<File> {
    use std::{os::windows::fs::OpenOptionsExt, path::Component};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let Component::Prefix(prefix) = path.components().next()? else {
        return None;
    };
    let drive = prefix.as_os_str().to_string_lossy();
    if drive.len() != 2 || !drive.ends_with(':') {
        return None;
    }
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(format!(r"\\.\{drive}"))
        .ok()
}

fn usn_record_value(bytes: &[u8]) -> Option<i64> {
    // FSCTL_READ_FILE_USN_DATA returns a single V2 or V3 USN record. Parse
    // the wire layout directly so malformed/truncated output is uncacheable.
    let record_length =
        usize::try_from(u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?)).ok()?;
    let major = u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?);
    let offset = match major {
        2 => 24,
        3 => 40,
        _ => return None,
    };
    if record_length < offset + 8 || record_length > bytes.len() {
        return None;
    }
    Some(i64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn usn_output_value(call_succeeded: bool, output: &[u8], returned: u32) -> Option<i64> {
    if !call_succeeded {
        return None;
    }
    let returned = usize::try_from(returned).ok()?;
    let output = output.get(..returned)?;
    usn_record_value(output)
}

#[cfg(test)]
mod tests;
