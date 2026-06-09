//! Data-driven catalog of NVIDIA DLSS driver-profile settings.
//!
//! A single compile-time bundled JSON (`bundled/dlss_settings.json`) is the
//! source of truth for every DLSS setting RenderPilot can write, together with
//! its value table. One generic [`CatalogSetting`] adapts each entry to the
//! [`NvapiSetting`] trait, so the orchestration / storage / DTO layers treat
//! catalog settings exactly like the (now retired) hand-coded render-preset
//! setting — adding a setting is a JSON edit, not new Rust.
//!
//! `dll_kind` is populated **only** on the three forced-preset-letter settings
//! (SR/FG/RR). That is what makes the existing DLL-version preset-manifest
//! filtering in the nvapi module apply to those — and nothing else.

use std::fmt;
use std::sync::LazyLock;

use renderpilot_nvapi::{
    setting::{NvapiValueOption, SettingContext},
    DlssDllKind, NvapiSetting, NvapiValueType,
};
use serde::Deserialize;

const BUNDLED_CATALOG: &str = include_str!("bundled/dlss_settings.json");

/// File name looked up in the local libraries directory to override the bundled
/// catalog at runtime (same mechanism the DLSS preset manifests use, and the
/// hook for future auto-update from GitHub Pages).
const CATALOG_FILE_NAME: &str = "dlss_settings.json";

static CATALOG: LazyLock<Vec<DlssSettingDef>> =
    LazyLock::new(|| load_catalog_from_disk_or_fallback(CATALOG_FILE_NAME, BUNDLED_CATALOG));

/// Prefers a valid `dlss_settings.json` in the local libraries directory; falls
/// back to the compile-time bundled copy when it is absent or malformed.
fn load_catalog_from_disk_or_fallback(file_name: &str, bundled_json: &str) -> Vec<DlssSettingDef> {
    let from_disk = crate::libraries::local_preset_manifest_path(file_name)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|json| parse_catalog(&json).ok());

    if let Some(catalog) = from_disk {
        return catalog;
    }

    crate::util::load_bundled_asset_or_default(
        || parse_catalog(bundled_json),
        Vec::new,
        "DLSS settings catalog",
    )
}

// -----------------------------------------------------------------------------
// Schema types
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct DlssSettingsCatalog {
    schema_version: u32,
    settings: Vec<DlssSettingDef>,
}

/// One DLSS setting: identity, the DRS id, optional DLL-version dependency, and
/// its full value table.
#[derive(Debug, Clone, Deserialize)]
pub struct DlssSettingDef {
    /// Wire-stable identifier, e.g. `"dlss_sr_quality_level"`.
    pub key: String,
    /// Short control label for the UI.
    pub label: String,
    /// Grouping family for the UI: `"sr"` / `"fg"` / `"rr"`.
    pub family: String,
    /// NVIDIA DRS setting id (parsed from a `"0x…"` or decimal string).
    #[serde(deserialize_with = "de_hex_u32")]
    pub nvapi_id: u32,
    /// DLL family whose version constrains valid values. Set only on the
    /// forced-preset-letter settings; `None` (absent in JSON) otherwise.
    #[serde(default, deserialize_with = "de_opt_dll_kind")]
    pub dll_kind: Option<DlssDllKind>,
    /// Optional help text shown beside the control.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional minimum driver-version hint (display only).
    #[serde(default)]
    pub min_driver: Option<String>,
    /// DWORD representing "no override", shown when the setting is absent from
    /// the profile. Defaults to `0`; declare it (JSON `"default"`) only when `0`
    /// is a real value — e.g. Forced Quality Level, where `0` means Performance
    /// and "no override" is N/A (`3`).
    #[serde(rename = "default", default, deserialize_with = "de_hex_u32")]
    pub default_dword: u32,
    /// Ordered dropdown options.
    pub values: Vec<DlssSettingValue>,
}

/// One selectable value of a [`DlssSettingDef`].
#[derive(Debug, Clone, Deserialize)]
pub struct DlssSettingValue {
    /// Human-readable label.
    pub label: String,
    /// Wire-stable string identifier used in API calls.
    pub wire: String,
    /// Raw DWORD written to the NVIDIA driver profile.
    #[serde(deserialize_with = "de_hex_u32")]
    pub dword: u32,
}

// -----------------------------------------------------------------------------
// Deserialization helpers
// -----------------------------------------------------------------------------

