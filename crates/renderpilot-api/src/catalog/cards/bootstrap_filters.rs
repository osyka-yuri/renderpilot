//! Normalizes the persisted games-filter setting into the effective bootstrap selection.
//!
//! The bootstrap response serializes this same value back to the frontend, so the
//! first card page and the visible filter state cannot interpret missing, legacy,
//! or malformed settings differently.

use renderpilot_orchestration::domain::{AddonKind, GraphicsTechnology, Launcher};

const INITIAL_LIBRARY_FILTERS: &[GraphicsTechnology] = &[
    GraphicsTechnology::DlssSuperResolution,
    GraphicsTechnology::DlssFrameGeneration,
    GraphicsTechnology::DlssRayReconstruction,
    GraphicsTechnology::NvidiaStreamline,
    GraphicsTechnology::IntelXeSs,
    GraphicsTechnology::IntelXeFg,
    GraphicsTechnology::IntelXeLl,
    GraphicsTechnology::AmdFsr,
    GraphicsTechnology::AmdFsrFrameGeneration,
    GraphicsTechnology::AmdFsrRayRegeneration,
    GraphicsTechnology::DirectStorage,
    GraphicsTechnology::MicrosoftDxc,
    GraphicsTechnology::D3D12Agility,
    GraphicsTechnology::OpenVr,
];
const INITIAL_LAUNCHER_FILTERS: &[Launcher] = &[
    Launcher::Steam,
    Launcher::Epic,
    Launcher::Gog,
    Launcher::Ubisoft,
    Launcher::Ea,
    Launcher::BattleNet,
    Launcher::Xbox,
    Launcher::Manual,
];

#[derive(Debug, Default, serde::Serialize)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[serde(rename_all = "camelCase")]
pub(super) struct EffectiveGamesFilters {
    pub(super) libraries: Vec<String>,
    pub(super) addons: Vec<String>,
    pub(super) launchers: Vec<String>,
    pub(super) launcher_order: Vec<String>,
    pub(super) search_query: String,
    pub(super) show_hidden: bool,
    pub(super) favorites_only: bool,
}

impl EffectiveGamesFilters {
    fn initial() -> Self {
        Self {
            libraries: INITIAL_LIBRARY_FILTERS
                .iter()
                .map(|technology| technology.as_slug().to_owned())
                .collect(),
            addons: initial_addon_filters(),
            launchers: INITIAL_LAUNCHER_FILTERS
                .iter()
                .map(|launcher| launcher.as_str().to_owned())
                .collect(),
            ..Self::default()
        }
    }

    fn canonicalized(mut self) -> Self {
        self.libraries = canonicalize_selection(
            &self.libraries,
            INITIAL_LIBRARY_FILTERS
                .iter()
                .map(|technology| technology.as_slug()),
        );
        self.addons = canonicalize_selection(
            &self.addons,
            AddonKind::ALL.iter().map(|kind| kind.as_str()),
        );
        self.launchers = canonicalize_selection(
            &self.launchers,
            INITIAL_LAUNCHER_FILTERS
                .iter()
                .map(|launcher| launcher.as_str()),
        );
        self.launcher_order = canonicalize_order(
            &self.launcher_order,
            INITIAL_LAUNCHER_FILTERS
                .iter()
                .map(|launcher| launcher.as_str()),
        );
        self
    }
}

pub(super) fn parse_bootstrap_filters(value: Option<&str>) -> EffectiveGamesFilters {
    let Some(value) = value else {
        return EffectiveGamesFilters::initial().canonicalized();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(value) else {
        return EffectiveGamesFilters::initial().canonicalized();
    };

    let parsed = match value {
        serde_json::Value::Array(libraries) => EffectiveGamesFilters {
            libraries: read_string_values(&libraries),
            addons: initial_addon_filters(),
            ..EffectiveGamesFilters::default()
        },
        serde_json::Value::Object(fields) => EffectiveGamesFilters {
            libraries: read_object_string_list(&fields, "libraries"),
            addons: fields
                .get("addons")
                .map_or_else(initial_addon_filters, read_string_list),
            launchers: read_object_string_list(&fields, "launchers"),
            launcher_order: read_object_string_list(&fields, "launcherOrder"),
            search_query: fields
                .get("searchQuery")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_owned(),
            show_hidden: fields
                .get("showHidden")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or_default(),
            favorites_only: fields
                .get("favoritesOnly")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or_default(),
        },
        _ => EffectiveGamesFilters::initial(),
    };

    parsed.canonicalized()
}

fn initial_addon_filters() -> Vec<String> {
    AddonKind::ALL
        .iter()
        .map(|kind| kind.as_str().to_owned())
        .collect()
}

fn read_object_string_list(
    fields: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Vec<String> {
    fields.get(field).map(read_string_list).unwrap_or_default()
}

fn read_string_list(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map_or_else(Vec::new, |values| read_string_values(values))
}

fn read_string_values(values: &[serde_json::Value]) -> Vec<String> {
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if !result.iter().any(|existing| existing == value) {
            result.push(value.to_owned());
        }
    }
    result
}

fn canonicalize_selection<'a>(
    selected: &[String],
    available: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    available
        .filter(|value| selected.iter().any(|selected| selected == value))
        .map(str::to_owned)
        .collect()
}

