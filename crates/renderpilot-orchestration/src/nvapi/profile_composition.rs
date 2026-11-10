//! Canonical NVIDIA profile composition and setting reconciliation.

use std::collections::HashSet;

use renderpilot_nvapi::{ApplicationIdentity, SettingIdentity};
use serde_json::Value;

pub(super) fn composition_value(
    mut applications: Vec<ApplicationIdentity>,
    mut settings: Vec<SettingIdentity>,
) -> Value {
    applications.sort_by_cached_key(|application| application.app_name.to_lowercase());
    settings.sort_by_key(|setting| setting.id);
    serde_json::json!({ "applications": applications, "settings": settings })
}

struct Composition<'a> {
    applications: &'a [Value],
    settings: &'a [Value],
}

fn setting_id(setting: &Value) -> Option<u32> {
    u32::try_from(setting.get("id")?.as_u64()?).ok()
}

impl<'a> Composition<'a> {
    fn read(value: &'a Value) -> Result<Self, String> {
        let fields = value
            .as_object()
            .filter(|fields| fields.len() == 2)
            .ok_or("owned NVIDIA profile composition is incomplete")?;
        let applications = fields
            .get("applications")
            .and_then(Value::as_array)
            .ok_or("owned NVIDIA profile applications are invalid")?;
        let settings = fields
            .get("settings")
            .and_then(Value::as_array)
            .ok_or("owned NVIDIA profile settings are invalid")?;
        let mut ids = HashSet::new();
        for setting in settings {
            let id = setting_id(setting).ok_or("owned NVIDIA profile setting id is invalid")?;
            if !ids.insert(id) {
                return Err("owned NVIDIA profile composition contains duplicate settings".into());
            }
        }
        Ok(Self {
            applications,
            settings,
        })
    }

    fn other_settings(&self, selected: u32) -> impl Iterator<Item = &'a Value> {
        self.settings
            .iter()
            .filter(move |setting| setting_id(setting) != Some(selected))
    }
}

fn decode_composition(json: &str) -> Result<Value, String> {
    serde_json::from_str(json)
        .map_err(|error| format!("invalid NVIDIA profile composition: {error}"))
}

pub(super) fn validate_composition(json: &str) -> Result<(), String> {
    let value = decode_composition(json)?;
    Composition::read(&value)?;
    Ok(())
}

pub(super) fn composition_matches_except_setting(
    expected_json: &str,
    actual: &Value,
    selected: u32,
) -> Result<bool, String> {
    let expected = decode_composition(expected_json)?;
    let expected = Composition::read(&expected)?;
    let actual = Composition::read(actual)?;
    Ok(expected.applications == actual.applications
        && expected
            .other_settings(selected)
            .eq(actual.other_settings(selected)))
}

/// Verifies a fresh observation against the captured before-state. A successful
/// write updates only its own setting in the durable ownership receipt.
pub(super) fn reconcile_owned_setting(
    recorded_json: &str,
    before: &Value,
    actual: &Value,
    selected: u32,
    applied: bool,
) -> Result<Option<String>, String> {
    let recorded = decode_composition(recorded_json)?;
    let recorded = Composition::read(&recorded)?;
    let before_fields = Composition::read(before)?;
    let actual_fields = Composition::read(actual)?;
    if recorded.applications != before_fields.applications
        || before_fields.applications != actual_fields.applications
    {
        return Err("owned NVIDIA profile applications changed".into());
    }
    if !applied {
        return if before == actual {
            Ok(None)
        } else {
            Err("owned NVIDIA profile composition no longer matches the before-state".into())
        };
    }
    if !before_fields
        .other_settings(selected)
        .eq(actual_fields.other_settings(selected))
    {
        return Err("unrelated NVIDIA profile settings changed during the write".into());
    }

    let mut settings: Vec<_> = recorded.other_settings(selected).collect();
    settings.extend(
        actual_fields
            .settings
            .iter()
            .filter(|setting| setting_id(setting) == Some(selected)),
    );
    settings.sort_by_key(|setting| setting_id(setting));
    Ok(Some(
        serde_json::json!({ "applications": recorded.applications, "settings": settings })
            .to_string(),
    ))
}