fn parse_u32_hex_or_dec(raw: &str) -> Result<u32, String> {
    let trimmed = raw.trim();
    let parsed = match trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => trimmed.parse::<u32>(),
    };
    parsed.map_err(|e| format!("invalid u32 `{raw}`: {e}"))
}

fn de_hex_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    parse_u32_hex_or_dec(&raw).map_err(serde::de::Error::custom)
}

fn de_opt_dll_kind<'de, D>(deserializer: D) -> Result<Option<DlssDllKind>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt.as_deref() {
        None => Ok(None),
        Some("sr") => Ok(Some(DlssDllKind::Sr)),
        Some("fg") => Ok(Some(DlssDllKind::FrameGen)),
        Some("rr") => Ok(Some(DlssDllKind::RayReconstruction)),
        Some(other) => Err(serde::de::Error::custom(format!(
            "unknown dll_kind `{other}` (expected sr | fg | rr)"
        ))),
    }
}

// -----------------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------------

/// Errors produced while parsing or validating the DLSS settings catalog JSON.
#[derive(Debug)]
pub enum CatalogError {
    /// The catalog JSON could not be deserialized.
    InvalidJson(String),
    /// The catalog `schema_version` is newer than this build supports.
    UnsupportedSchemaVersion(u32),
    /// Two settings share the same `key`.
    DuplicateKey(String),
    /// A setting has no entries in its `values` array.
    EmptyValues(String),
    /// Two values within the same setting share the same `wire` string.
    DuplicateWire {
        /// The setting key.
        key: String,
        /// The duplicated wire string.
        wire: String,
    },
    /// Two values within the same setting share the same DWORD.
    DuplicateDword {
        /// The setting key.
        key: String,
        /// The duplicated DWORD.
        dword: u32,
    },
    /// The setting's `default` DWORD is not present in its `values` array.
    DefaultNotInValues {
        /// The setting key.
        key: String,
        /// The default DWORD that has no matching value entry.
        dword: u32,
    },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(detail) => write!(f, "invalid dlss settings catalog JSON: {detail}"),
            Self::UnsupportedSchemaVersion(v) => {
                write!(f, "catalog schema_version {v} is newer than supported (1)")
            }
            Self::DuplicateKey(key) => write!(f, "duplicate setting key `{key}`"),
            Self::EmptyValues(key) => write!(f, "setting `{key}` has no values"),
            Self::DuplicateWire { key, wire } => {
                write!(f, "setting `{key}` has duplicate wire `{wire}`")
            }
            Self::DuplicateDword { key, dword } => {
                write!(f, "setting `{key}` has duplicate dword {dword}")
            }
            Self::DefaultNotInValues { key, dword } => {
                write!(
                    f,
                    "setting `{key}` default dword {dword} is not one of its values"
                )
            }
        }
    }
}

impl std::error::Error for CatalogError {}

// -----------------------------------------------------------------------------
// Loading + validation
// -----------------------------------------------------------------------------

fn parse_catalog(json: &str) -> Result<Vec<DlssSettingDef>, CatalogError> {
    let catalog: DlssSettingsCatalog =
        serde_json::from_str(json).map_err(|e| CatalogError::InvalidJson(e.to_string()))?;
    if catalog.schema_version != 1 {
        return Err(CatalogError::UnsupportedSchemaVersion(
            catalog.schema_version,
        ));
    }

    let mut seen_keys = std::collections::HashSet::new();
    for def in &catalog.settings {
        if !seen_keys.insert(def.key.as_str()) {
            return Err(CatalogError::DuplicateKey(def.key.clone()));
        }
        if def.values.is_empty() {
            return Err(CatalogError::EmptyValues(def.key.clone()));
        }
        let mut seen_wire = std::collections::HashSet::new();
        let mut seen_dword = std::collections::HashSet::new();
        for value in &def.values {
            if !seen_wire.insert(value.wire.as_str()) {
                return Err(CatalogError::DuplicateWire {
                    key: def.key.clone(),
                    wire: value.wire.clone(),
                });
            }
            if !seen_dword.insert(value.dword) {
                return Err(CatalogError::DuplicateDword {
                    key: def.key.clone(),
                    dword: value.dword,
                });
            }
        }
        if !def
            .values
            .iter()
            .any(|value| value.dword == def.default_dword)
        {
            return Err(CatalogError::DefaultNotInValues {
                key: def.key.clone(),
                dword: def.default_dword,
            });
        }
    }

    Ok(catalog.settings)
}

/// All DLSS settings, in declaration order.
pub fn catalog() -> &'static [DlssSettingDef] {
    &CATALOG
}