fn canonicalize_order<'a>(
    selected: &[String],
    available: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let available = available.collect::<Vec<_>>();
    let mut result = Vec::with_capacity(available.len());

    for value in selected {
        if available.contains(&value.as_str()) && !result.contains(value) {
            result.push(value.to_owned());
        }
    }
    for value in available {
        if !result.iter().any(|selected| selected == value) {
            result.push(value.to_owned());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::parse_bootstrap_filters;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct ContractCase {
        name: String,
        persisted: serde_json::Value,
        expected: serde_json::Value,
    }

    #[test]
    fn missing_or_invalid_settings_use_the_initial_ui_selection() {
        let missing = parse_bootstrap_filters(None);
        let invalid = parse_bootstrap_filters(Some("not-json"));

        assert_eq!(missing, invalid);
        assert_eq!(
            missing.libraries,
            vec![
                "dlss_super_resolution",
                "dlss_frame_generation",
                "dlss_ray_reconstruction",
                "nvidia_streamline",
                "intel_xess",
                "intel_xefg",
                "intel_xell",
                "amd_fsr",
                "amd_fsr_frame_generation",
                "amd_fsr_ray_regeneration",
                "direct_storage",
                "microsoft_dxc",
                "d3d12_agility",
                "openvr",
            ]
        );
        assert_eq!(
            missing.addons,
            vec![String::from("renodx"), String::from("luma")]
        );
        assert_eq!(
            missing.launchers,
            vec![
                "Steam",
                "Epic",
                "Gog",
                "Ubisoft",
                "Ea",
                "BattleNet",
                "Xbox",
                "Manual",
            ]
        );
        assert_eq!(missing.launcher_order, missing.launchers);
    }

    #[test]
    fn explicit_empty_filters_remain_empty() {
        let parsed = parse_bootstrap_filters(Some(
            r#"{
                "libraries": [],
                "addons": null,
                "launchers": [],
                "launcherOrder": [],
                "searchQuery": "  ",
                "showHidden": false,
                "favoritesOnly": false
            }"#,
        ));

        assert!(parsed.libraries.is_empty());
        assert!(parsed.addons.is_empty());
        assert!(parsed.launchers.is_empty());
        assert_eq!(parsed.launcher_order.len(), 8);
        assert!(parsed.search_query.is_empty());
        assert!(!parsed.show_hidden);
        assert!(!parsed.favorites_only);
    }

    #[test]
    fn missing_addons_field_uses_the_legacy_select_all_behavior() {
        let parsed = parse_bootstrap_filters(Some(r#"{"libraries":[],"launchers":[]}"#));

        assert_eq!(
            parsed.addons,
            vec![String::from("renodx"), String::from("luma")]
        );
    }

    #[test]
    fn legacy_library_array_is_preserved_and_uses_current_addon_defaults() {
        let parsed = parse_bootstrap_filters(Some(
            r#"[" dlss_super_resolution ", null, "dlss_super_resolution"]"#,
        ));

        assert_eq!(
            parsed.libraries,
            vec![String::from("dlss_super_resolution")]
        );
        assert_eq!(
            parsed.addons,
            vec![String::from("renodx"), String::from("luma")]
        );
    }

    #[test]
    fn stale_and_internal_values_are_removed_before_filters_reach_the_frontend() {
        let parsed = parse_bootstrap_filters(Some(
            r#"{
                "libraries": ["unknown", "amd_fsr_loader", "intel_xess"],
                "addons": ["future-addon", "luma"],
                "launchers": ["FutureLauncher", "Steam"],
                "launcherOrder": ["FutureLauncher", "Steam"]
            }"#,
        ));

        assert_eq!(parsed.libraries, vec![String::from("intel_xess")]);
        assert_eq!(parsed.addons, vec![String::from("luma")]);
        assert_eq!(parsed.launchers, vec![String::from("Steam")]);
        assert_eq!(parsed.launcher_order[0], "Steam");
        assert_eq!(parsed.launcher_order.len(), 8);
    }

    #[test]
    fn backend_and_frontend_share_filter_normalization_scenarios() {
        let cases = serde_json::from_str::<Vec<ContractCase>>(include_str!(
            "../../../../../testdata/games-filter-bootstrap-cases.json"
        ))
        .expect("shared bootstrap contract cases must be valid JSON");

        for case in cases {
            let actual = parse_bootstrap_filters(Some(&case.persisted.to_string()));
            assert_eq!(
                serde_json::to_value(actual).expect("effective filters must serialize"),
                case.expected,
                "shared contract case failed: {}",
                case.name
            );
        }
    }
}
