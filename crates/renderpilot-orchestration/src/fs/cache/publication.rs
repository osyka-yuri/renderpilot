//! Exact retirement and immutable corrupt-cache diagnostics.

#[cfg(any(all(windows, test), all(test, target_os = "linux")))]
use std::cell::RefCell;
#[cfg(all(windows, test))]
use std::fs;
#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::Path;

#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
};

use crate::ServiceError;
use crate::fs::atomic;

use super::observation::CacheFileSnapshot;
#[cfg(not(windows))]
use super::observation::snapshot_still_current;
use super::{MAX_CORRUPT_DIAGNOSTIC_BYTES, MAX_CORRUPT_DIAGNOSTICS};

#[cfg(all(windows, test))]
#[derive(Debug)]
pub(super) enum CachePublicationTestHook {
    FailBeforeExactRetirement,
    InstallSuccessorAfterExactRetirement(Vec<u8>),
}

#[cfg(all(windows, test))]
thread_local! {
    static CACHE_PUBLICATION_TEST_HOOK: RefCell<Option<CachePublicationTestHook>> = const { RefCell::new(None) };
}

#[cfg(all(windows, test))]
pub(super) struct CachePublicationTestHookGuard(Option<CachePublicationTestHook>);

#[cfg(all(windows, test))]
pub(super) fn inject_cache_publication_test_hook(
    hook: CachePublicationTestHook,
) -> CachePublicationTestHookGuard {
    let previous = CACHE_PUBLICATION_TEST_HOOK.with(|current| current.replace(Some(hook)));
    CachePublicationTestHookGuard(previous)
}

#[cfg(all(windows, test))]
impl Drop for CachePublicationTestHookGuard {
    fn drop(&mut self) {
        CACHE_PUBLICATION_TEST_HOOK.with(|current| current.replace(self.0.take()));
    }
}

#[cfg(all(windows, test))]
fn cache_publication_test_before_exact_retirement() -> io::Result<()> {
    CACHE_PUBLICATION_TEST_HOOK.with(|current| {
        let fail = matches!(
            current.borrow().as_ref(),
            Some(CachePublicationTestHook::FailBeforeExactRetirement)
        );
        if fail {
            current.borrow_mut().take();
            Err(io::Error::other(
                "injected cache publication failure before exact retirement",
            ))
        } else {
            Ok(())
        }
    })
}

#[cfg(all(windows, test))]
fn cache_publication_test_after_exact_retirement(path: &Path) -> io::Result<()> {
    match CACHE_PUBLICATION_TEST_HOOK.with(|current| current.borrow_mut().take()) {
        Some(CachePublicationTestHook::InstallSuccessorAfterExactRetirement(bytes)) => {
            fs::write(path, bytes)
        }
        None | Some(CachePublicationTestHook::FailBeforeExactRetirement) => Ok(()),
    }
}

/// A test-only interposition after an exact Linux snapshot has been read but
/// before conflict classification. Production never contains this replacement
/// path; it lets tests demonstrate that the retained snapshot cannot authorize
/// replacing a successor installed at the destination pathname.
#[cfg(all(test, target_os = "linux"))]
#[derive(Debug)]
pub(super) enum LinuxCacheConflictTestHook {
    InstallSuccessorAfterSnapshotProof(Vec<u8>),
}

#[cfg(all(test, target_os = "linux"))]
thread_local! {
    static LINUX_CACHE_CONFLICT_TEST_HOOK: RefCell<Option<LinuxCacheConflictTestHook>> = const { RefCell::new(None) };
}

#[cfg(all(test, target_os = "linux"))]
pub(super) struct LinuxCacheConflictTestHookGuard(Option<LinuxCacheConflictTestHook>);

#[cfg(all(test, target_os = "linux"))]
pub(super) fn inject_linux_cache_conflict_test_hook(
    hook: LinuxCacheConflictTestHook,
) -> LinuxCacheConflictTestHookGuard {
    LinuxCacheConflictTestHookGuard(
        LINUX_CACHE_CONFLICT_TEST_HOOK.with(|current| current.replace(Some(hook))),
    )
}

#[cfg(all(test, target_os = "linux"))]
impl Drop for LinuxCacheConflictTestHookGuard {
    fn drop(&mut self) {
        LINUX_CACHE_CONFLICT_TEST_HOOK.with(|current| current.replace(self.0.take()));
    }
}

#[cfg(all(test, target_os = "linux"))]
pub(super) fn cache_linux_conflict_test_after_snapshot_proof(
    path: &Path,
) -> Result<(), ServiceError> {
    match LINUX_CACHE_CONFLICT_TEST_HOOK.with(|current| current.borrow_mut().take()) {
        Some(LinuxCacheConflictTestHook::InstallSuccessorAfterSnapshotProof(bytes)) => {
            atomic::write_file_atomically(path, &bytes)
        }
        None => Ok(()),
    }
}

