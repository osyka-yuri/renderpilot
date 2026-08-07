//! Cross-process cache transaction leases.

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
#[cfg(target_os = "linux")]
use std::{
    os::linux::net::SocketAddrExt,
    os::unix::{
        ffi::OsStrExt,
        net::{SocketAddr, UnixDatagram},
    },
};
#[cfg(target_os = "linux")]
use std::{thread, time::Duration};

use sha2::{Digest, Sha256};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_FAILED, WAIT_OBJECT_0},
    System::Threading::{INFINITE, WaitForSingleObject},
};

use crate::ServiceError;

const CACHE_AUTHORITY_VERSION: &[u8] = b"renderpilot-cache-authority-v2\0";

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "CreateMutexW and ReleaseMutex are the narrowly scoped Win32 kernel authority boundary"
)]
unsafe extern "system" {
    fn CreateMutexW(
        attributes: *const core::ffi::c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> HANDLE;
    fn ReleaseMutex(mutex: HANDLE) -> i32;
}

/// Runs one short local cache operation under an exclusive cross-process
/// kernel-backed authority. Supported production targets deliberately do not
/// create a filesystem sidecar: a cache path is data, never lock authority.
pub(crate) fn with_cache_file_transaction<T, F>(
    path: &Path,
    operation: F,
) -> Result<T, ServiceError>
where
    F: FnOnce() -> Result<T, ServiceError>,
{
    let _lease = acquire_cache_transaction_lease(path)?;
    operation()
}

fn normalized_cache_path(path: &Path) -> Result<PathBuf, ServiceError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            crate::failed(format!(
                "cannot lock cache path `{}` because it has no parent directory",
                path.display()
            ))
        })?;
    let leaf = path
        .file_name()
        .filter(|leaf| !leaf.is_empty())
        .ok_or_else(|| {
            crate::failed(format!(
                "cannot lock cache path `{}` because it has no file name",
                path.display()
            ))
        })?;
    fs::create_dir_all(parent).map_err(|error| {
        crate::failed(format!(
            "failed to create cache directory `{}`: {error}",
            parent.display()
        ))
    })?;
    let parent = parent.canonicalize().map_err(|error| {
        crate::failed(format!(
            "failed to normalize cache directory `{}`: {error}",
            parent.display()
        ))
    })?;
    Ok(parent.join(leaf))
}

fn cache_authority_name(path: &Path) -> Result<String, ServiceError> {
    let normalized = normalized_cache_path(path)?;
    let mut digest = Sha256::new();
    digest.update(CACHE_AUTHORITY_VERSION);
    #[cfg(windows)]
    for unit in normalized.as_os_str().encode_wide() {
        // Windows cache lookups are case-insensitive. Normalize the ASCII
        // portion without lossy UTF-16 conversion so equivalent configured
        // drive and ordinary directory spellings converge on one mutex.
        let unit = if (u16::from(b'A')..=u16::from(b'Z')).contains(&unit) {
            unit + (u16::from(b'a') - u16::from(b'A'))
        } else {
            unit
        };
        digest.update(unit.to_le_bytes());
    }
    #[cfg(target_os = "linux")]
    digest.update(normalized.as_os_str().as_bytes());
    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    digest.update(normalized.as_os_str().to_string_lossy().as_bytes());
    Ok(format!(
        "renderpilot-cache-v2-{}",
        hex::encode(digest.finalize())
    ))
}

#[cfg(windows)]
struct WindowsCacheLease(HANDLE);

#[cfg(windows)]
impl Drop for WindowsCacheLease {
    #[expect(
        unsafe_code,
        reason = "the owning mutex lease releases and closes exactly its Win32 handle"
    )]
    fn drop(&mut self) {
        // SAFETY: this type owns an acquired non-null mutex handle.
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "a named Win32 mutex is the process-lifetime cache transaction authority"
)]
fn acquire_cache_transaction_lease(path: &Path) -> Result<WindowsCacheLease, ServiceError> {
    let name = cache_authority_name(path)?;
    let name = format!("Local\\{name}")
        .encode_utf16()
        .chain(std::iter::once(0_u16))
        .collect::<Vec<_>>();
    // SAFETY: a null SECURITY_ATTRIBUTES requests the non-inheritable default,
    // and `name` is a NUL-terminated, process-owned UTF-16 allocation.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(crate::failed(format!(
            "failed to create cache transaction mutex for `{}`: {}",
            path.display(),
            io::Error::last_os_error()
        )));
    }
    // SAFETY: `handle` is a live mutex handle from CreateMutexW.
    let wait = unsafe { WaitForSingleObject(handle, INFINITE) };
    if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
        return Ok(WindowsCacheLease(handle));
    }
    // SAFETY: acquisition failed before ownership transferred to the RAII
    // wrapper, so this call closes the exact handle returned above.
    unsafe {
        let _ = CloseHandle(handle);
    }
    let detail = if wait == WAIT_FAILED {
        io::Error::last_os_error().to_string()
    } else {
        format!("unexpected wait result 0x{wait:08X}")
    };
    Err(crate::failed(format!(
        "failed to acquire cache transaction mutex for `{}`: {detail}",
        path.display()
    )))
}

#[cfg(target_os = "linux")]
struct LinuxCacheLease {
    _socket: UnixDatagram,
}

#[cfg(target_os = "linux")]
fn acquire_cache_transaction_lease(path: &Path) -> Result<LinuxCacheLease, ServiceError> {
    let name = cache_authority_name(path)?;
    let address = SocketAddr::from_abstract_name(name.as_bytes()).map_err(|error| {
        crate::failed(format!(
            "failed to construct cache transaction socket for `{}`: {error}",
            path.display()
        ))
    })?;
    loop {
        match UnixDatagram::bind_addr(&address) {
            Ok(socket) => return Ok(LinuxCacheLease { _socket: socket }),
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                return Err(crate::failed(format!(
                    "failed to acquire cache transaction socket for `{}`: {error}",
                    path.display()
                )));
            }
        }
    }
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
struct DevelopmentCacheLease {
    _file: fs::File,
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
fn acquire_cache_transaction_lease(path: &Path) -> Result<DevelopmentCacheLease, ServiceError> {
    // Unsupported development targets retain the former sidecar fallback. It
    // is intentionally not selected on Windows or Linux production builds.
    let lock_path = crate::fs::with_added_extension(path, "renderpilot-cache-v1.lock")
        .map_err(|error| crate::failed(error.to_string()))?;
    let lease = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| {
            crate::failed(format!(
                "failed to open development cache transaction lease `{}`: {error}",
                lock_path.display()
            ))
        })?;
    lease.lock().map_err(|error| {
        crate::failed(format!(
            "failed to acquire development cache transaction lease `{}`: {error}",
            lock_path.display()
        ))
    })?;
    Ok(DevelopmentCacheLease { _file: lease })
}
