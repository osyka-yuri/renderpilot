//! NVAPI setting target scope and write operations.

use renderpilot_nvapi::setting::SettingContext;
use renderpilot_nvapi::{DrsSession, Profile};

use super::super::dto::NvapiWarningDto;
use crate::ServiceError;

/// Which NVIDIA DRS profile an NVAPI setting operation targets.
///
/// The read/write/assembly logic is identical for both variants; this enum
/// captures the only three differences between them: which profile is
/// resolved, whether a local baseline is tracked, and whether an effective
/// executable exists.
#[derive(Debug, Clone, Copy)]
pub enum SettingTarget<'a> {
    /// A specific game's profile, resolved by executable. Baselines are
    /// persisted keyed by `game_id`.
    Game {
        /// Catalog id of the game whose profile (and baselines) this targets.
        game_id: &'a str,
    },
    /// The global/base driver profile (`_GLOBAL_DRIVER_PROFILE_`), which
    /// applies to every game without its own override. No baseline tracking
    /// (the baseline table is keyed by a real `game_id`).
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
    /// missing per-game profile is benign (no warning); a missing global base
    /// profile is reported.
    pub(super) fn resolve_profile_for_read<'s>(
        &self,
        session: &'s DrsSession<'s>,
        exe: Option<&str>,
    ) -> (Option<Profile<'s>>, Option<NvapiWarningDto>) {
        match self {
            Self::Game { .. } => match exe {
                Some(exe) => (session.find_profile_by_exe(exe).ok(), None),
                None => (None, None),
            },
            Self::Global => match session.base_profile() {
                Ok(profile) => (Some(profile), None),
                Err(_) => (None, Some(NvapiWarningDto::DrsFailed)),
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
                let exe = ctx.effective_exe.as_deref().ok_or_else(|| {
                    ServiceError::command_failed("no executable detected for game")
                })?;
                session.find_profile_by_exe(exe).map_err(|_| {
                    ServiceError::command_failed(format!("NVIDIA profile for {exe} not found"))
                })
            }
            Self::Global => session.base_profile().map_err(|e| {
                ServiceError::command_failed(format!("global driver profile unavailable: {e}"))
            }),
        }
    }
}

/// Operation to perform when writing an NVAPI setting value.
#[derive(Debug, Clone, Copy)]
pub enum WriteOp {
    /// Set the setting to the given DWORD value.
    Set(u32),
    /// Delete the setting override, restoring the driver predefined default.
    Delete,
}