#[cfg(windows)]
pub(super) fn publish_after_exact_retirement(
    path: &Path,
    snapshot: CacheFileSnapshot,
    candidate_bytes: &[u8],
) -> Result<atomic::NoReplaceWrite, ServiceError> {
    // The candidate must exist durably before the exact rejected object is
    // retired. Publication remains no-replace after the pinned handle closes,
    // preserving any late successor that claims the reusable pathname.
    let prepared = atomic::prepare_file_atomically_no_replace(path, candidate_bytes)?;
    #[cfg(test)]
    if let Err(retirement_error) = cache_publication_test_before_exact_retirement() {
        return match prepared.discard() {
            Ok(()) => Err(crate::failed(format!(
                "cache `{}` could not retire the exact pinned snapshot before publication: {retirement_error}",
                path.display()
            ))),
            Err(cleanup_error) => Err(crate::failed(format!(
                "cache `{}` could not retire the exact pinned snapshot before publication: {retirement_error}; {cleanup_error}",
                path.display()
            ))),
        };
    }
    if let Err(retirement_error) = delete_exact_snapshot(&snapshot) {
        return match prepared.discard() {
            Ok(()) => Err(crate::failed(format!(
                "cache `{}` could not retire the exact pinned snapshot before publication: {retirement_error}",
                path.display()
            ))),
            Err(cleanup_error) => Err(crate::failed(format!(
                "cache `{}` could not retire the exact pinned snapshot before publication: {retirement_error}; {cleanup_error}",
                path.display()
            ))),
        };
    }
    drop(snapshot);
    #[cfg(test)]
    cache_publication_test_after_exact_retirement(path).map_err(|error| {
        crate::failed(format!(
            "cache `{}` could not install the deterministic test successor after exact retirement: {error}",
            path.display()
        ))
    })?;
    prepared.publish()
}

/// Writes one immutable diagnostic from the already captured rejected object.
/// It never reads the reusable active pathname for diagnostic bytes. On
/// Windows, a successful diagnostic may delete the exact retained object only
/// after a fresh no-follow generation proof; Linux leaves the active name for
/// the next atomic publication because unlinking a reusable pathname cannot
/// prove that a non-compliant publisher did not install a successor.
pub(super) enum CacheRetirement {
    #[cfg(windows)]
    Retired,
    Retained,
}

enum DiagnosticWrite {
    Written,
    Occupied,
}

pub(super) fn quarantine_snapshot_at_locked(
    path: &Path,
    snapshot: CacheFileSnapshot,
) -> Result<CacheRetirement, ServiceError> {
    let base = crate::fs::with_added_extension(path, "corrupt").map_err(|error| {
        crate::failed(format!(
            "cache quarantine: cannot derive diagnostic slots for `{}`: {error}",
            path.display()
        ))
    })?;
    for slot in 0..MAX_CORRUPT_DIAGNOSTICS {
        let candidate = if slot == 0 {
            base.clone()
        } else {
            crate::fs::with_added_extension(&base, &slot.to_string()).map_err(|error| {
                crate::failed(format!(
                    "cache quarantine: cannot derive diagnostic slot for `{}`: {error}",
                    path.display()
                ))
            })?
        };
        match write_snapshot_no_replace(&candidate, &snapshot) {
            Ok(DiagnosticWrite::Written) => return retire_snapshot_if_current(path, snapshot),
            Ok(DiagnosticWrite::Occupied) => continue,
            Err(error) => return Err(error),
        }
    }
    log::debug!(
        "cache quarantine: all diagnostic slots for `{}` are occupied; preserving the active cache until refresh",
        path.display()
    );
    Ok(CacheRetirement::Retained)
}

fn write_snapshot_no_replace(
    destination: &Path,
    snapshot: &CacheFileSnapshot,
) -> Result<DiagnosticWrite, ServiceError> {
    let byte_len = snapshot.bytes.len().min(MAX_CORRUPT_DIAGNOSTIC_BYTES);
    match atomic::write_file_atomically_no_replace(destination, &snapshot.bytes[..byte_len])? {
        atomic::NoReplaceWrite::Published => {
            crate::fs::sync_parent_directory_best_effort(destination);
            Ok(DiagnosticWrite::Written)
        }
        atomic::NoReplaceWrite::Occupied => Ok(DiagnosticWrite::Occupied),
    }
}

fn retire_snapshot_if_current(
    path: &Path,
    snapshot: CacheFileSnapshot,
) -> Result<CacheRetirement, ServiceError> {
    #[cfg(windows)]
    {
        // The exclusive retained handle is the current-object proof. Do not
        // re-open the public name between durable diagnostic publication and
        // exact handle retirement.
        match delete_exact_snapshot(&snapshot) {
            Ok(()) => {
                drop(snapshot);
                Ok(CacheRetirement::Retired)
            }
            Err(error) => Err(crate::failed(format!(
                "cache quarantine: could not remove the exact rejected cache `{}` after durable diagnostic publication: {error}",
                path.display()
            ))),
        }
    }
    #[cfg(not(windows))]
    {
        if !snapshot_still_current(path, &snapshot)? {
            return Ok(CacheRetirement::Retained);
        }
        // Linux's authority socket excludes compliant publishers, but POSIX
        // unlink is still pathname based. Keep the active name so an external
        // replacement cannot be removed after the proof above.
        let _ = path;
        drop(snapshot);
        Ok(CacheRetirement::Retained)
    }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "SetFileInformationByHandle deletes the exact retained no-follow cache object"
)]
fn delete_exact_snapshot(snapshot: &CacheFileSnapshot) -> io::Result<()> {
    let mut disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: `snapshot.owner` is the retained exclusive DELETE-capable handle
    // for the exact no-follow object that supplied the diagnostic bytes.
    let result = unsafe {
        SetFileInformationByHandle(
            snapshot.owner.file().as_raw_handle(),
            FileDispositionInfo,
            (&raw mut disposition).cast(),
            u32::try_from(std::mem::size_of::<FILE_DISPOSITION_INFO>())
                .expect("FILE_DISPOSITION_INFO size fits u32"),
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
