use std::collections::HashMap;

use renderpilot_domain::normalized_path_key;
use renderpilot_nvapi::{DrsSession, Nvapi, NvapiError, Profile};
use renderpilot_storage_sqlite::NvapiPendingOperationRow;
use serde_json::Value;

use super::receipts::{parse_snapshot, snapshot_paths};
use super::{Lookup, Observation, ObservedProfile};
pub(super) fn observe_live(row: &NvapiPendingOperationRow) -> Result<Observation, String> {
    let nvapi = Nvapi::get().ok_or_else(|| "NVAPI is unavailable".to_owned())?;
    nvapi
        .initialize()
        .map_err(|error| format!("NVAPI initialization failed: {error}"))?;
    let session = nvapi
        .create_session()
        .map_err(|error| format!("could not open a fresh DRS session: {error}"))?;
    observe_in_session(&session, row)
}

fn observe_in_session(
    session: &DrsSession<'_>,
    row: &NvapiPendingOperationRow,
) -> Result<Observation, String> {
    let before = parse_snapshot(&row.before_json)?;
    let after = parse_snapshot(&row.after_json)?;
    let setting_id = before
        .get("setting_id")
        .or_else(|| after.get("setting_id"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    let target_id = row.target_id.as_deref();

    let named_profile = match target_id {
        Some(name) => observe_name(session, name, setting_id),
        None => Lookup::Missing,
    };
    let mut path_profiles = HashMap::new();
    for path in snapshot_paths(&before).chain(snapshot_paths(&after)) {
        let key = normalized_path_key(path);
        if let std::collections::hash_map::Entry::Vacant(entry) = path_profiles.entry(key) {
            entry.insert(observe_path(session, path, setting_id));
        }
    }
    let base_profile = if row.game_id.is_none() {
        match session.base_profile() {
            Ok(profile) => observe_profile(&profile, setting_id)
                .map(Lookup::Found)
                .unwrap_or_else(Lookup::Failed),
            Err(error) => lookup_error(error, false),
        }
    } else {
        Lookup::Missing
    };
    Ok(Observation {
        named_profile,
        path_profiles,
        base_profile,
    })
}

fn observe_name(
    session: &DrsSession<'_>,
    profile_name: &str,
    setting_id: Option<u32>,
) -> Lookup<ObservedProfile> {
    match session.find_profile_by_name(profile_name) {
        Ok(profile) => observe_profile(&profile, setting_id)
            .map(Lookup::Found)
            .unwrap_or_else(Lookup::Failed),
        Err(error) => lookup_error(error, true),
    }
}

fn observe_path(
    session: &DrsSession<'_>,
    path: &str,
    setting_id: Option<u32>,
) -> Lookup<ObservedProfile> {
    match session.find_profile_by_exe(path) {
        Ok(profile) => observe_profile(&profile, setting_id)
            .map(Lookup::Found)
            .unwrap_or_else(Lookup::Failed),
        Err(error) => lookup_error(error, false),
    }
}

fn lookup_error(error: NvapiError, by_name: bool) -> Lookup<ObservedProfile> {
    match error {
        NvapiError::ProfileNotFound if by_name => Lookup::Missing,
        NvapiError::ExecutableNotFound if !by_name => Lookup::Missing,
        NvapiError::ExecutableAmbiguous if !by_name => Lookup::Ambiguous,
        other => Lookup::Failed(other.to_string()),
    }
}

fn observe_profile(
    profile: &Profile<'_>,
    setting_id: Option<u32>,
) -> Result<ObservedProfile, String> {
    let explicit_setting = setting_id
        .map(|id| match profile.get_dword_full(id) {
            Ok(state) => Ok((
                id,
                state.is_explicit_in_profile,
                state.is_explicit_in_profile.then_some(state.current),
            )),
            Err(NvapiError::GetSettingFailed(code))
                if code == renderpilot_nvapi::NVAPI_SETTING_NOT_FOUND =>
            {
                Ok((id, false, None))
            }
            Err(error) => Err(error.to_string()),
        })
        .transpose()?;
    Ok(ObservedProfile {
        identity: profile.identity().map_err(|error| error.to_string())?,
        applications: profile.applications().map_err(|error| error.to_string())?,
        settings: profile.settings().map_err(|error| error.to_string())?,
        matched_application: profile.matched_application(),
        explicit_setting,
    })
}
