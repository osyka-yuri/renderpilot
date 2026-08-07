//! Stable cache snapshots and platform file identity.

#[cfg(target_os = "linux")]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::{
    fs,
    io::{self, Read},
    path::Path,
};
#[cfg(windows)]
use std::{
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    thread,
    time::Duration,
};

use sha2::{Digest, Sha256};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
    GetFileInformationByHandle,
};

use crate::ServiceError;

use super::contract::{CacheFileIdentity, CacheGenerationState};
use super::{CACHE_CHURN_RETRIES, CacheGeneration};

/// Reads the active cache exactly once through one file handle and derives a
/// generation from the bytes read from that handle. The caller already owns the
/// cache lease.
#[derive(Debug)]
pub(super) struct CacheFileSnapshot {
    pub(super) owner: RetainedCacheFile,
    pub(super) bytes: Vec<u8>,
    pub(super) metadata: fs::Metadata,
    pub(super) generation: CacheGeneration,
}

/// Holds the exact snapshot descriptor even on targets where retirement is
/// intentionally non-deleting. Keeping that owner structural preserves the
/// observation lifetime without target-specific lint suppression.
#[derive(Debug)]
pub(super) struct RetainedCacheFile(fs::File);

impl RetainedCacheFile {
    pub(super) fn retain(&self) {
        let _ = &self.0;
    }

    #[cfg(windows)]
    pub(super) fn file(&self) -> &fs::File {
        &self.0
    }
}

pub(super) fn read_cache_file_locked(
    path: &Path,
) -> Result<Option<CacheFileSnapshot>, ServiceError> {
    let Some(mut file) = open_cache_file_locked(path)? else {
        return Ok(None);
    };
    let initial_metadata = file.metadata().map_err(|error| {
        crate::failed(format!(
            "failed to inspect cache `{}`: {error}",
            path.display()
        ))
    })?;
    if !cache_metadata_is_regular(&initial_metadata) {
        return Err(crate::failed(format!(
            "cache `{}` was not a regular non-link file",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(initial_metadata.len()).unwrap_or(0));
    file.read_to_end(&mut bytes).map_err(|error| {
        crate::failed(format!(
            "failed to read cache `{}`: {error}",
            path.display()
        ))
    })?;
    let metadata = file.metadata().map_err(|error| {
        crate::failed(format!(
            "failed to inspect completed cache read `{}`: {error}",
            path.display()
        ))
    })?;
    if !cache_metadata_is_regular(&metadata) {
        return Err(crate::failed(format!(
            "cache `{}` changed to a non-regular object while being read",
            path.display()
        )));
    }
    let file_identity = cache_file_identity(&file, &metadata).map_err(|error| {
        crate::failed(format!(
            "failed to identify cache `{}`: {error}",
            path.display()
        ))
    })?;
    let generation = CacheGeneration(CacheGenerationState::Present {
        file_identity,
        length: bytes.len() as u64,
        sha256: Sha256::digest(&bytes).into(),
    });
    Ok(Some(CacheFileSnapshot {
        owner: RetainedCacheFile(file),
        bytes,
        metadata,
        generation,
    }))
}

pub(super) fn stable_cache_snapshot(
    path: &Path,
) -> Result<Option<CacheFileSnapshot>, ServiceError> {
    #[cfg(windows)]
    {
        // The no-share handle is the final cache proof. A second pathname
        // lookup would race the exact object this handle already pins.
        read_cache_file_locked(path)
    }
    #[cfg(not(windows))]
    {
        for _ in 0..=CACHE_CHURN_RETRIES {
            let current = read_cache_file_locked(path)?;
            let recheck = read_cache_file_locked(path)?;
            if current.is_none() && recheck.is_none() {
                return Ok(None);
            }
            if current
                .as_ref()
                .zip(recheck.as_ref())
                .is_some_and(|(current, recheck)| current.generation == recheck.generation)
            {
                return Ok(current);
            }
        }
        Err(cache_churn_error(path))
    }
}

#[cfg(not(windows))]
pub(super) fn snapshot_still_current(
    path: &Path,
    snapshot: &CacheFileSnapshot,
) -> Result<bool, ServiceError> {
    Ok(read_cache_file_locked(path)?
        .is_some_and(|current| current.generation == snapshot.generation))
}

pub(super) fn cache_churn_error(path: &Path) -> ServiceError {
    crate::failed(format!(
        "cache `{}` changed repeatedly during a protected transaction; refusing to overwrite an unverified successor",
        path.display()
    ))
}

fn open_cache_file_no_follow(path: &Path) -> io::Result<fs::File> {
    #[cfg(windows)]
    {
        fs::OpenOptions::new()
            .read(true)
            .access_mode(FILE_GENERIC_READ | DELETE)
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }
    #[cfg(target_os = "linux")]
    {
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(
                i32::try_from(rustix::fs::OFlags::NOFOLLOW.bits()).expect("O_NOFOLLOW fits i32"),
            )
            .open(path)
    }
    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    {
        fs::File::open(path)
    }
}

fn open_cache_file_locked(path: &Path) -> Result<Option<fs::File>, ServiceError> {
    #[cfg(windows)]
    {
        for attempt in 0..=CACHE_CHURN_RETRIES {
            match open_cache_file_no_follow(path) {
                Ok(file) => return Ok(Some(file)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) if windows_cache_contention(&error) => {
                    if attempt == CACHE_CHURN_RETRIES {
                        return Err(cache_churn_error(path));
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => {
                    return Err(crate::failed(format!(
                        "failed to open cache `{}`: {error}",
                        path.display()
                    )));
                }
            }
        }
        unreachable!("cache contention loop either opens or returns a churn error")
    }
    #[cfg(not(windows))]
    match open_cache_file_no_follow(path) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(crate::failed(format!(
            "failed to open cache `{}`: {error}",
            path.display()
        ))),
    }
}

#[cfg(windows)]
fn windows_cache_contention(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32 | 33))
}

fn cache_metadata_is_regular(metadata: &fs::Metadata) -> bool {
    if !metadata.is_file() {
        return false;
    }
    #[cfg(windows)]
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return false;
    }
    true
}

#[cfg(target_os = "linux")]
fn cache_file_identity(_file: &fs::File, metadata: &fs::Metadata) -> io::Result<CacheFileIdentity> {
    Ok(CacheFileIdentity::Linux {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "GetFileInformationByHandle is the Windows file-identity boundary"
)]
fn cache_file_identity(file: &fs::File, _metadata: &fs::Metadata) -> io::Result<CacheFileIdentity> {
    let mut information = std::mem::MaybeUninit::uninit();
    // SAFETY: the file handle remains open and the out-pointer addresses a
    // correctly sized, writable BY_HANDLE_FILE_INFORMATION allocation.
    let result =
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, information.as_mut_ptr()) };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a nonzero result guarantees the API initialized the structure.
    let information = unsafe { information.assume_init() };
    Ok(CacheFileIdentity::Windows {
        volume_serial_number: information.dwVolumeSerialNumber,
        file_index: (u64::from(information.nFileIndexHigh) << 32)
            | u64::from(information.nFileIndexLow),
    })
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
fn cache_file_identity(_file: &fs::File, metadata: &fs::Metadata) -> io::Result<CacheFileIdentity> {
    Ok(CacheFileIdentity::MtimeFallback {
        modified: metadata.modified().ok(),
    })
}
