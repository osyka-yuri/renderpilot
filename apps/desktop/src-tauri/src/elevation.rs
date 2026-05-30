//! Utilities and helpers for managing Windows User Account Control (UAC) elevation.
//!
//! The desktop executable is compiled with an `asInvoker` manifest (the default 
//! for Tauri applications), which causes it to inherit the caller's access token 
//! upon launch. During startup, the application verifies its elevation status. 
//! If the process lacks administrator privileges, it attempts to relaunch itself 
//! using `ShellExecuteExW` with the `runas` verb. Should the user grant UAC 
//! consent, the initial process terminates to allow the newly elevated instance 
//! to proceed. Conversely, if the user declines the prompt—or if system policies 
//! prohibit elevation—the original process continues execution but with NVAPI 
//! write operations disabled.
//!
//! All underlying Win32 Foreign Function Interface (FFI) bindings are encapsulated 
//! within this module, guarded by a `#[cfg(windows)]` attribute at the module declaration.
#![cfg(windows)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// The `ShellExecuteW` function yields an `HINSTANCE` value greater than `32` 
/// upon successful execution, or an `SE_ERR_*` error code upon failure. 
/// Within this context, the primary concern is isolating the `access-denied` error 
/// (the standard result when a user dismisses a UAC prompt) from all other outcomes.
const SE_ERR_ACCESSDENIED: isize = 5;
const SHELL_EXECUTE_SUCCESS_THRESHOLD: isize = 32;

/// A sentinel command-line argument appended during an elevation relaunch.
/// This prevents recursive and infinite UAC prompt loops in edge cases where 
/// the subsequently launched process still fails to acquire elevated privileges 
/// (e.g., split-token scenarios or restrictive Group Policy configurations).
pub const ELEVATION_ATTEMPTED_MARKER: &str = "--rp-elevation-attempted";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationState {
    Elevated,
    NotElevated,
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
}

/// Retrieves the current elevation state of the executing process.
///
/// Any underlying FFI failure is safely mapped to `NotElevated`. This conservative 
/// fallback ensures that in the worst-case scenario—where the API fails despite 
/// the user possessing administrator privileges—the UI will merely display the 
/// "Relaunch as administrator" banner, and the subsequent relaunch workflow 
/// will quickly short-circuit upon verifying existing permissions.
pub fn current_elevation() -> ElevationState {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return ElevationState::NotElevated;
        }

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut size: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );

        let _ = CloseHandle(token);

        if ok == 0 {
            return ElevationState::NotElevated;
        }

        if elevation.TokenIsElevated != 0 {
            ElevationState::Elevated
        } else {
            ElevationState::NotElevated
        }
    }
}

/// Returns `true` if the process command-line carries our sentinel arg,
/// meaning a prior process in this session already tried to elevate.
#[cfg(not(debug_assertions))]
pub fn has_elevation_attempted_marker() -> bool {
    std::env::args_os().any(|a| a == ELEVATION_ATTEMPTED_MARKER)
}

/// Spawns an identical instance of the current executable using `ShellExecuteExW`
/// configured with the `runas` verb, prompting the Windows OS to display a UAC 
/// consent dialog to the user.
///
/// Outcomes:
/// - `Relaunched`: The newly elevated process is initializing. The invoking caller 
///   must return from the `main` function to ensure the current (unelevated) process 
///   terminates gracefully.
/// - `UserCancelled`: The user proactively dismissed the UAC dialog. The caller 
///   should proceed in a degraded operational mode.
/// - `PolicyBlocked(err)`: `ShellExecuteExW` encountered a distinct failure (e.g., 
///   UAC is disabled via Group Policy, or the user is on a restricted standard account). 
///   The caller must also proceed in a degraded mode.
pub fn attempt_self_relaunch_elevated() -> ElevationStartupDecision {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return ElevationStartupDecision::PolicyBlocked(0),
    };

    // Build joined args: existing args (skipping exe argv[0]) + sentinel.
    let mut args: Vec<String> = std::env::args_os()
        .skip(1)
        .filter_map(|a| a.into_string().ok())
        .collect();
    if !args.iter().any(|a| a == ELEVATION_ATTEMPTED_MARKER) {
        args.push(ELEVATION_ATTEMPTED_MARKER.to_owned());
    }
    let args_string = args.join(" ");

    let exe_w = to_wide(exe.as_os_str());
    let verb_w = to_wide("runas");
    let args_w = to_wide(args_string.as_str());

    let hinst = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb_w.as_ptr(),
            exe_w.as_ptr(),
            args_w.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = hinst as isize;

    if code > SHELL_EXECUTE_SUCCESS_THRESHOLD {
        ElevationStartupDecision::Relaunched
    } else if code == SE_ERR_ACCESSDENIED {
        // Standard outcome when the user cancels the UAC consent dialog.
        ElevationStartupDecision::UserCancelled
    } else {
        ElevationStartupDecision::PolicyBlocked(code as u32)
    }
}

fn to_wide<S: AsRef<OsStr>>(s: S) -> Vec<u16> {
    let mut v: Vec<u16> = s.as_ref().encode_wide().collect();
    v.push(0);
    v
}
