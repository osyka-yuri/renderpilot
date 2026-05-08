//! Catalog toggles for remote cover sources.
//!
//! Missing, empty, and unknown settings default to enabled.

use renderpilot_storage_sqlite::SqliteStorage;

use crate::error::CliError;

pub(crate) const COVERS_STEAM_CDN_SETTING: &str = "covers_steam_cdn_enabled";
pub(crate) const COVERS_GOG_CDN_SETTING: &str = "covers_gog_cdn_enabled";

/// Enables SteamGridDB lookups, independent of whether an API key row exists.
pub(crate) const COVERS_STEAMGRIDDB_REMOTE_SETTING: &str = "covers_steamgriddb_enabled";

const DISABLED_SETTING_VALUES: &[&str] = &["false", "0", "no"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CoverRemotePolicy {
    pub(crate) steam_cdn: bool,
    pub(crate) gog_cdn: bool,
    pub(crate) steamgriddb: bool,
}

impl CoverRemotePolicy {
    pub(crate) const DEFAULT: Self = Self {
        steam_cdn: true,
        gog_cdn: true,
        steamgriddb: true,
    };

    pub(crate) fn load(storage: &SqliteStorage) -> Result<Self, CliError> {
        Ok(Self {
            steam_cdn: load_enabled_setting(storage, COVERS_STEAM_CDN_SETTING)?,
            gog_cdn: load_enabled_setting(storage, COVERS_GOG_CDN_SETTING)?,
            steamgriddb: load_enabled_setting(storage, COVERS_STEAMGRIDDB_REMOTE_SETTING)?,
        })
    }
}

impl Default for CoverRemotePolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

fn load_enabled_setting(storage: &SqliteStorage, key: &str) -> Result<bool, CliError> {
    let raw = storage.get_setting(key)?;
    Ok(parse_setting_bool_default_true(raw.as_deref()))
}

/// Parses a boolean-like setting where absence, blank values, and unknown values
/// intentionally default to `true`.
///
/// This keeps remote cover sources enabled unless the user explicitly disables them.
#[must_use]
pub(crate) fn parse_setting_bool_default_true(raw: Option<&str>) -> bool {
    let Some(value) = raw.map(str::trim) else {
        return true;
    };

    !is_disabled_setting_value(value)
}

fn is_disabled_setting_value(value: &str) -> bool {
    DISABLED_SETTING_VALUES
        .iter()
        .any(|disabled| value.eq_ignore_ascii_case(disabled))
}

#[cfg(test)]
mod tests {
    use super::{parse_setting_bool_default_true, CoverRemotePolicy, DISABLED_SETTING_VALUES};

    #[test]
    fn policy_default_enables_all_sources() {
        assert_eq!(
            CoverRemotePolicy::default(),
            CoverRemotePolicy {
                steam_cdn: true,
                gog_cdn: true,
                steamgriddb: true,
            }
        );
    }

    #[test]
    fn default_true_when_missing_empty_or_whitespace() {
        assert!(parse_setting_bool_default_true(None));
        assert!(parse_setting_bool_default_true(Some("")));
        assert!(parse_setting_bool_default_true(Some("   ")));
    }

    #[test]
    fn explicit_false_variants_are_disabled() {
        for value in DISABLED_SETTING_VALUES {
            assert!(
                !parse_setting_bool_default_true(Some(value)),
                "{value:?} should disable the setting"
            );
        }
    }

    #[test]
    fn explicit_false_variants_are_trimmed_and_case_insensitive() {
        assert!(!parse_setting_bool_default_true(Some(" false ")));
        assert!(!parse_setting_bool_default_true(Some(" FALSE ")));
        assert!(!parse_setting_bool_default_true(Some(" No ")));
        assert!(!parse_setting_bool_default_true(Some(" 0 ")));
    }

    #[test]
    fn true_like_values_stay_enabled() {
        assert!(parse_setting_bool_default_true(Some("true")));
        assert!(parse_setting_bool_default_true(Some("TRUE")));
        assert!(parse_setting_bool_default_true(Some("1")));
        assert!(parse_setting_bool_default_true(Some("yes")));
    }

    #[test]
    fn unknown_values_stay_enabled_for_backward_compatibility() {
        assert!(parse_setting_bool_default_true(Some("maybe")));
        assert!(parse_setting_bool_default_true(Some("disabled")));
        assert!(parse_setting_bool_default_true(Some("off")));
    }
}
