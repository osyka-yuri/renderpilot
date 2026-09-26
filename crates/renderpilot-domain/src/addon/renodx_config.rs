//! Durable, RenoDX-specific provenance for managed ReShade.ini configuration.

use std::borrow::Cow;

use serde::{Deserialize, Deserializer, Serialize};

use crate::PathRef;

/// Current serialized shape of a RenoDX multi-key receipt.
pub const RENODX_CONFIG_RECEIPT_SCHEMA_VERSION: u8 = 2;

/// Closed set of RenoDX keys RenderPilot may own in `[renodx]`.
///
/// `Set_Path` is intentionally part of the durable/planner vocabulary even
/// though the public catalogue config has its own enum that excludes it:
/// processing_path is the sole authority for that key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenoDxManagedConfigKey {
    /// RenoDX processing path.
    SetPath,
    /// B8G8R8A8 typeless resource upgrade mode.
    UpgradeB8G8R8A8Typeless,
    /// B8G8R8A8 UNORM resource upgrade mode.
    UpgradeB8G8R8A8Unorm,
    /// R8G8B8A8 typeless resource upgrade mode.
    UpgradeR8G8B8A8Typeless,
    /// R8G8B8A8 UNORM resource upgrade mode.
    UpgradeR8G8B8A8Unorm,
    /// R10G10B10A2 UNORM resource upgrade mode.
    UpgradeR10G10B10A2Unorm,
    /// R10G10B10A2 typeless resource upgrade mode.
    UpgradeR10G10B10A2Typeless,
    /// R11G11B10 float resource upgrade mode.
    UpgradeR11G11B10Float,
    /// R16G16B16A16 typeless resource upgrade mode.
    UpgradeR16G16B16A16Typeless,
    /// Copy-destination resource upgrade mode.
    UpgradeCopyDestinations,
    /// Force-borderless mode.
    ForceBorderless,
    /// Use scRGB swap-chain mode.
    UpgradeUseScrgb,
    /// Swap-chain encoding mode.
    SwapchainEncoding,
    /// Compatibility scaling offset.
    ScalingOffset,
    /// Compatibility tonemap offset.
    TonemapOffset,
    /// Compatibility blit-copy workaround mode.
    BlitCopyHack,
    /// Swap-chain proxy mode.
    UseSwapchainProxy,
    /// Force pipeline cloning.
    ForcePipelineCloning,
    /// Color-grade contrast preset value.
    ColorGradeContrast,
    /// Color-grade saturation preset value.
    ColorGradeSaturation,
    /// Color-grade blowout preset value.
    ColorGradeBlowout,
}

