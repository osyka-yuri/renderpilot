//! Live DWORD reads from an open DRS profile.

use renderpilot_nvapi::setting::{NvapiSetting, SettingContext};
use renderpilot_nvapi::{DwordSettingState, NVAPI_SETTING_NOT_FOUND, NvapiError, Profile};

use super::super::dto::NvapiWarningDto;
use super::session::open_drs_session;
use super::target::SettingTarget;
use crate::ServiceError;

/// Outcome of a single live NVAPI read, decoupled from how the DRS session was
/// obtained so the single-setting and batch paths can share response assembly.
#[derive(Clone, Copy)]
pub(super) struct LiveRead {
    pub(super) current: u32,
    pub(super) predefined: Option<u32>,
    pub(super) is_current_predefined: bool,
    pub(super) has_profile_for_exe: bool,
    /// Set when the live value could not be read; surfaced as a UI warning.
    pub(super) warning: Option<NvapiWarningDto>,
}

impl LiveRead {
    /// The setting is absent from the profile (no override): the current value
    /// is the setting's declared default and it counts as "at the driver
    /// default".
    pub(super) fn unset(default: u32) -> Self {
        Self {
            current: default,
            predefined: None,
            is_current_predefined: true,
            has_profile_for_exe: true,
            warning: None,
        }
    }

    /// The driver/profile could not be read at all: show the declared default
    /// and surface the reason.
    pub(super) fn unavailable(default: u32, warning: Option<NvapiWarningDto>) -> Self {
        Self {
            current: default,
            predefined: None,
            is_current_predefined: false,
            has_profile_for_exe: false,
            warning,
        }
    }
}

pub(super) fn read_pre_state(
    setting: &dyn NvapiSetting,
    profile: &Profile<'_>,
) -> Result<DwordSettingState, ServiceError> {
    match profile.get_dword_full(setting.nvapi_id()) {
        Ok(state) => Ok(state),
        Err(NvapiError::GetSettingFailed(code)) if code == NVAPI_SETTING_NOT_FOUND => {
            Ok(DwordSettingState {
                current: setting.default_dword(),
                predefined: None,
                is_current_predefined: true,
            })
        }
        Err(e) => Err(ServiceError::command_failed(format!(
            "could not read setting: {e}"
        ))),
    }
}

/// Reads the live state of a single setting, opening its own DRS session.
/// Used by the single-setting read path; the batch path opens one session and
/// calls [`read_dword_or_default`] directly.
pub(super) fn read_live_or_default(
    target: &SettingTarget<'_>,
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
) -> LiveRead {
    let unavailable =
        |warning: NvapiWarningDto| LiveRead::unavailable(setting.default_dword(), Some(warning));

    let exe = ctx.effective_exe.as_deref();
    if target.requires_exe() && exe.is_none() {
        return unavailable(NvapiWarningDto::NoExecutable);
    }
    let session = match open_drs_session() {
        Ok(session) => session,
        Err(warning) => return unavailable(warning),
    };
    match target.resolve_profile_for_read(&session, exe) {
        (Some(profile), _) => read_dword_or_default(&profile, setting),
        (None, warning) => LiveRead::unavailable(setting.default_dword(), warning),
    }
}

/// Reads a DWORD from an already-resolved profile. A missing setting (or any
/// read failure) is treated as the setting's default with no warning -- absence
/// is the expected "no override" state.
pub(super) fn read_dword_or_default(profile: &Profile<'_>, setting: &dyn NvapiSetting) -> LiveRead {
    match profile.get_dword_full(setting.nvapi_id()) {
        Ok(state) => LiveRead {
            current: state.current,
            predefined: state.predefined,
            is_current_predefined: state.is_current_predefined,
            has_profile_for_exe: true,
            warning: None,
        },
        Err(_) => LiveRead::unset(setting.default_dword()),
    }
}
