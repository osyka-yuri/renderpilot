//! Read NVAPI setting state (single and batch).

use renderpilot_nvapi::setting::{NvapiSetting, SettingContext};

use super::super::dto::{NvapiWarningDto, SettingStateResponse};
use super::assemble::assemble_response;
use super::live::{LiveRead, read_dword_or_default, read_live_or_default};
use super::session::open_drs_session;
use super::target::SettingTarget;
use crate::ServiceError;

/// Reads the live state of a single NVAPI `setting` for `game_id`.
pub fn read_setting_state(
    context: &crate::Context,
    target: &SettingTarget<'_>,
    setting: &dyn NvapiSetting,
    ctx: &SettingContext,
) -> Result<SettingStateResponse, ServiceError> {
    let live = read_live_or_default(target, setting, ctx);
    assemble_response(setting, ctx, context.storage(), target, live)
}

/// Reads the live state of **every** supplied setting through a single DRS
/// session + profile lookup, instead of one session per setting. The session
/// and profile are resolved once up front; if any step fails, each setting
/// reports the same diagnostic warning and falls back to default values --
/// mirroring `read_live_or_default` but without re-opening the driver.
pub fn read_all_setting_states(
    context: &crate::Context,
    target: &SettingTarget<'_>,
    settings: &[Box<dyn NvapiSetting>],
    ctx: &SettingContext,
) -> Result<Vec<SettingStateResponse>, ServiceError> {
    let storage = context.storage();
    let exe = ctx.effective_exe.as_deref();

    let session_result = if target.requires_exe() && exe.is_none() {
        Err(NvapiWarningDto::NoExecutable)
    } else {
        open_drs_session()
    };
    let (session, session_warning) =
        session_result.map_or_else(|w| (None, Some(w)), |s| (Some(s), None));

    let (profile, profile_warning) = match session.as_ref() {
        Some(session) => target.resolve_profile_for_read(session, exe),
        None => (None, None),
    };
    let unavailable_warning = session_warning.or(profile_warning);

    let mut responses = Vec::with_capacity(settings.len());
    for setting in settings {
        let setting = setting.as_ref();
        let live = match &profile {
            Some(profile) => read_dword_or_default(profile, setting),
            None => LiveRead::unavailable(setting.default_dword(), unavailable_warning),
        };
        responses.push(assemble_response(setting, ctx, storage, target, live)?);
    }
    Ok(responses)
}