impl RenoDxManagedConfigKey {
    /// Every managed key, including the processing-path-only `Set_Path` key.
    pub const ALL: &'static [Self] = &[
        Self::SetPath,
        Self::UpgradeB8G8R8A8Typeless,
        Self::UpgradeB8G8R8A8Unorm,
        Self::UpgradeR8G8B8A8Typeless,
        Self::UpgradeR8G8B8A8Unorm,
        Self::UpgradeR10G10B10A2Unorm,
        Self::UpgradeR10G10B10A2Typeless,
        Self::UpgradeR11G11B10Float,
        Self::UpgradeR16G16B16A16Typeless,
        Self::UpgradeCopyDestinations,
        Self::ForceBorderless,
        Self::UpgradeUseScrgb,
        Self::SwapchainEncoding,
        Self::ScalingOffset,
        Self::TonemapOffset,
        Self::BlitCopyHack,
        Self::UseSwapchainProxy,
        Self::ForcePipelineCloning,
        Self::ColorGradeContrast,
        Self::ColorGradeSaturation,
        Self::ColorGradeBlowout,
    ];

    /// Parses one exact canonical INI key name.
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "Set_Path" => Self::SetPath,
            "Upgrade_B8G8R8A8_TYPELESS" => Self::UpgradeB8G8R8A8Typeless,
            "Upgrade_B8G8R8A8_UNORM" => Self::UpgradeB8G8R8A8Unorm,
            "Upgrade_R8G8B8A8_TYPELESS" => Self::UpgradeR8G8B8A8Typeless,
            "Upgrade_R8G8B8A8_UNORM" => Self::UpgradeR8G8B8A8Unorm,
            "Upgrade_R10G10B10A2_UNORM" => Self::UpgradeR10G10B10A2Unorm,
            "Upgrade_R10G10B10A2_TYPELESS" => Self::UpgradeR10G10B10A2Typeless,
            "Upgrade_R11G11B10_FLOAT" => Self::UpgradeR11G11B10Float,
            "Upgrade_R16G16B16A16_TYPELESS" => Self::UpgradeR16G16B16A16Typeless,
            "Upgrade_CopyDestinations" => Self::UpgradeCopyDestinations,
            "ForceBorderless" => Self::ForceBorderless,
            "Upgrade_UseSCRGB" => Self::UpgradeUseScrgb,
            "Swapchain_Encoding" => Self::SwapchainEncoding,
            "Scaling_Offset" => Self::ScalingOffset,
            "Tonemap_Offset" => Self::TonemapOffset,
            "Blit_Copy_Hack" => Self::BlitCopyHack,
            "Use_Swapchain_Proxy" => Self::UseSwapchainProxy,
            "Force_Pipeline_Cloning" => Self::ForcePipelineCloning,
            "ColorGradeContrast" => Self::ColorGradeContrast,
            "ColorGradeSaturation" => Self::ColorGradeSaturation,
            "ColorGradeBlowout" => Self::ColorGradeBlowout,
            _ => return None,
        })
    }

    /// Returns the canonical INI key spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SetPath => "Set_Path",
            Self::UpgradeB8G8R8A8Typeless => "Upgrade_B8G8R8A8_TYPELESS",
            Self::UpgradeB8G8R8A8Unorm => "Upgrade_B8G8R8A8_UNORM",
            Self::UpgradeR8G8B8A8Typeless => "Upgrade_R8G8B8A8_TYPELESS",
            Self::UpgradeR8G8B8A8Unorm => "Upgrade_R8G8B8A8_UNORM",
            Self::UpgradeR10G10B10A2Unorm => "Upgrade_R10G10B10A2_UNORM",
            Self::UpgradeR10G10B10A2Typeless => "Upgrade_R10G10B10A2_TYPELESS",
            Self::UpgradeR11G11B10Float => "Upgrade_R11G11B10_FLOAT",
            Self::UpgradeR16G16B16A16Typeless => "Upgrade_R16G16B16A16_TYPELESS",
            Self::UpgradeCopyDestinations => "Upgrade_CopyDestinations",
            Self::ForceBorderless => "ForceBorderless",
            Self::UpgradeUseScrgb => "Upgrade_UseSCRGB",
            Self::SwapchainEncoding => "Swapchain_Encoding",
            Self::ScalingOffset => "Scaling_Offset",
            Self::TonemapOffset => "Tonemap_Offset",
            Self::BlitCopyHack => "Blit_Copy_Hack",
            Self::UseSwapchainProxy => "Use_Swapchain_Proxy",
            Self::ForcePipelineCloning => "Force_Pipeline_Cloning",
            Self::ColorGradeContrast => "ColorGradeContrast",
            Self::ColorGradeSaturation => "ColorGradeSaturation",
            Self::ColorGradeBlowout => "ColorGradeBlowout",
        }
    }

    /// Inclusive accepted integer range for this key.
    #[must_use]
    pub const fn value_range(self) -> (i32, i32) {
        match self {
            Self::SetPath
            | Self::ForceBorderless
            | Self::UpgradeUseScrgb
            | Self::SwapchainEncoding
            | Self::ForcePipelineCloning => (0, 1),
            Self::UpgradeCopyDestinations | Self::UseSwapchainProxy => (0, 2),
            Self::UpgradeB8G8R8A8Typeless
            | Self::UpgradeB8G8R8A8Unorm
            | Self::UpgradeR8G8B8A8Typeless
            | Self::UpgradeR8G8B8A8Unorm
            | Self::UpgradeR10G10B10A2Unorm
            | Self::UpgradeR10G10B10A2Typeless
            | Self::UpgradeR11G11B10Float
            | Self::UpgradeR16G16B16A16Typeless
            | Self::BlitCopyHack => (0, 3),
            Self::ScalingOffset | Self::TonemapOffset => (0, 8),
            Self::ColorGradeContrast | Self::ColorGradeSaturation | Self::ColorGradeBlowout => {
                (0, 100)
            }
        }
    }

    /// Returns whether a canonical integer is valid for this key.
    #[must_use]
    pub const fn accepts_value(self, value: i32) -> bool {
        let (minimum, maximum) = self.value_range();
        value >= minimum && value <= maximum
    }
}

