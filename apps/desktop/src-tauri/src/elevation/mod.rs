//! Utilities and helpers for managing Windows User Account Control (UAC) elevation.
//!
//! The desktop executable is compiled with an `asInvoker` manifest (the default
//! for Tauri applications), which causes it to inherit the caller's access token
//! upon launch. During startup, the application verifies its elevation status.
//! If the process lacks administrator privileges, it attempts to relaunch itself
//! using `ShellExecuteW` with the `runas` verb. Should the user grant UAC
//! consent, the initial process terminates to allow the newly elevated instance
//! to proceed. Conversely, if the user declines the prompt—or if system policies
//! prohibit elevation—the original process continues execution but with NVAPI
//! write operations disabled.
//!
//! Anti-loop handoff uses a short-lived pending file rather than a sticky CLI
//! argument. The Tauri NSIS updater restarts the app with `/R /ARGS` after an
//! update and would forward any CLI marker from the elevated process, leaving
//! the new unelevated instance stuck without a UAC prompt.
//!
//! All underlying Win32 Foreign Function Interface (FFI) bindings are encapsulated
//! within this module, guarded by a `#[cfg(windows)]` attribute at the module declaration.
#![cfg(windows)]
#![expect(
    unsafe_code,
    reason = "Win32 token elevation and ShellExecuteW relaunch; each call has a local SAFETY contract"
)]

mod handoff;
mod relaunch;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub use handoff::clear_elevation_handoff_marker;
pub use relaunch::attempt_self_relaunch_elevated;

/// Stable per-user data root shared by elevated and unelevated processes.
///
/// Resolution order:
/// 1. `RENDERPILOT_APP_DIR` (portable builds set this at process start);
/// 2. `%LOCALAPPDATA%\RenderPilot`;
/// 3. `%TEMP%\RenderPilot` when `LOCALAPPDATA` is unset.
pub(crate) fn renderpilot_local_data_dir() -> std::path::PathBuf {
    resolve_renderpilot_data_dir(
        std::env::var_os(renderpilot_orchestration::portable::APP_DIR_ENV),
        std::env::var_os("LOCALAPPDATA"),
        &std::env::temp_dir(),
    )
}

