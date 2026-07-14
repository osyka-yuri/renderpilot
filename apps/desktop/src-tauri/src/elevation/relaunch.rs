//! Elevated self-relaunch via `ShellExecuteW` with the `runas` verb.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::handoff::{
    clear_elevation_handoff_marker, has_recent_elevation_handoff, write_elevation_handoff_marker,
};
use super::{ElevationRelaunchTrigger, ElevationStartupDecision};

/// The `ShellExecuteW` function yields an `HINSTANCE` value greater than `32`
/// upon successful execution, or an `SE_ERR_*` error code upon failure.
/// Within this context, the primary concern is isolating the `access-denied` error
/// (the standard result when a user dismisses a UAC prompt) from all other outcomes.
const SE_ERR_ACCESSDENIED: isize = 5;
const SHELL_EXECUTE_SUCCESS_THRESHOLD: isize = 32;

/// Legacy CLI sentinel from earlier builds. Still stripped from relaunch argv so
/// NSIS `/ARGS` does not keep forwarding it forever, but it is **not** used as
/// the anti-loop signal (see module docs).
pub(super) const ELEVATION_ATTEMPTED_MARKER: &str = "--rp-elevation-attempted";

/// Pure anti-loop policy: only startup auto-elevation is suppressed by a live handoff.
///
/// `UserRequest` is never suppressed — the banner must always be able to open UAC
/// even while a StartupAuto handoff TTL is live.
pub(super) fn should_suppress_auto_elevation(
    trigger: ElevationRelaunchTrigger,
    recent_handoff: bool,
) -> bool {
    matches!(trigger, ElevationRelaunchTrigger::StartupAuto) && recent_handoff
}

/// Drops the legacy sticky elevation marker from forwarded argv. Does not add it.
pub(super) fn filter_legacy_elevation_marker_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .filter(|a| a.as_ref() != ELEVATION_ATTEMPTED_MARKER)
        .map(|a| a.as_ref().to_owned())
        .collect()
}

/// Spawns an identical instance of the current executable using `ShellExecuteW`
/// configured with the `runas` verb, prompting the Windows OS to display a UAC
/// consent dialog to the user.
///
/// For [`ElevationRelaunchTrigger::StartupAuto`], a live handoff marker suppresses
/// another prompt and yields [`ElevationStartupDecision::SkippedRecentHandoff`].
/// [`ElevationRelaunchTrigger::UserRequest`] always proceeds to UAC.
///
/// Outcomes:
/// - `Relaunched`: The newly elevated process is initializing. The invoking caller
///   must return from the `main` function to ensure the current (unelevated) process
///   terminates gracefully.
/// - `UserCancelled`: The user proactively dismissed the UAC dialog. The caller
///   should proceed in a degraded operational mode.
/// - `PolicyBlocked(err)`: `ShellExecuteW` encountered a distinct failure (e.g.,
///   UAC is disabled via Group Policy, or the user is on a restricted standard account).
///   The caller must also proceed in a degraded mode.
/// - `SkippedRecentHandoff`: Startup auto-elevation only; recent handoff still live.
pub fn attempt_self_relaunch_elevated(
    trigger: ElevationRelaunchTrigger,
) -> ElevationStartupDecision {
    if should_suppress_auto_elevation(trigger, has_recent_elevation_handoff()) {
        return ElevationStartupDecision::SkippedRecentHandoff;
    }

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return ElevationStartupDecision::PolicyBlocked(0),
    };

    // Forward existing args except the legacy sticky marker (and do not re-add it).
    // Sticky CLI markers survive NSIS `/ARGS` restarts after app updates and
    // previously suppressed auto-elevation on the next launch.
    let args_string = relaunch_args_string();

    // Record handoff *before* ShellExecute so a non-elevated child that still
    // somehow starts (or a quick re-launch while UAC is open) does not loop.
    write_elevation_handoff_marker();

    let exe_w = to_wide(exe.as_os_str());
    let verb_w = to_wide("runas");
    let args_w = to_wide(args_string.as_str());

    // SAFETY: all wide strings are NUL-terminated; `hwnd` is null (no owner);
    // pointers remain valid for the duration of the call; return value is only
    // interpreted as success/failure codes, never as an owned handle.
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
        // Elevated child will clear the handoff marker on successful elevate.
        ElevationStartupDecision::Relaunched
    } else if code == SE_ERR_ACCESSDENIED {
        // Standard outcome when the user cancels the UAC consent dialog.
        clear_elevation_handoff_marker();
        ElevationStartupDecision::UserCancelled
    } else {
        clear_elevation_handoff_marker();
        ElevationStartupDecision::PolicyBlocked(code as u32)
    }
}

fn relaunch_args_string() -> String {
    let args = std::env::args_os()
        .skip(1)
        .filter_map(|a| a.into_string().ok());
    filter_legacy_elevation_marker_args(args).join(" ")
}

fn to_wide<S: AsRef<OsStr>>(s: S) -> Vec<u16> {
    let mut v: Vec<u16> = s.as_ref().encode_wide().collect();
    v.push(0);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_legacy_marker_strips_only_the_sentinel() {
        let filtered = filter_legacy_elevation_marker_args([
            "--foo".to_owned(),
            ELEVATION_ATTEMPTED_MARKER.to_owned(),
            "bar".to_owned(),
            ELEVATION_ATTEMPTED_MARKER.to_owned(),
        ]);
        assert_eq!(filtered, vec!["--foo".to_owned(), "bar".to_owned()]);
    }

    #[test]
    fn filter_legacy_marker_leaves_unrelated_args() {
        let filtered = filter_legacy_elevation_marker_args(["--flag", "value", "--other"]);
        assert_eq!(
            filtered,
            vec![
                "--flag".to_owned(),
                "value".to_owned(),
                "--other".to_owned()
            ]
        );
    }

    #[test]
    fn filter_legacy_marker_never_injects_the_sentinel() {
        assert!(filter_legacy_elevation_marker_args(Vec::<String>::new()).is_empty());
    }
}