/// The exact logical value that existed before RenoDX managed Set_Path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum RenoDxSetPathBaseline {
    /// Set_Path was absent from the unique `[renodx]` section.
    Absent,
    /// Set_Path existed; the value is retained exactly as a logical RHS.
    Present {
        /// The exact trimmed right-hand-side value from before the install.
        value: String,
    },
}

/// The typed value last written by RenderPilot. Only 0 and 1 are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenoDxSetPathValue {
    /// The native RenoDX path (`Set_Path=0`).
    Zero,
    /// The upgrade RenoDX path (`Set_Path=1`).
    One,
}

/// Provenance for one managed RenoDX key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum RenoDxManagedBaseline {
    /// The key did not exist before RenderPilot took ownership.
    Absent,
    /// The key existed with this exact logical value.
    Present {
        /// Exact right-hand-side text captured before ownership.
        value: String,
    },
}

/// Durable provenance for one typed RenoDX configuration key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxConfigEntry {
    /// Closed key name written by RenderPilot.
    pub key: String,
    /// Value captured before ownership.
    pub baseline: RenoDxManagedBaseline,
    /// Canonical integer value last written by RenderPilot.
    pub last_written: i32,
}

impl RenoDxSetPathValue {
    /// Returns the only wire representation accepted for this typed value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zero => "0",
            Self::One => "1",
        }
    }

    /// Returns the canonical numeric representation used by the INI planner.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        match self {
            Self::Zero => 0,
            Self::One => 1,
        }
    }
}

/// Receipt for one exact ReShade.ini path and all managed RenoDX keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenoDxConfigReceipt {
    /// Receipt schema version.
    pub schema_version: u8,
    /// Exact path of the ReShade.ini that was planned.
    pub ini_path: PathRef,
    /// Value captured before RenderPilot took ownership.
    pub baseline: RenoDxSetPathBaseline,
    /// Whether the target `[renodx]` section existed before the write.
    pub section_preexisted: bool,
    /// Last typed value written by RenderPilot.
    pub last_written: RenoDxSetPathValue,
    /// Exact body of the anchor line that was given a trailing newline by RenderPilot.
    #[serde(default)]
    pub newline_anchor: Option<String>,
    /// All managed keys. Empty for legacy v1 JSON until normalized by deserialization.
    #[serde(default)]
    pub entries: Vec<RenoDxConfigEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenoDxConfigReceiptWire {
    schema_version: u8,
    ini_path: PathRef,
    baseline: RenoDxSetPathBaseline,
    section_preexisted: bool,
    last_written: RenoDxSetPathValue,
    #[serde(default)]
    newline_anchor: Option<String>,
    #[serde(default)]
    entries: Option<Vec<RenoDxConfigEntry>>,
}

impl<'de> Deserialize<'de> for RenoDxConfigReceipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RenoDxConfigReceiptWire::deserialize(deserializer)?;
        let entries = match (wire.schema_version, wire.entries) {
            // Schema v1 never had a multi-key payload.  Ignore any injected
            // `entries` field and normalize the trusted legacy fields only.
            (1, _) => vec![legacy_set_path_entry(&wire.baseline, wire.last_written)],
            (_, Some(entries)) => entries,
            (_, None) => Vec::new(),
        };
        Ok(Self {
            schema_version: wire.schema_version,
            ini_path: wire.ini_path,
            baseline: wire.baseline,
            section_preexisted: wire.section_preexisted,
            last_written: wire.last_written,
            newline_anchor: wire.newline_anchor,
            entries,
        })
    }
}

