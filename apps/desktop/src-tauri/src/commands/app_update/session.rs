//! Process-local updater session state.
//!
//! A check attempt is an owned rollback capability: cancellation and every
//! early return restore only the `Checking` state created by that attempt.

use std::sync::{Mutex, MutexGuard};

use crate::commands::{CommandError, error::CommandErrorKind};

use super::dto::UpdateResult;

#[derive(Default)]
pub(crate) struct AppUpdateState {
    session: Mutex<UpdateSession>,
}

#[derive(Default)]
pub(super) enum UpdateSession {
    #[default]
    Idle,
    Checking {
        attempt_id: String,
    },
    #[cfg(all(windows, feature = "portable"))]
    Checked {
        id: String,
    },
    #[cfg(all(windows, feature = "portable"))]
    Downloaded {
        id: String,
    },
    #[cfg(not(all(windows, feature = "portable")))]
    Checked {
        id: String,
        update: tauri_plugin_updater::Update,
    },
    #[cfg(not(all(windows, feature = "portable")))]
    Downloaded {
        id: String,
        update: tauri_plugin_updater::Update,
        bytes: Vec<u8>,
    },
}

pub(super) struct CheckAttempt<'state> {
    state: &'state AppUpdateState,
    id: String,
    armed: bool,
}

impl<'state> CheckAttempt<'state> {
    pub(super) fn start(
        state: &'state AppUpdateState,
        make_id: impl FnOnce() -> UpdateResult<String>,
    ) -> UpdateResult<Self> {
        // ID creation is deliberately before admission. If entropy fails, no
        // session state exists to roll back.
        let id = make_id()?;
        let mut session = lock(state)?;
        if !matches!(*session, UpdateSession::Idle) {
            return Err(CommandError::with_diagnostic(
                CommandErrorKind::AppUpdateSessionActive,
                "another updater session is active",
            ));
        }
        *session = UpdateSession::Checking {
            attempt_id: id.clone(),
        };
        drop(session);
        Ok(Self {
            state,
            id,
            armed: true,
        })
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn finish_idle(self) -> UpdateResult<()> {
        self.finish(UpdateSession::Idle)
    }

    #[cfg(all(windows, feature = "portable"))]
    pub(super) fn finish_portable(self) -> UpdateResult<()> {
        let id = self.id.clone();
        self.finish(UpdateSession::Checked { id })
    }

    #[cfg(not(all(windows, feature = "portable")))]
    pub(super) fn finish_installed(self, update: tauri_plugin_updater::Update) -> UpdateResult<()> {
        let id = self.id.clone();
        self.finish(UpdateSession::Checked { id, update })
    }

    fn finish(mut self, next: UpdateSession) -> UpdateResult<()> {
        let mut session = lock(self.state)?;
        if !matches!(
            &*session,
            UpdateSession::Checking { attempt_id } if attempt_id == &self.id
        ) {
            return Err(CommandError::with_diagnostic(
                CommandErrorKind::AppUpdateInvalidState,
                "updater check attempt was no longer current",
            ));
        }
        *session = next;
        self.armed = false;
        Ok(())
    }
}

impl Drop for CheckAttempt<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Ok(mut session) = self.state.session.lock() else {
            return;
        };
        if matches!(
            &*session,
            UpdateSession::Checking { attempt_id } if attempt_id == &self.id
        ) {
            *session = UpdateSession::Idle;
        }
    }
}

pub(super) fn lock(state: &AppUpdateState) -> UpdateResult<MutexGuard<'_, UpdateSession>> {
    state.session.lock().map_err(|_| {
        CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateStateFailed,
            "updater session poisoned",
        )
    })
}

#[cfg(all(windows, feature = "portable"))]
pub(super) fn reset(state: &AppUpdateState) {
    if let Ok(mut session) = state.session.lock() {
        *session = UpdateSession::Idle;
    }
}

pub(super) fn close(state: &AppUpdateState, session_id: &str) -> UpdateResult<()> {
    let mut session = lock(state)?;
    match &*session {
        UpdateSession::Checked { id, .. } | UpdateSession::Downloaded { id, .. }
            if id == session_id =>
        {
            *session = UpdateSession::Idle;
            Ok(())
        }
        UpdateSession::Idle => Ok(()),
        UpdateSession::Checking { .. } => Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateInvalidState,
            "updater check is still running",
        )),
        _ => Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateInvalidSession,
            "updater session did not match",
        )),
    }
}

#[cfg(all(windows, feature = "portable"))]
pub(super) fn require_portable(
    state: &AppUpdateState,
    id: &str,
    downloaded: bool,
) -> UpdateResult<()> {
    let session = lock(state)?;
    match (&*session, downloaded) {
        (UpdateSession::Checked { id: actual }, false) if actual == id => Ok(()),
        (UpdateSession::Downloaded { id: actual }, true) if actual == id => Ok(()),
        _ => Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateInvalidSession,
            "portable updater session was not ready for this request",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_error() -> CommandError {
        CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateStateFailed,
            "injected id failure",
        )
    }

    #[test]
    fn id_failure_happens_before_session_admission() {
        let state = AppUpdateState::default();
        assert!(CheckAttempt::start(&state, || Err(test_error())).is_err());
        assert!(matches!(
            *lock(&state).expect("lock state"),
            UpdateSession::Idle
        ));
    }

    #[test]
    fn cancelling_a_check_restores_retryability() {
        let state = AppUpdateState::default();
        let attempt =
            CheckAttempt::start(&state, || Ok("first".to_owned())).expect("start first attempt");
        drop(attempt);

        let retry = CheckAttempt::start(&state, || Ok("retry".to_owned()))
            .expect("cancelled check must be retryable");
        assert_eq!(retry.id(), "retry");
    }

    #[test]
    fn finalization_disarms_rollback() {
        let state = AppUpdateState::default();
        let attempt =
            CheckAttempt::start(&state, || Ok("finished".to_owned())).expect("start attempt");
        attempt
            .finish(UpdateSession::Checking {
                attempt_id: "final-state".to_owned(),
            })
            .expect("finish attempt");

        assert!(matches!(
            &*lock(&state).expect("lock state"),
            UpdateSession::Checking { attempt_id } if attempt_id == "final-state"
        ));
    }

    #[test]
    fn stale_attempt_drop_cannot_reset_a_newer_attempt() {
        let state = AppUpdateState::default();
        let stale =
            CheckAttempt::start(&state, || Ok("stale".to_owned())).expect("start stale attempt");
        *lock(&state).expect("replace state") = UpdateSession::Checking {
            attempt_id: "current".to_owned(),
        };
        drop(stale);

        assert!(matches!(
            &*lock(&state).expect("lock state"),
            UpdateSession::Checking { attempt_id } if attempt_id == "current"
        ));
    }
}
