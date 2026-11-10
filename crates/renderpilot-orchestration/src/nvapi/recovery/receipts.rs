use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::normalized_path_key;
use renderpilot_nvapi::ApplicationIdentity;
use serde_json::Value;

use super::super::profile_composition::{composition_value, validate_composition};
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
    serde_json::to_string(&observed_composition(profile)).ok()
}

pub(super) fn observed_composition(profile: &ObservedProfile) -> Value {
    composition_value(profile.applications.clone(), profile.settings.clone())
}

pub(super) fn settings_match_before(profile: &ObservedProfile, before: &Value) -> bool {
    let Some(composition) = string(before, "composition") else {
        return false;
    };
    let Ok(expected) = serde_json::from_str::<Value>(composition) else {
        return false;
    };
    let actual = observed_composition(profile);
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

/// Require the selected path in `app_name` for RenderPilot-owned profile
/// lifecycle recovery. Setting recovery uses the returned lookup witness even
/// when NVIDIA records only a basename.
pub(super) fn exact_owned_lifecycle_witness_for_path<'a>(
    profile: &'a ObservedProfile,
    path: &str,
) -> Option<&'a ApplicationIdentity> {
    profile
        .matched_application
        .as_ref()
        .filter(|application| path_equal(&application.app_name, path))
}

pub(super) fn exact_owned_lifecycle_witness_json_for_path(
    profile: &ObservedProfile,
    path: &str,
) -> Option<String> {
    exact_owned_lifecycle_witness_for_path(profile, path)
        .and_then(|witness| serde_json::to_string(witness).ok())
}

/// Serialize the application record returned by the selected full-path lookup.
/// A basename `app_name` is valid; comparison still uses the complete record.
pub(super) fn effective_setting_witness_json(profile: &ObservedProfile) -> Option<String> {
    let witness = profile.matched_application.as_ref()?;
    if witness.app_name.is_empty() {
        return None;
    }
    serde_json::to_string(witness).ok()
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
    let value: Value = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if let Some(composition) = value.get("owned_before_composition") {
        let composition = composition
            .as_str()
            .ok_or_else(|| "owned profile composition receipt is not a string".to_owned())?;
        validate_composition(composition)?;
    }
    Ok(value)
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
