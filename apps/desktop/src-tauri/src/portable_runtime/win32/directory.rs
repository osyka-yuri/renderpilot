#![expect(
    unsafe_code,
    reason = "D17 pins authority directories and classifies raw Win32 names through the bounded no-follow API boundary"
)]

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::Path,
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FindClose,
        FindFirstFileW, FindNextFileW, GetFileInformationByHandle, OPEN_EXISTING, WIN32_FIND_DATAW,
    },
};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    win32::process::path_wide_nul,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceScan {
    entries: BTreeMap<Vec<u16>, EntryKind>,
}

impl NamespaceScan {
    pub fn entries(&self) -> &BTreeMap<Vec<u16>, EntryKind> {
        &self.entries
    }
}

/// Creates the leaf when absent, then pins it with a no-follow directory
/// handle. Reparse points, non-directories, and unopenable leaves are retained
/// and fail closed rather than being repaired or removed.
pub fn ensure_plain_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    verify_directory_leaf(path)
}

/// D17 Scan A/B: enumerate raw UTF-16 names twice, classify every entry via a
/// no-follow handle, and reject any visibility or metadata delta.
pub fn stable_scan(path: &Path) -> Result<NamespaceScan> {
    stable_scan_skipping(path, &[])
}

/// Returns the stable NT file identity of a directory after opening its leaf
/// with `OPEN_REPARSE_POINT`.  Callers use this instead of textual paths when
/// binding portable-root and selected-generation authority.
pub fn directory_identity_digest_no_reparse(path: &Path) -> Result<String> {
    identity_digest_no_reparse(path, EntryKind::Directory)
}

/// Returns the stable NT file identity of an ordinary file after a no-follow
/// open.  A reparse point, unexpected object type, or a hard-linked authority
/// file fails closed.
pub fn file_identity_digest_no_reparse(path: &Path) -> Result<String> {
    identity_digest_no_reparse(path, EntryKind::File)
}

/// Re-opens an authority directory solely to prove its leaf is still a plain
/// directory.  The caller must separately retain any exclusive lock it owns.
pub fn verify_directory_no_reparse(path: &Path) -> Result<()> {
    verify_directory_leaf(path)
}

/// A retained share-zero admission handle cannot be reopened while it is doing
/// its job. Its raw leaf is therefore classified from the same A/B snapshots
/// but not reopened; the retained handle remains the pinned authority.
pub fn stable_scan_skipping(path: &Path, skip_open: &[&str]) -> Result<NamespaceScan> {
    let first = scan_once(path, skip_open)?;
    let second = scan_once(path, skip_open)?;
    if first != second {
        return Err(PortableRuntimeError::new(
            "portable_namespace_unstable",
            "raw namespace scan A/B differed",
        ));
    }
    Ok(first)
}

pub fn expect_exact_addition(
    before: &NamespaceScan,
    after: &NamespaceScan,
    name: &str,
    kind: EntryKind,
) -> Result<()> {
    let raw_name = OsStr::new(name).encode_wide().collect::<Vec<_>>();
    let mut expected = before.entries.clone();
    if expected.insert(raw_name, kind).is_some() || expected != after.entries {
        return Err(PortableRuntimeError::new(
            "portable_namespace_publication",
            "publication delta was not one exact new leaf",
        ));
    }
    Ok(())
}

pub fn require_known_entries_skipping(
    root: &Path,
    scan: &NamespaceScan,
    allowed: &[(&str, EntryKind)],
    skip_open: &[&str],
) -> Result<()> {
    for (raw_name, kind) in scan.entries() {
        let name = String::from_utf16(raw_name).map_err(|_| {
            PortableRuntimeError::new(
                "portable_namespace_unknown",
                "entry name was not valid UTF-16",
            )
        })?;
        let expected = allowed
            .iter()
            .find(|(allowed_name, _)| *allowed_name == name)
            .map(|(_, kind)| *kind)
            .ok_or_else(|| {
                PortableRuntimeError::new(
                    "portable_namespace_unknown",
                    "namespace contained an unrecognized leaf",
                )
            })?;
        if expected != *kind {
            return Err(PortableRuntimeError::new(
                "portable_namespace_unknown",
                "known namespace leaf had the wrong object type",
            ));
        }
        if !skip_open.iter().any(|candidate| *candidate == name) {
            verify_entry_leaf(&root.join(std::ffi::OsString::from_wide(raw_name)), *kind)?;
        }
    }
    Ok(())
}