fn legacy_set_path_entry(
    baseline: &RenoDxSetPathBaseline,
    last_written: RenoDxSetPathValue,
) -> RenoDxConfigEntry {
    RenoDxConfigEntry {
        key: RenoDxManagedConfigKey::SetPath.as_str().to_owned(),
        baseline: match baseline {
            RenoDxSetPathBaseline::Absent => RenoDxManagedBaseline::Absent,
            RenoDxSetPathBaseline::Present { value } => RenoDxManagedBaseline::Present {
                value: value.clone(),
            },
        },
        last_written: last_written.as_i32(),
    }
}

impl RenoDxConfigReceipt {
    /// Builds a receipt for one exact ReShade.ini path.
    #[must_use]
    pub fn new(
        ini_path: PathRef,
        baseline: RenoDxSetPathBaseline,
        section_preexisted: bool,
        last_written: RenoDxSetPathValue,
    ) -> Self {
        let entry = legacy_set_path_entry(&baseline, last_written);
        Self {
            schema_version: RENODX_CONFIG_RECEIPT_SCHEMA_VERSION,
            ini_path,
            baseline,
            section_preexisted,
            last_written,
            newline_anchor: None,
            entries: vec![entry],
        }
    }

    /// Builds a v2 receipt from the exact key provenance captured by the planner.
    #[must_use]
    pub fn from_entries(
        ini_path: PathRef,
        section_preexisted: bool,
        newline_anchor: Option<String>,
        entries: Vec<RenoDxConfigEntry>,
    ) -> Self {
        let set_path = entries
            .iter()
            .find(|entry| entry.key == RenoDxManagedConfigKey::SetPath.as_str());
        let (baseline, last_written) = match set_path {
            Some(entry) => (
                match &entry.baseline {
                    RenoDxManagedBaseline::Absent => RenoDxSetPathBaseline::Absent,
                    RenoDxManagedBaseline::Present { value } => RenoDxSetPathBaseline::Present {
                        value: value.clone(),
                    },
                },
                if entry.last_written == 0 {
                    RenoDxSetPathValue::Zero
                } else {
                    RenoDxSetPathValue::One
                },
            ),
            None => (RenoDxSetPathBaseline::Absent, RenoDxSetPathValue::Zero),
        };
        Self {
            schema_version: RENODX_CONFIG_RECEIPT_SCHEMA_VERSION,
            ini_path,
            baseline,
            section_preexisted,
            last_written,
            newline_anchor,
            entries,
        }
    }

