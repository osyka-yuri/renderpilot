//! Stable updater IPC boundary.
//!
//! The command layer owns only serialization and session correlation. Native
//! Tauri/NSIS resources and portable supervisor requests live in separate,
//! compile-time-selected backends.

mod dto;
#[cfg(not(all(windows, feature = "portable")))]
mod installed;
#[cfg(all(windows, feature = "portable"))]
mod portable;
mod session;

use dto::UpdateResult;
pub use dto::{AppUpdateApplyResponse, AppUpdateCheckResponse, AppUpdateDownloadEvent};
pub(crate) use session::AppUpdateState;
use session::CheckAttempt;
use tauri::ipc::Channel;

use super::{CommandBoundary, CommandError, error::CommandErrorKind};

#[tauri::command]
pub async fn app_update_check(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppUpdateState>,
) -> UpdateResult<Option<AppUpdateCheckResponse>> {
    let boundary = CommandBoundary::new("app_update_check");
    async {
        portable_request_open()?;
        let attempt = CheckAttempt::start(&state, random_id)?;
        let session_id = attempt.id().to_owned();

        #[cfg(all(windows, feature = "portable"))]
        {
            drop(app);
            let Some(metadata) = portable::check(&session_id)? else {
                attempt.finish_idle()?;
                return Ok(None);
            };
            attempt.finish_portable()?;
            Ok(Some(AppUpdateCheckResponse {
                session_id,
                metadata,
            }))
        }

        #[cfg(not(all(windows, feature = "portable")))]
        {
            let Some(checked) = installed::check(&app).await? else {
                attempt.finish_idle()?;
                return Ok(None);
            };
            let metadata = checked.metadata;
            attempt.finish_installed(checked.update)?;
            Ok(Some(AppUpdateCheckResponse {
                session_id,
                metadata,
            }))
        }
    }
    .await
    .map_err(|error| boundary.record(error))
}

#[tauri::command]
pub async fn app_update_download(
    session_id: String,
    on_event: Channel<AppUpdateDownloadEvent>,
    state: tauri::State<'_, AppUpdateState>,
) -> UpdateResult<()> {
    let boundary = CommandBoundary::new("app_update_download");
    async {
        portable_request_open()?;

        #[cfg(all(windows, feature = "portable"))]
        {
            portable::download(&state, session_id, &on_event)
        }

        #[cfg(not(all(windows, feature = "portable")))]
        {
            installed::download(&state, session_id, on_event).await
        }
    }
    .await
    .map_err(|error| boundary.record(error))
}

#[tauri::command]
pub async fn app_update_apply(
    session_id: String,
    state: tauri::State<'_, AppUpdateState>,
    app: tauri::AppHandle,
) -> UpdateResult<AppUpdateApplyResponse> {
    let boundary = CommandBoundary::new("app_update_apply");
    async {
        portable_request_open()?;

        #[cfg(all(windows, feature = "portable"))]
        {
            portable::apply(&state, &session_id)?;
            session::reset(&state);
            app.exit(0);
            Ok(AppUpdateApplyResponse::NativeExit)
        }

        #[cfg(not(all(windows, feature = "portable")))]
        {
            drop(app);
            installed::apply(&state, session_id)
        }
    }
    .await
    .map_err(|error| boundary.record(error))
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri IPC commands receive deserialized ownership values"
)]
pub fn app_update_close(
    session_id: String,
    state: tauri::State<'_, AppUpdateState>,
) -> UpdateResult<()> {
    let boundary = CommandBoundary::new("app_update_close");
    session::close(&state, &session_id).map_err(|error| boundary.record(error))
}

#[tauri::command]
#[cfg_attr(
    all(windows, feature = "portable"),
    expect(
        clippy::needless_pass_by_value,
        reason = "Tauri IPC commands receive deserialized ownership values"
    )
)]
pub fn portable_trial_ready(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(all(windows, feature = "portable"))]
    {
        crate::complete_portable_activation(&app)
    }
    #[cfg(not(all(windows, feature = "portable")))]
    {
        drop(app);
        Ok(())
    }
}

fn random_id() -> UpdateResult<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        CommandError::with_diagnostic(CommandErrorKind::AppUpdateStateFailed, error)
    })?;
    let mut id = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(id, "{byte:02x}");
    }
    Ok(id)
}

fn portable_request_open() -> UpdateResult<()> {
    #[cfg(all(windows, feature = "portable"))]
    crate::portable_runtime::activation::require_committed().map_err(portable::map_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn updater_boundary_registers_diagnostics_without_expanding_the_wire_shape() {
        let error = CommandBoundary::new("app_update_check").record(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateCheckFailed,
            "private updater detail",
        ));

        assert_eq!(
            serde_json::to_value(error).expect("serialize updater command error"),
            json!({ "code": "app_update_check_failed" })
        );
    }
}
