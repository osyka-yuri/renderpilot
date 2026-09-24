use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::normalized_path_key;
use renderpilot_nvapi::ApplicationIdentity;
use serde_json::Value;

use super::{Lookup, Observation, ObservedProfile};
pub(super) fn profile_receipts(
    profile: &ObservedProfile,
    witness: &ApplicationIdentity,
) -> AppResult<(String, String, String)> {
    Ok((
        serde_json::to_string(&profile.identity)
            .map_err(|error| AppError::storage_failed(error.to_string()))?,
        serde_json::to_string(witness)
            .map_err(|error| AppError::storage_failed(error.to_string()))?,
        composition_json(profile).ok_or_else(|| {
            AppError::storage_failed("could not encode NVIDIA profile composition")
        })?,
    ))
}

pub(super) fn composition_json(profile: &ObservedProfile) -> Option<String> {
    let mut applications = profile.applications.clone();
    applications.sort_by_cached_key(|application| application.app_name.to_lowercase());
    let mut settings = profile.settings.clone();
    settings.sort_by_key(|setting| setting.id);
    serde_json::to_string(&serde_json::json!({"applications": applications, "settings": settings}))
        .ok()
}

pub(super) fn composition_matches_except_setting(
    expected_json: &str,
    observed: &ObservedProfile,
    setting_id: u32,
) -> AppResult<bool> {
    let mut expected: Value = match serde_json::from_str(expected_json) {
        Ok(value) => value,
        Err(error) => {
            return Err(AppError::storage_failed(format!(
                "invalid owned profile composition: {error}"
            )));
        }
    };
    let mut actual: Value = match composition_json(observed) {
        Some(json) => serde_json::from_str(&json)
            .map_err(|error| AppError::storage_failed(error.to_string()))?,
        None => return Ok(false),
    };
    remove_setting_from_composition(&mut expected, setting_id);
    remove_setting_from_composition(&mut actual, setting_id);
    Ok(expected == actual)
}

fn remove_setting_from_composition(composition: &mut Value, setting_id: u32) {
    if let Some(settings) = composition
        .get_mut("settings")
        .and_then(Value::as_array_mut)
    {
        settings.retain(|setting| {
            setting.get("id").and_then(Value::as_u64) != Some(u64::from(setting_id))
        });
    }
}

pub(super) fn settings_match_before(profile: &ObservedProfile, before: &Value) -> bool {
    let Some(composition) = string(before, "composition") else {
        return false;
    };
    let Ok(expected) = serde_json::from_str::<Value>(composition) else {
        return false;
    };
    let Ok(actual) = composition_json(profile)
        .ok_or(())
        .and_then(|json| serde_json::from_str::<Value>(&json).map_err(|_| ()))
    else {
        return false;
    };
    expected.get("settings") == actual.get("settings")
}

pub(super) fn created_profile_matches(
    named: &ObservedProfile,
    by_path: &ObservedProfile,
    profile_name: &str,
    path: &str,
) -> bool {
    named.identity.name == profile_name
        && !named.identity.is_predefined
        && by_path.identity.name == profile_name
        && named.applications.len() == 1
        && by_path.applications.len() == 1
        && path_equal(&named.applications[0].app_name, path)
        && path_equal(&by_path.applications[0].app_name, path)
        && named.settings.is_empty()
        && by_path.settings.is_empty()
}

pub(super) fn witness_for_path<'a>(
    profile: &'a ObservedProfile,
    path: &str,
) -> Option<&'a ApplicationIdentity> {
    profile
        .matched_application
        .as_ref()
        .filter(|application| path_equal(&application.app_name, path))
}

pub(super) fn witness_json_for_path(profile: &ObservedProfile, path: &str) -> Option<String> {
    witness_for_path(profile, path).and_then(|witness| serde_json::to_string(witness).ok())
}

pub(super) fn path_lookup<'a>(
    observation: &'a Observation,
    path: &str,
) -> &'a Lookup<ObservedProfile> {
    observation
        .path_profiles
        .get(&normalized_path_key(path))
        .unwrap_or(&MISSING_LOOKUP)
}

static MISSING_LOOKUP: Lookup<ObservedProfile> = Lookup::Missing;

pub(super) fn found_path<'a>(
    observation: &'a Observation,
    path: &str,
) -> Option<&'a ObservedProfile> {
    match path_lookup(observation, path) {
        Lookup::Found(profile) => Some(profile),
        _ => None,
    }
}

pub(super) fn path_equal(left: &str, right: &str) -> bool {
    normalized_path_key(left) == normalized_path_key(right)
}

pub(super) fn parse_snapshot(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|error| error.to_string())
}

pub(super) fn string<'a>(snapshot: &'a Value, key: &str) -> Option<&'a str> {
    snapshot.get(key).and_then(Value::as_str)
}

pub(super) fn unsigned(snapshot: &Value, key: &str) -> Option<u32> {
    snapshot
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

pub(super) fn setting_state(snapshot: &Value) -> Option<(bool, Option<u32>)> {
    let present = snapshot.get("present")?.as_bool()?;
    let value = snapshot
        .get("value")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    if present && value.is_none() {
        return None;
    }
    Some((present, value))
}

pub(super) fn snapshot_paths(snapshot: &Value) -> impl Iterator<Item = &str> + '_ {
    [
        "binding_path",
        "executable_path",
        "old_path",
        "new_path",
        "application_path",
    ]
    .into_iter()
    .filter_map(|key| snapshot.get(key).and_then(Value::as_str))
}