/// Pure path policy for [`renderpilot_local_data_dir`] (unit-testable).
pub(crate) fn resolve_renderpilot_data_dir(
    app_dir_env: Option<std::ffi::OsString>,
    local_appdata: Option<std::ffi::OsString>,
    temp_dir: &std::path::Path,
) -> std::path::PathBuf {
    if let Some(app_dir) = app_dir_env {
        let path = std::path::PathBuf::from(app_dir);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    if let Some(local) = local_appdata {
        let path = std::path::PathBuf::from(local);
        if !path.as_os_str().is_empty() {
            return path.join("RenderPilot");
        }
    }
    temp_dir.join("RenderPilot")
}

/// Owns a Win32 process/token `HANDLE` and closes it on drop.
///
/// Prevents leaks if `GetTokenInformation` (or future early returns) exit the
/// elevation probe without an explicit `CloseHandle`.
struct OwnedHandle(HANDLE);

impl OwnedHandle {
    /// Takes ownership of a non-null handle. Null is treated as empty.
    #[must_use]
    fn new(handle: HANDLE) -> Option<Self> {
        if handle.is_null() {
            None
        } else {
            Some(Self(handle))
        }
    }

    fn as_raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid open handle from `OpenProcessToken` and is
        // closed exactly once here; no other code retains a copy.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationState {
    Elevated,
    NotElevated,
}

/// Why elevation relaunch was requested — controls handoff anti-loop policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationRelaunchTrigger {
    /// Process startup auto-prompt. Suppressed while a recent handoff marker is live.
    StartupAuto,
    /// Explicit UI "Relaunch as administrator". Always shows UAC.
    UserRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationStartupDecision {
    /// `ShellExecuteW` succeeded; the elevated copy is starting. The
    /// caller MUST return from `main` so the un-elevated process exits
    /// cleanly.
    Relaunched,
    /// User dismissed the UAC consent dialog.
    UserCancelled,
    /// `ShellExecuteW` failed for any reason other than user cancel
    /// (e.g. UAC disabled by group policy, no privilege available on a
    /// standard account). Caller should keep running in degraded mode.
    PolicyBlocked(u32),
    /// Startup auto-elevation skipped: a recent handoff marker is still live.
    /// Only returned for [`ElevationRelaunchTrigger::StartupAuto`].
    SkippedRecentHandoff,
}

/// Retrieves the current elevation state of the executing process.
///
/// Any underlying FFI failure is safely mapped to `NotElevated`. This conservative
/// fallback ensures that in the worst-case scenario—where the API fails despite
/// the user possessing administrator privileges—the UI will merely display the
/// "Relaunch as administrator" banner, and the subsequent relaunch workflow
/// will quickly short-circuit upon verifying existing permissions.
pub fn current_elevation() -> ElevationState {
    // SAFETY: process handle is not owned; TOKEN_QUERY is a valid access mask;
    // `token` is only closed via `OwnedHandle` if OpenProcessToken succeeds.
    let token = unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return ElevationState::NotElevated;
        }
        match OwnedHandle::new(token) {
            Some(owned) => owned,
            None => return ElevationState::NotElevated,
        }
    };

    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut size: u32 = 0;
    // SAFETY: `token` is an open process token; buffer points at a
    // `TOKEN_ELEVATION` of the correct size for `TokenElevation`.
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw(),
            TokenElevation,
            &mut elevation as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        )
    };

    if ok == 0 {
        return ElevationState::NotElevated;
    }

    if elevation.TokenIsElevated != 0 {
        ElevationState::Elevated
    } else {
        ElevationState::NotElevated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn should_suppress_only_startup_auto_with_recent_handoff() {
        assert!(relaunch::should_suppress_auto_elevation(
            ElevationRelaunchTrigger::StartupAuto,
            true
        ));
        assert!(!relaunch::should_suppress_auto_elevation(
            ElevationRelaunchTrigger::StartupAuto,
            false
        ));
        assert!(!relaunch::should_suppress_auto_elevation(
            ElevationRelaunchTrigger::UserRequest,
            true
        ));
        assert!(!relaunch::should_suppress_auto_elevation(
            ElevationRelaunchTrigger::UserRequest,
            false
        ));
    }

    #[test]
    fn data_dir_prefers_portable_app_dir_env() {
        let temp = Path::new("C:\\temp");
        let resolved = resolve_renderpilot_data_dir(
            Some(std::ffi::OsString::from("D:\\portable\\data")),
            Some(std::ffi::OsString::from("C:\\Users\\me\\AppData\\Local")),
            temp,
        );
        assert_eq!(resolved, Path::new("D:\\portable\\data"));
    }

    #[test]
    fn data_dir_uses_local_appdata_when_no_portable_env() {
        let temp = Path::new("C:\\temp");
        let resolved = resolve_renderpilot_data_dir(
            None,
            Some(std::ffi::OsString::from("C:\\Users\\me\\AppData\\Local")),
            temp,
        );
        assert_eq!(
            resolved,
            Path::new("C:\\Users\\me\\AppData\\Local\\RenderPilot")
        );
    }

    #[test]
    fn data_dir_falls_back_to_temp_when_env_missing() {
        let temp = Path::new("C:\\temp");
        let resolved = resolve_renderpilot_data_dir(None, None, temp);
        assert_eq!(resolved, Path::new("C:\\temp\\RenderPilot"));
    }

    #[test]
    fn data_dir_ignores_empty_portable_env() {
        let temp = Path::new("C:\\temp");
        let resolved = resolve_renderpilot_data_dir(
            Some(std::ffi::OsString::from("")),
            Some(std::ffi::OsString::from("C:\\Local")),
            temp,
        );
        assert_eq!(resolved, Path::new("C:\\Local\\RenderPilot"));
    }
}
