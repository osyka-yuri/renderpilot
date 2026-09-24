//! NVAPI setting target scope and write operations.

use renderpilot_nvapi::setting::SettingContext;
use renderpilot_nvapi::{DrsSession, NvapiError, Profile};

use super::super::dto::NvapiWarningDto;
use crate::ServiceError;

/// Which NVIDIA DRS profile an NVAPI setting operation targets.
///
/// The read/write/assembly logic is identical for both variants; this enum
/// captures the only three differences between them: which profile is
/// resolved, whether a per-game original state is tracked, and whether an effective
/// executable exists.
#[derive(Debug, Clone, Copy)]
pub enum SettingTarget<'a> {
    /// A specific game's profile, resolved by executable. Setting claims are
    /// associated with the exact game and executable path.
    Game {
        /// Catalog id of the game whose profile and setting claims this targets.
        game_id: &'a str,
    },
    /// The global/base driver profile (`_GLOBAL_DRIVER_PROFILE_`), which
    /// applies to every game without its own override. It has no per-game
    /// original state to restore.
    Global,
}

impl SettingTarget<'_> {
    /// The game this target tracks state for, or `None` for the global profile.
    pub(super) fn game_id(&self) -> Option<&str> {
        match self {
            Self::Game { game_id } => Some(game_id),
            Self::Global => None,
        }
    }

    /// Whether reads/writes are scoped to an executable's profile (`true`) or
    /// the global base profile (`false`, which needs no executable).
    pub(super) fn requires_exe(&self) -> bool {
        matches!(self, Self::Game { .. })
    }

    /// Resolves the DRS profile within an open session for a *read*. Returns
    /// the profile when resolved, plus an optional warning to surface. A
    /// Only the documented full-path absence result is benign; ambiguity and
    /// other DRS lookup failures remain visible to the caller.
    pub(super) fn resolve_profile_for_read<'s>(
        &self,
        session: &'s DrsSession<'s>,
        exe: Option<&str>,
    ) -> (Option<Profile<'s>>, Option<NvapiWarningDto>) {
        match self {
            Self::Game { .. } => match exe {
                Some(exe) => match session.find_profile_by_exe(exe) {
                    Ok(profile) => (Some(profile), None),
                    Err(error) => (None, profile_lookup_warning(&error)),
                },
                None => (None, None),
            },
            Self::Global => match session.base_profile() {
                Ok(profile) => (Some(profile), None),
                Err(error) => (
                    None,
                    Some(
                        profile_lookup_warning(&error)
                            .unwrap_or(NvapiWarningDto::DrsProfileLookupFailed),
                    ),
                ),
            },
        }
    }

    /// Resolves the DRS profile for a *write*, where a missing profile is a
    /// hard error.
    pub(super) fn resolve_profile_for_write<'s>(
        &self,
        session: &'s DrsSession<'s>,
        ctx: &SettingContext,
    ) -> Result<Profile<'s>, ServiceError> {
        match self {
            Self::Game { .. } => {
                let exe = ctx.effective_exe_path.as_deref().ok_or_else(|| {
                    ServiceError::command_failed("no executable detected for game")
                })?;
                session
                    .find_profile_by_exe(exe)
                    .map_err(|error| profile_lookup_error(exe, error))
            }
            Self::Global => session.base_profile().map_err(|e| {
                ServiceError::command_failed(format!("global driver profile unavailable: {e}"))
            }),
        }
    }
}

fn profile_lookup_warning(error: &NvapiError) -> Option<NvapiWarningDto> {
    match error {
        NvapiError::ExecutableNotFound => None,
        NvapiError::ExecutableAmbiguous => Some(NvapiWarningDto::ExecutableAmbiguous),
        _ => Some(NvapiWarningDto::DrsProfileLookupFailed),
    }
}

fn profile_lookup_error(exe: &str, error: NvapiError) -> ServiceError {
    match error {
        NvapiError::ExecutableNotFound => {
            ServiceError::command_failed(format!("NVIDIA profile for {exe} was not found"))
        }
        NvapiError::ExecutableAmbiguous => ServiceError::command_failed(format!(
            "NVIDIA reports multiple profiles for {exe}; choose an unambiguous executable binding"
        )),
        other => ServiceError::command_failed(format!(
            "NVIDIA DRS profile lookup failed for {exe}: {other}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_documented_executable_absence_is_a_benign_read() {
        assert_eq!(
            profile_lookup_warning(&NvapiError::ExecutableNotFound),
            None
        );
        assert_eq!(
            profile_lookup_warning(&NvapiError::ExecutableAmbiguous),
            Some(NvapiWarningDto::ExecutableAmbiguous)
        );
        assert_eq!(
            profile_lookup_warning(&NvapiError::DrsOperationFailed {
                operation: "find application",
                status: -1,
            }),
            Some(NvapiWarningDto::DrsProfileLookupFailed)
        );
    }

    #[test]
    fn writes_preserve_ambiguous_and_non_absence_lookup_errors() {
        let ambiguous = profile_lookup_error("C:/game.exe", NvapiError::ExecutableAmbiguous);
        assert!(ambiguous.to_string().contains("multiple profiles"));
        let failed = profile_lookup_error(
            "C:/game.exe",
            NvapiError::DrsOperationFailed {
                operation: "find application",
                status: -1,
            },
        );
        assert!(failed.to_string().contains("DRS profile lookup failed"));
        let missing = profile_lookup_error("C:/game.exe", NvapiError::ExecutableNotFound);
        assert!(missing.to_string().contains("was not found"));
    }
}

/// Operation to perform when writing an NVAPI setting value.
#[derive(Debug, Clone, Copy)]
pub enum WriteOp {
    /// Set the setting to the given DWORD value.
    Set(u32),
    /// Delete the setting override, restoring the driver predefined default.
    Delete,
    /// Restore the exact explicit state RenderPilot observed before its first write.
    RestoreOriginal,
}
