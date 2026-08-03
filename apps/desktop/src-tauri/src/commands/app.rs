//! App lifecycle Tauri commands (init snapshot, elevation).

use super::error::CommandErrorKind;
use super::{CommandBoundary, CommandError, JsonCommandResult};

/// Returns the `AppInitializationState` snapshot computed at startup.
/// Synchronous: the state is already in managed memory, no I/O.
// `tauri::command` requires `State` parameters by value.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri injects State by value in the generated command signature"
)]
#[tauri::command]
pub fn get_app_initialization_state(
    state: tauri::State<'_, crate::AppInitializationState>,
) -> crate::AppInitializationState {
    *state.inner()
}

/// Relaunches the app elevated via `ShellExecuteW(verb="runas")` and exits this process.
/// Returns a stable elevation-specific machine code when relaunch cannot proceed.
#[tauri::command]
pub async fn request_admin_relaunch(app: tauri::AppHandle) -> JsonCommandResult {
    let boundary = CommandBoundary::new("request_admin_relaunch");
    #[cfg(windows)]
    {
        use crate::elevation::{
            ElevationRelaunchTrigger, ElevationStartupDecision, attempt_self_relaunch_elevated,
        };
        match attempt_self_relaunch_elevated(ElevationRelaunchTrigger::UserRequest) {
            ElevationStartupDecision::Relaunched => {
                app.exit(0);
                Ok(serde_json::json!({ "relaunched": true }))
            }
            ElevationStartupDecision::UserCancelled => {
                Err(boundary.record(CommandError::with_diagnostic(
                    CommandErrorKind::ElevationCancelled,
                    "UAC consent was declined",
                )))
            }
            ElevationStartupDecision::PolicyBlocked(code) => {
                Err(boundary.record(CommandError::with_diagnostic(
                    CommandErrorKind::ElevationPolicyBlocked,
                    format_args!("OS denied the elevation request (ShellExecute code {code})"),
                )))
            }
            // UserRequest never suppresses on a live handoff; keep exhaustiveness.
            ElevationStartupDecision::SkippedRecentHandoff => {
                debug_assert!(false, "UserRequest must not skip elevation handoff");
                Err(boundary.record(CommandError::with_diagnostic(
                    CommandErrorKind::ElevationRelaunchFailed,
                    "elevation relaunch was unexpectedly skipped",
                )))
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err(boundary.record(CommandError::with_diagnostic(
            CommandErrorKind::ElevationUnsupported,
            "administrator relaunch is only supported on Windows",
        )))
    }
}