fn scan_once(root: &Path, skip_open: &[&str]) -> Result<NamespaceScan> {
    verify_directory_leaf(root)?;
    let pattern = path_wide_nul(&root.join("*"));
    let mut data = WIN32_FIND_DATAW::default();
    let handle = unsafe { FindFirstFileW(pattern.as_ptr(), &raw mut data) };
    if handle == INVALID_HANDLE_VALUE {
        let error = unsafe { GetLastError() };
        if error == ERROR_FILE_NOT_FOUND {
            return Ok(NamespaceScan {
                entries: BTreeMap::new(),
            });
        }
        return Err(PortableRuntimeError::new(
            "portable_namespace_unopenable",
            format!("FindFirstFileW failed: {error}"),
        ));
    }
    let result = (|| {
        let mut entries = BTreeMap::new();
        loop {
            if let Some(raw_name) = raw_name(&data)? {
                let kind = classify_find_data(&data)?;
                let path = root.join(std::ffi::OsString::from_wide(&raw_name));
                let skip = String::from_utf16(&raw_name)
                    .ok()
                    .is_some_and(|name| skip_open.iter().any(|candidate| *candidate == name));
                if !skip {
                    verify_entry_leaf(&path, kind)?;
                }
                if entries.insert(raw_name, kind).is_some() {
                    return Err(PortableRuntimeError::new(
                        "portable_namespace_unknown",
                        "raw namespace contained a duplicate name",
                    ));
                }
            }
            if unsafe { FindNextFileW(handle, &raw mut data) } == 0 {
                let error = unsafe { GetLastError() };
                if error == ERROR_NO_MORE_FILES {
                    break;
                }
                return Err(PortableRuntimeError::new(
                    "portable_namespace_unopenable",
                    format!("FindNextFileW failed: {error}"),
                ));
            }
        }
        Ok(NamespaceScan { entries })
    })();
    unsafe {
        let _ = FindClose(handle);
    }
    result
}

fn raw_name(data: &WIN32_FIND_DATAW) -> Result<Option<Vec<u16>>> {
    let length = data
        .cFileName
        .iter()
        .position(|unit| *unit == 0)
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_namespace_unknown", "raw name was unterminated")
        })?;
    let name = data.cFileName[..length].to_vec();
    if name == [b'.' as u16] || name == [b'.' as u16, b'.' as u16] {
        return Ok(None);
    }
    if name.is_empty()
        || name
            .iter()
            .any(|unit| *unit == b'\\' as u16 || *unit == b'/' as u16)
    {
        return Err(PortableRuntimeError::new(
            "portable_namespace_unknown",
            "raw name was empty or non-leaf",
        ));
    }
    Ok(Some(name))
}

fn classify_find_data(data: &WIN32_FIND_DATAW) -> Result<EntryKind> {
    if data.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(PortableRuntimeError::new(
            "portable_namespace_reparse",
            "namespace contained a reparse point",
        ));
    }
    if data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        Ok(EntryKind::Directory)
    } else {
        Ok(EntryKind::File)
    }
}

fn verify_directory_leaf(path: &Path) -> Result<()> {
    verify_entry_leaf(path, EntryKind::Directory)
}

fn verify_entry_leaf(path: &Path, expected: EntryKind) -> Result<()> {
    let wide = path_wide_nul(path);
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT
                | if expected == EntryKind::Directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(PortableRuntimeError::new(
            "portable_namespace_unopenable",
            "namespace leaf could not be opened without following links",
        ));
    }
    let result = verify_handle(handle, expected);
    unsafe {
        let _ = CloseHandle(handle);
    }
    result
}

fn identity_digest_no_reparse(path: &Path, expected: EntryKind) -> Result<String> {
    let wide = path_wide_nul(path);
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT
                | if expected == EntryKind::Directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(PortableRuntimeError::new(
            "portable_namespace_unopenable",
            "authority identity leaf could not be opened without following links",
        ));
    }
    let result = (|| {
        verify_handle(handle, expected)?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        if unsafe { GetFileInformationByHandle(handle, &raw mut info) } == 0 {
            return Err(PortableRuntimeError::new(
                "portable_namespace_unopenable",
                "could not read authority identity metadata",
            ));
        }
        Ok(crate::portable_runtime::signature::sha256_hex(
            format!(
                "renderpilot-portable-file-id-v1\\0{}\\0{}\\0{}",
                info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
            )
            .as_bytes(),
        ))
    })();
    unsafe {
        let _ = CloseHandle(handle);
    }
    result
}

fn verify_handle(handle: HANDLE, expected: EntryKind) -> Result<()> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(handle, &raw mut info) } == 0 {
        return Err(PortableRuntimeError::new(
            "portable_namespace_unopenable",
            "could not read pinned leaf metadata",
        ));
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(PortableRuntimeError::new(
            "portable_namespace_reparse",
            "pinned namespace leaf was a reparse point",
        ));
    }
    let actual = if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        EntryKind::Directory
    } else {
        EntryKind::File
    };
    if actual != expected {
        return Err(PortableRuntimeError::new(
            "portable_namespace_unknown",
            "pinned namespace leaf changed object type",
        ));
    }
    if actual == EntryKind::File && info.nNumberOfLinks != 1 {
        return Err(PortableRuntimeError::new(
            "portable_namespace_multilink",
            "authority file had multiple hard links",
        ));
    }
    Ok(())
}

/// Verifies the already-retained admission authority rather than reopening it
/// with a share mode that the live lock deliberately forbids.
pub fn verify_admission_handle(handle: HANDLE) -> Result<()> {
    verify_handle(handle, EntryKind::File)
}
