//! Live DWORD reads from an open DRS profile.

use renderpilot_nvapi::setting::{NvapiSetting, SettingContext};
use renderpilot_nvapi::{DwordSettingState, NVAPI_SETTING_NOT_FOUND, NvapiError, Profile};

use super::super::dto::NvapiWarningDto;
use super::session::open_drs_session;
use super::target::SettingTarget;
use crate::ServiceError;

/// Outcome of a single live NVAPI read, decoupled from how the DRS session was
/// obtained so the single-setting and batch paths can share response assembly.
pub(super) struct LiveRead {
    pub(super) current: u32,
    /// Exact live profile setting presence; `None` means DRS did not confirm it.
    pub(super) current_is_explicit: Option<bool>,
    pub(super) predefined: Option<u32>,
    pub(super) is_current_predefined: bool,
    pub(super) has_profile_for_exe: bool,
    /// Exact DRS identity of the profile resolved for the selected full path.
    pub(super) profile_name: Option<String>,
    /// Set when the live value could not be read; surfaced as a UI warning.
    pub(super) warning: Option<NvapiWarningDto>,
}

pub(super) struct ProfileReadIdentity {
    profile_name: Option<String>,
    warning: Option<NvapiWarningDto>,
}

impl LiveRead {
    /// The setting is absent from the profile (no override): the current value
    /// is the setting's declared default and it counts as "at the driver
    /// default".
    #[cfg(test)]
    pub(super) fn unset(default: u32) -> Self {
        Self {
            current: default,
            current_is_explicit: Some(false),
            predefined: None,
            is_current_predefined: true,
            has_profile_for_exe: true,
            profile_name: None,
            warning: None,
        }
    }

    /// The driver/profile could not be read at all: show the declared default
    /// and surface the reason.
    pub(super) fn unavailable(default: u32, warning: Option<NvapiWarningDto>) -> Self {
        Self {
            current: default,
            current_is_explicit: None,
            predefined: None,
            is_current_predefined: false,
            has_profile_for_exe: false,
            profile_name: None,
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
                is_explicit_in_profile: false,
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

    let exe = ctx.effective_exe_path.as_deref();
    if target.requires_exe() && exe.is_none() {
        return unavailable(NvapiWarningDto::NoExecutable);
    }
    let session = match open_drs_session() {
        Ok(session) => session,
        Err(warning) => return unavailable(warning),
    };
    match target.resolve_profile_for_read(&session, exe) {
        (Some(profile), _) => {
            let identity = profile_read_identity(&profile);
            read_dword_or_default(&profile, setting, &identity)
        }
        (None, warning) => LiveRead::unavailable(setting.default_dword(), warning),
    }
}

pub(super) fn profile_read_identity(profile: &Profile<'_>) -> ProfileReadIdentity {
    match profile.identity() {
        Ok(identity) => ProfileReadIdentity {
            profile_name: Some(identity.name),
            warning: None,
        },
        Err(_) => ProfileReadIdentity {
            profile_name: None,
            warning: Some(NvapiWarningDto::DrsProfileLookupFailed),
        },
    }
}

/// Reads a DWORD from an already-resolved profile. A missing setting (or any
/// read failure) is treated as the setting's default with no warning -- absence
/// is the expected "no override" state.
pub(super) fn read_dword_or_default(
    profile: &Profile<'_>,
    setting: &dyn NvapiSetting,
    identity: &ProfileReadIdentity,
) -> LiveRead {
    let profile_name = identity.profile_name.clone();
    let identity_warning = identity.warning;
    match profile.get_dword_full(setting.nvapi_id()) {
        Ok(state) => LiveRead {
            current: state.current,
            current_is_explicit: Some(state.is_explicit_in_profile),
            predefined: state.predefined,
            is_current_predefined: state.is_current_predefined,
            has_profile_for_exe: true,
            profile_name,
            warning: identity_warning,
        },
        Err(NvapiError::GetSettingFailed(code)) if code == NVAPI_SETTING_NOT_FOUND => LiveRead {
            current: setting.default_dword(),
            current_is_explicit: Some(false),
            predefined: None,
            is_current_predefined: true,
            has_profile_for_exe: true,
            profile_name,
            warning: identity_warning,
        },
        Err(_) => LiveRead {
            current: setting.default_dword(),
            current_is_explicit: None,
            predefined: None,
            is_current_predefined: false,
            has_profile_for_exe: true,
            profile_name,
            warning: Some(NvapiWarningDto::DrsFailed),
        },
    }
}
