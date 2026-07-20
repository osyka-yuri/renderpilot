//! App lifecycle Tauri commands (init snapshot, elevation).

use renderpilot_api::ApiError;
use renderpilot_orchestration::ServiceError;

use super::{CommandError, JsonCommandResult};

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
/// Returns `CommandFailed` if the user declines the UAC prompt or policy blocks elevation;
/// the frontend shows a non-fatal toast in that case.
#[tauri::command]
pub async fn request_admin_relaunch(app: tauri::AppHandle) -> JsonCommandResult {
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
            ElevationStartupDecision::UserCancelled => Err(CommandError::from(ApiError::Service(
                ServiceError::CommandFailed("UAC consent was declined".to_owned()),
            ))),
            ElevationStartupDecision::PolicyBlocked(code) => Err(CommandError::from(
                ApiError::Service(ServiceError::CommandFailed(format!(
                    "OS denied the elevation request (ShellExecute code {code})"
                ))),
            )),
            // UserRequest never suppresses on a live handoff; keep exhaustiveness.
            ElevationStartupDecision::SkippedRecentHandoff => {
                debug_assert!(false, "UserRequest must not skip elevation handoff");
                Err(CommandError::from(ApiError::Service(
                    ServiceError::CommandFailed(
                        "elevation relaunch was unexpectedly skipped".to_owned(),
                    ),
                )))
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err(CommandError::from(ApiError::Service(
            ServiceError::CommandFailed(
                "administrator relaunch is only supported on Windows".to_owned(),
            ),
        )))
    }
}