    /// Returns v2 entries, normalizing a persisted schema-v1 receipt to its
    /// single legacy Set_Path entry.
    #[must_use]
    pub fn managed_entries(&self) -> Cow<'_, [RenoDxConfigEntry]> {
        if self.schema_version == 1 {
            return Cow::Owned(vec![legacy_set_path_entry(
                &self.baseline,
                self.last_written,
            )]);
        }
        if !self.entries.is_empty() {
            return Cow::Borrowed(&self.entries);
        }
        Cow::Owned(vec![legacy_set_path_entry(
            &self.baseline,
            self.last_written,
        )])
    }

    /// Sets the anchor line that was given a trailing newline by RenderPilot.
    #[must_use]
    pub fn with_newline_anchor(mut self, anchor: Option<String>) -> Self {
        self.newline_anchor = anchor;
        self
    }

    /// Returns whether this receipt can be acted on by the current planner.
    #[must_use]
    pub fn is_supported(&self) -> bool {
        if !self
            .ini_path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("ReShade.ini"))
            || !self
                .newline_anchor
                .as_deref()
                .is_none_or(is_safe_restorable_value)
        {
            return false;
        }
        match self.schema_version {
            1 => self.v1_is_supported(),
            RENODX_CONFIG_RECEIPT_SCHEMA_VERSION => self.v2_is_supported(),
            _ => false,
        }
    }

    fn v1_is_supported(&self) -> bool {
        match &self.baseline {
            RenoDxSetPathBaseline::Absent => true,
            // The v1 baseline is an opaque logical RHS.  An empty RHS and
            // embedded `=` are valid INI values; control line breaks and NUL
            // are unsafe at the receipt boundary.
            RenoDxSetPathBaseline::Present { value } => {
                self.section_preexisted
                    && self.newline_anchor.is_none()
                    && is_safe_restorable_value(value)
            }
        }
    }

    fn v2_is_supported(&self) -> bool {
        !self.entries.is_empty()
            && unique_keys(&self.entries)
            && self.v2_legacy_mirror_is_consistent()
            && self.v2_provenance_is_supported()
            && self.entries.iter().all(|entry| {
                RenoDxManagedConfigKey::parse(&entry.key)
                    .is_some_and(|key| key.accepts_value(entry.last_written))
                    && !entry.baseline_value_is_unsafe()
            })
    }

    fn v2_legacy_mirror_is_consistent(&self) -> bool {
        let Some(set_path) = self
            .entries
            .iter()
            .find(|entry| entry.key == RenoDxManagedConfigKey::SetPath.as_str())
        else {
            return matches!(self.baseline, RenoDxSetPathBaseline::Absent)
                && self.last_written == RenoDxSetPathValue::Zero;
        };
        let baseline_matches = match (&self.baseline, &set_path.baseline) {
            (RenoDxSetPathBaseline::Absent, RenoDxManagedBaseline::Absent) => true,
            (
                RenoDxSetPathBaseline::Present { value: legacy },
                RenoDxManagedBaseline::Present { value: entry },
            ) => legacy == entry,
            _ => false,
        };
        let value_matches = set_path.last_written == self.last_written.as_i32();
        baseline_matches && value_matches
    }

    fn v2_provenance_is_supported(&self) -> bool {
        let mut has_absent_baseline = false;
        for entry in &self.entries {
            match &entry.baseline {
                RenoDxManagedBaseline::Absent => has_absent_baseline = true,
                RenoDxManagedBaseline::Present { .. } if !self.section_preexisted => {
                    return false;
                }
                RenoDxManagedBaseline::Present { .. } => {}
            }
        }
        self.newline_anchor.is_none() || has_absent_baseline
    }
}

impl RenoDxConfigEntry {
    fn baseline_value_is_unsafe(&self) -> bool {
        match &self.baseline {
            RenoDxManagedBaseline::Absent => false,
            RenoDxManagedBaseline::Present { value } => !is_safe_restorable_value(value),
        }
    }
}

fn is_safe_restorable_value(value: &str) -> bool {
    !value.contains(['\0', '\r', '\n'])
}