/// Resolves a setting definition by its wire key.
pub fn find(key: &str) -> Option<&'static DlssSettingDef> {
    CATALOG.iter().find(|def| def.key == key)
}

// -----------------------------------------------------------------------------
// `NvapiSetting` adapter
// -----------------------------------------------------------------------------

/// Generic [`NvapiSetting`] backed by a single catalog entry. Cheap to clone
/// (it holds only a `'static` reference into the parsed catalog).
#[derive(Debug, Clone, Copy)]
pub struct CatalogSetting {
    def: &'static DlssSettingDef,
}

impl CatalogSetting {
    /// Creates a new `CatalogSetting` wrapping the given definition.
    pub fn new(def: &'static DlssSettingDef) -> Self {
        Self { def }
    }
}

impl NvapiSetting for CatalogSetting {
    fn key(&self) -> &'static str {
        self.def.key.as_str()
    }

    fn label(&self) -> &'static str {
        self.def.label.as_str()
    }

    fn nvapi_id(&self) -> u32 {
        self.def.nvapi_id
    }

    fn value_type(&self) -> NvapiValueType {
        NvapiValueType::Dword
    }

    fn dll_kind(&self) -> Option<DlssDllKind> {
        self.def.dll_kind
    }

    fn enumerate_values(&self, _ctx: &SettingContext) -> Vec<NvapiValueOption> {
        self.def
            .values
            .iter()
            .map(|value| NvapiValueOption {
                wire: value.wire.clone(),
                label: value.label.clone(),
                dword: value.dword,
                supported_by_context: true,
            })
            .collect()
    }

    fn parse_wire(&self, wire: &str) -> Option<u32> {
        self.def
            .values
            .iter()
            .find(|value| value.wire == wire)
            .map(|value| value.dword)
    }

    fn format_wire(&self, dword: u32) -> Option<String> {
        self.def
            .values
            .iter()
            .find(|value| value.dword == dword)
            .map(|value| value.wire.clone())
    }

    fn label_for_dword(&self, dword: u32) -> Option<String> {
        self.def
            .values
            .iter()
            .find(|value| value.dword == dword)
            .map(|value| value.label.clone())
    }

    fn default_dword(&self) -> u32 {
        self.def.default_dword
    }

    fn family(&self) -> Option<&str> {
        Some(self.def.family.as_str())
    }

    fn description(&self) -> Option<&str> {
        self.def.description.as_deref()
    }

    fn min_driver(&self) -> Option<&str> {
        self.def.min_driver.as_deref()
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_parses_and_has_expected_keys() {
        let defs = catalog();
        assert!(!defs.is_empty());
        for key in [
            "dlss_sr_render_preset",
            "dlss_sr_quality_level",
            "dlss_sr_scaling_ratio",
            "dlss_fg_render_preset",
            "dlss_fg_mode",
            "dlss_mfg_fixed_count",
            "dlss_rr_render_preset",
            "dlss_rr_scaling_ratio",
        ] {
            assert!(find(key).is_some(), "missing catalog key {key}");
        }
    }

    #[test]
    fn dll_kind_is_set_only_on_preset_letters() {
        for def in catalog() {
            let is_preset_letter = def.key.ends_with("_render_preset");
            assert_eq!(
                def.dll_kind.is_some(),
                is_preset_letter,
                "dll_kind mismatch for {}",
                def.key
            );
        }
    }

    #[test]
    fn render_preset_dll_kind_matches_family() {
        assert_eq!(
            find("dlss_sr_render_preset").unwrap().dll_kind,
            Some(DlssDllKind::Sr)
        );
        assert_eq!(
            find("dlss_fg_render_preset").unwrap().dll_kind,
            Some(DlssDllKind::FrameGen)
        );
        assert_eq!(
            find("dlss_rr_render_preset").unwrap().dll_kind,
            Some(DlssDllKind::RayReconstruction)
        );
    }

    #[test]
    fn fg_recommended_sentinel_differs_from_sr_and_rr() {
        let recommended = |key: &str| {
            find(key)
                .unwrap()
                .values
                .iter()
                .find(|v| v.wire == "recommended")
                .unwrap()
                .dword
        };
        assert_eq!(recommended("dlss_fg_render_preset"), 0x00FF_FFFE);
        assert_eq!(recommended("dlss_sr_render_preset"), 0x00FF_FFFF);
        assert_eq!(recommended("dlss_rr_render_preset"), 0x00FF_FFFF);
    }

    #[test]
    fn known_ids_are_correct() {
        assert_eq!(find("dlss_sr_render_preset").unwrap().nvapi_id, 0x10E4_1DF3);
        assert_eq!(find("dlss_sr_quality_level").unwrap().nvapi_id, 0x10AF_B768);
        assert_eq!(find("dlss_sr_scaling_ratio").unwrap().nvapi_id, 0x10E4_1DF5);
        assert_eq!(find("dlss_mfg_fixed_count").unwrap().nvapi_id, 0x104D_6667);
        assert_eq!(find("dlss_rr_render_preset").unwrap().nvapi_id, 0x10E4_1DF7);
    }

    #[test]
    fn catalog_setting_roundtrips_every_value() {
        for def in catalog() {
            let setting = CatalogSetting::new(def);
            assert_eq!(setting.key(), def.key.as_str());
            assert_eq!(setting.value_type(), NvapiValueType::Dword);
            assert_eq!(setting.family(), Some(def.family.as_str()));
            for value in &def.values {
                assert_eq!(setting.parse_wire(&value.wire), Some(value.dword));
                assert_eq!(
                    setting.format_wire(value.dword).as_deref(),
                    Some(value.wire.as_str())
                );
                assert_eq!(
                    setting.label_for_dword(value.dword).as_deref(),
                    Some(value.label.as_str())
                );
            }
            assert!(setting.parse_wire("definitely-not-a-wire").is_none());
        }
    }

    #[test]
    fn hex_and_decimal_values_both_parse() {
        assert_eq!(parse_u32_hex_or_dec("0x10E41DF3").unwrap(), 0x10E4_1DF3);
        assert_eq!(parse_u32_hex_or_dec("0X64").unwrap(), 100);
        assert_eq!(parse_u32_hex_or_dec("16777216").unwrap(), 0x0100_0000);
        assert!(parse_u32_hex_or_dec("nope").is_err());
    }

    #[test]
    fn parse_rejects_duplicate_keys() {
        let bad = r#"{"schema_version":1,"settings":[
            {"key":"x","label":"X","family":"sr","nvapi_id":"0x1","values":[{"label":"A","wire":"a","dword":"0x0"}]},
            {"key":"x","label":"Y","family":"sr","nvapi_id":"0x2","values":[{"label":"B","wire":"b","dword":"0x0"}]}
        ]}"#;
        assert!(matches!(
            parse_catalog(bad),
            Err(CatalogError::DuplicateKey(_))
        ));
    }

    #[test]
    fn parse_rejects_duplicate_wire_within_setting() {
        let bad = r#"{"schema_version":1,"settings":[
            {"key":"x","label":"X","family":"sr","nvapi_id":"0x1","values":[
                {"label":"A","wire":"a","dword":"0x0"},
                {"label":"B","wire":"a","dword":"0x1"}
            ]}
        ]}"#;
        assert!(matches!(
            parse_catalog(bad),
            Err(CatalogError::DuplicateWire { .. })
        ));
    }

    #[test]
    fn parse_rejects_future_schema_version() {
        let bad = r#"{"schema_version":99,"settings":[]}"#;
        assert!(matches!(
            parse_catalog(bad),
            Err(CatalogError::UnsupportedSchemaVersion(99))
        ));
    }

    #[test]
    fn quality_levels_default_to_na_not_performance() {
        // 0 means "Performance" for Forced Quality Level, so its "no override"
        // default must be N/A (3), not 0 — otherwise reverting looks like a
        // real value was forced.
        assert_eq!(find("dlss_sr_quality_level").unwrap().default_dword, 3);
        assert_eq!(find("dlss_rr_quality_level").unwrap().default_dword, 3);
        // Settings where 0 already means "off / game default" keep default 0.
        assert_eq!(find("dlss_sr_render_preset").unwrap().default_dword, 0);
        assert_eq!(find("dlss_sr_dll_override").unwrap().default_dword, 0);
        // The trait surfaces it too.
        let setting = CatalogSetting::new(find("dlss_sr_quality_level").unwrap());
        assert_eq!(setting.default_dword(), 3);
    }

    #[test]
    fn parse_rejects_default_absent_from_values() {
        let bad = r#"{"schema_version":1,"settings":[
            {"key":"x","label":"X","family":"sr","nvapi_id":"0x1","default":"0x9","values":[
                {"label":"A","wire":"a","dword":"0x0"}
            ]}
        ]}"#;
        assert!(matches!(
            parse_catalog(bad),
            Err(CatalogError::DefaultNotInValues { .. })
        ));
    }
}