fn unique_keys(entries: &[RenoDxConfigEntry]) -> bool {
    entries.iter().enumerate().all(|(index, entry)| {
        entries[..index]
            .iter()
            .all(|prior| !prior.key.eq_ignore_ascii_case(&entry.key))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_managed_keys_round_trip_and_enforce_boundaries() {
        assert_eq!(RenoDxManagedConfigKey::ALL.len(), 21);
        for key in RenoDxManagedConfigKey::ALL {
            assert_eq!(RenoDxManagedConfigKey::parse(key.as_str()), Some(*key));
            let (minimum, maximum) = key.value_range();
            assert!(key.accepts_value(minimum));
            assert!(key.accepts_value(maximum));
            assert!(!key.accepts_value(minimum - 1));
            assert!(!key.accepts_value(maximum + 1));
        }
        assert_eq!(RenoDxManagedConfigKey::parse("Upgrade_Unknown"), None);
        assert_eq!(
            RenoDxManagedConfigKey::parse("Upgrade_R8G8R8A8_TYPELESS"),
            None
        );
        assert_eq!(RenoDxManagedConfigKey::parse("set_path"), None);
    }

    #[test]
    fn schema_v1_receipt_deserializes_to_one_set_path_entry() {
        let receipt: RenoDxConfigReceipt = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "ini_path": "C:/Game/ReShade.ini",
                "baseline": {"state": "absent"},
                "section_preexisted": false,
                "last_written": "one",
                "newline_anchor": null,
                "entries": [{
                    "key": "Upgrade_Unknown",
                    "baseline": {"state": "absent"},
                    "last_written": 99
                }]
            }"#,
        )
        .expect("v1 receipt");
        assert_eq!(receipt.entries.len(), 1);
        assert_eq!(receipt.entries[0].key, "Set_Path");
        assert_eq!(receipt.managed_entries().len(), 1);
        assert_eq!(receipt.managed_entries()[0].key, "Set_Path");
        assert_eq!(receipt.entries[0].last_written, 1);
        assert!(receipt.is_supported());

        let manually_constructed = RenoDxConfigReceipt {
            entries: vec![RenoDxConfigEntry {
                key: "Upgrade_Unknown".to_owned(),
                baseline: RenoDxManagedBaseline::Absent,
                last_written: 99,
            }],
            ..receipt
        };
        assert_eq!(manually_constructed.managed_entries()[0].key, "Set_Path");
        assert!(manually_constructed.is_supported());
    }

    #[test]
    fn schema_v2_roundtrips_typed_entries_and_rejects_duplicates() {
        let mut receipt = RenoDxConfigReceipt::from_entries(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            false,
            None,
            vec![RenoDxConfigEntry {
                key: "Upgrade_R10G10B10A2_UNORM".to_owned(),
                baseline: RenoDxManagedBaseline::Absent,
                last_written: 2,
            }],
        );
        let encoded = serde_json::to_vec(&receipt).expect("encode");
        let decoded: RenoDxConfigReceipt = serde_json::from_slice(&encoded).expect("decode");
        assert_eq!(decoded, receipt);
        receipt.entries.push(receipt.entries[0].clone());
        assert!(!receipt.is_supported());
    }

    #[test]
    fn schema_v2_rejects_unknown_upgrade_keys() {
        let receipt: RenoDxConfigReceipt = serde_json::from_str(
            r#"{
                "schema_version": 2,
                "ini_path": "C:/Game/ReShade.ini",
                "baseline": {"state": "absent"},
                "section_preexisted": false,
                "last_written": "zero",
                "newline_anchor": null,
                "entries": [{
                    "key": "Upgrade_Unknown",
                    "baseline": {"state": "absent"},
                    "last_written": 1
                }]
            }"#,
        )
        .expect("v2 receipt shape");
        assert!(!receipt.is_supported());
    }

    #[test]
    fn schema_v2_rejects_inconsistent_mirrors_and_impossible_provenance() {
        let path = || PathRef::new("C:/Game/ReShade.ini").expect("path");
        let typed_entry = |baseline| RenoDxConfigEntry {
            key: "Upgrade_R10G10B10A2_UNORM".to_owned(),
            baseline,
            last_written: 2,
        };

        let mut mirror_without_set_path = RenoDxConfigReceipt::from_entries(
            path(),
            true,
            None,
            vec![typed_entry(RenoDxManagedBaseline::Absent)],
        );
        mirror_without_set_path.baseline = RenoDxSetPathBaseline::Present {
            value: "0".to_owned(),
        };
        mirror_without_set_path.last_written = RenoDxSetPathValue::One;
        assert!(!mirror_without_set_path.is_supported());

        let mut mirror_with_set_path = RenoDxConfigReceipt::from_entries(
            path(),
            true,
            None,
            vec![
                RenoDxConfigEntry {
                    key: "Set_Path".to_owned(),
                    baseline: RenoDxManagedBaseline::Absent,
                    last_written: 1,
                },
                typed_entry(RenoDxManagedBaseline::Absent),
            ],
        );
        mirror_with_set_path.baseline = RenoDxSetPathBaseline::Present {
            value: "foreign".to_owned(),
        };
        assert!(!mirror_with_set_path.is_supported());

        let created_section_with_present_baseline = RenoDxConfigReceipt::from_entries(
            path(),
            false,
            None,
            vec![typed_entry(RenoDxManagedBaseline::Present {
                value: "before".to_owned(),
            })],
        );
        assert!(!created_section_with_present_baseline.is_supported());

        let all_present_with_anchor = RenoDxConfigReceipt::from_entries(
            path(),
            true,
            Some("[renodx]".to_owned()),
            vec![typed_entry(RenoDxManagedBaseline::Present {
                value: "before".to_owned(),
            })],
        );
        assert!(!all_present_with_anchor.is_supported());
    }

    #[test]
    fn receipt_rejects_control_lines_but_preserves_opaque_rhs_values() {
        let v1_with_newline: RenoDxConfigReceipt = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "ini_path": "C:/Game/ReShade.ini",
                "baseline": {"state": "present", "value": "before\r\nafter"},
                "section_preexisted": true,
                "last_written": "one",
                "newline_anchor": null
            }"#,
        )
        .expect("v1 receipt shape");
        assert!(!v1_with_newline.is_supported());

        let v1_with_anchor = RenoDxConfigReceipt::new(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            RenoDxSetPathBaseline::Absent,
            false,
            RenoDxSetPathValue::One,
        )
        .with_newline_anchor(Some("anchor\n".to_owned()));
        assert!(!v1_with_anchor.is_supported());

        let valid_v2 = RenoDxConfigReceipt::from_entries(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            true,
            Some("Upgrade_R10G10B10A2_UNORM=2".to_owned()),
            vec![
                RenoDxConfigEntry {
                    key: "Set_Path".to_owned(),
                    baseline: RenoDxManagedBaseline::Present {
                        value: "0".to_owned(),
                    },
                    last_written: 1,
                },
                RenoDxConfigEntry {
                    key: "Upgrade_R10G10B10A2_UNORM".to_owned(),
                    baseline: RenoDxManagedBaseline::Absent,
                    last_written: 2,
                },
            ],
        );
        assert!(valid_v2.is_supported(), "mixed provenance should be valid");

        let opaque_v2 = RenoDxConfigReceipt::from_entries(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            true,
            None,
            vec![RenoDxConfigEntry {
                key: "Upgrade_R10G10B10A2_UNORM".to_owned(),
                baseline: RenoDxManagedBaseline::Present {
                    value: "=opaque=".to_owned(),
                },
                last_written: 2,
            }],
        );
        assert!(opaque_v2.is_supported());

        let empty_v2 = RenoDxConfigReceipt::from_entries(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            true,
            None,
            vec![RenoDxConfigEntry {
                key: "Upgrade_R10G10B10A2_UNORM".to_owned(),
                baseline: RenoDxManagedBaseline::Present {
                    value: String::new(),
                },
                last_written: 2,
            }],
        );
        assert!(empty_v2.is_supported());

        let v2_with_newline = RenoDxConfigReceipt::from_entries(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            false,
            None,
            vec![RenoDxConfigEntry {
                key: "Upgrade_R10G10B10A2_UNORM".to_owned(),
                baseline: RenoDxManagedBaseline::Present {
                    value: "bad\rvalue".to_owned(),
                },
                last_written: 2,
            }],
        );
        assert!(!v2_with_newline.is_supported());

        let v2_json_with_newline: RenoDxConfigReceipt = serde_json::from_str(
            r#"{
                "schema_version": 2,
                "ini_path": "C:/Game/ReShade.ini",
                "baseline": {"state": "absent"},
                "section_preexisted": false,
                "last_written": "zero",
                "newline_anchor": null,
                "entries": [{
                    "key": "Upgrade_R10G10B10A2_UNORM",
                    "baseline": {"state": "present", "value": "bad\nvalue"},
                    "last_written": 2
                }]
            }"#,
        )
        .expect("v2 receipt shape");
        assert!(!v2_json_with_newline.is_supported());

        let v2_with_bad_anchor = RenoDxConfigReceipt::from_entries(
            PathRef::new("C:/Game/ReShade.ini").expect("path"),
            false,
            Some("bad\0anchor".to_owned()),
            vec![RenoDxConfigEntry {
                key: "Upgrade_R10G10B10A2_UNORM".to_owned(),
                baseline: RenoDxManagedBaseline::Absent,
                last_written: 2,
            }],
        );
        assert!(!v2_with_bad_anchor.is_supported());
    }
}
