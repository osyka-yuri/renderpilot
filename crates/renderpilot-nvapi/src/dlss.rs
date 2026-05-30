//! Typed API for DLSS driver-profile settings.

use crate::{
    ffi::setting_ids::DLSS_SR_RENDER_PRESET,
    setting::{DlssDllKind, NvapiSetting, NvapiValueOption, NvapiValueType, SettingContext},
};

/// Generates `DlssRenderPreset` and all related helpers from a single table.
///
/// Each arm is: `Variant => label, wire, dword`
macro_rules! define_preset {
    (
        $DefaultLabel:literal, $DefaultWire:literal, $DefaultDword:literal;
        $($Variant:ident => $Label:literal, $Wire:literal, $Dword:literal);* $(;)?
    ) => {
        /// DLSS Super Resolution forced render presets.
        ///
        /// Maps to the `DLSS_SR_RENDER_PRESET` DRS DWORD setting.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub enum DlssRenderPreset {
            /// Game-controlled default.
            #[default]
            Default,
            $($Variant,)*
        }

        impl DlssRenderPreset {
            /// All known variants in a stable order.
            pub const ALL: &'static [Self] = &[Self::Default, $(Self::$Variant,)*];

            /// Human-readable label used in the UI.
            pub const fn label(self) -> &'static str {
                match self {
                    Self::Default => $DefaultLabel,
                    $(Self::$Variant => $Label,)*
                }
            }

            /// Wire value used for serde and storage.
            pub const fn as_wire(self) -> &'static str {
                match self {
                    Self::Default => $DefaultWire,
                    $(Self::$Variant => $Wire,)*
                }
            }

            /// Raw DWORD value sent to the driver.
            pub const fn to_dword(self) -> u32 {
                match self {
                    Self::Default => $DefaultDword,
                    $(Self::$Variant => $Dword,)*
                }
            }

            /// Parses a wire value.
            pub fn from_wire(value: &str) -> Option<Self> {
                match value {
                    $DefaultWire => Some(Self::Default),
                    $($Wire => Some(Self::$Variant),)*
                    _ => None,
                }
            }

            /// Parses a raw DWORD value returned by the driver.
            pub fn from_dword(value: u32) -> Option<Self> {
                match value {
                    $DefaultDword => Some(Self::Default),
                    $($Dword => Some(Self::$Variant),)*
                    _ => None,
                }
            }
        }
    };
}

define_preset! {
    "Default", "default", 0;
    PresetA => "Preset A", "a", 1;
    PresetB => "Preset B", "b", 2;
    PresetC => "Preset C", "c", 3;
    PresetD => "Preset D", "d", 4;
    PresetE => "Preset E", "e", 5;
    PresetF => "Preset F", "f", 6;
    PresetG => "Preset G", "g", 7;
    PresetH => "Preset H", "h", 8;
    PresetI => "Preset I", "i", 9;
    PresetJ => "Preset J", "j", 10;
    PresetK => "Preset K", "k", 11;
    PresetL => "Preset L", "l", 12;
    PresetM => "Preset M", "m", 13;
    PresetN => "Preset N", "n", 14;
    PresetO => "Preset O", "o", 15;
    // 0x00ffffff = "Latest" — always activates the newest preset NVIDIA ships
    // with the current driver. Exposed in Inspector as a special entry.
    Latest => "Latest", "latest", 0x00ff_ffff;
}

// -----------------------------------------------------------------------------
// `NvapiSetting` impl
// -----------------------------------------------------------------------------

/// Concrete `NvapiSetting` implementation encapsulating the DLSS Super Resolution 
/// forced render preset (mapping directly to the `DLSS_SR_RENDER_PRESET` DRS DWORD).
///
/// This is deliberately architected as a concrete struct rather than utilizing module-level statics. 
/// This design enforces polymorphism, enabling the higher-level orchestration layer to homogenously 
/// maintain a registry of `Arc<dyn NvapiSetting>` encompassing all future setting implementations 
/// (e.g., DLAA, NVIDIA Reflex, Frame Generation).
#[derive(Debug, Default, Clone, Copy)]
pub struct DlssRenderPresetSetting;

impl DlssRenderPresetSetting {
    /// Stable wire key. Kept in a `const` so storage and Tauri commands
    /// can reference it without instantiating the struct.
    pub const KEY: &'static str = "dlss_sr_render_preset";
}

impl NvapiSetting for DlssRenderPresetSetting {
    fn key(&self) -> &'static str {
        Self::KEY
    }

    fn label(&self) -> &'static str {
        "DLSS Super Resolution Render Preset"
    }

    fn nvapi_id(&self) -> u32 {
        DLSS_SR_RENDER_PRESET
    }

    fn value_type(&self) -> NvapiValueType {
        NvapiValueType::Dword
    }

    fn dll_kind(&self) -> Option<DlssDllKind> {
        Some(DlssDllKind::Sr)
    }

    fn enumerate_values(&self, _ctx: &SettingContext) -> Vec<NvapiValueOption> {
        // Initially yields all variants with `supported_by_context = true`.
        // The authoritative orchestration layer is subsequently responsible for applying 
        // the downloaded preset manifest, deterministically flipping unsupported variants 
        // to `false` based on the active DLL version heuristic.
        DlssRenderPreset::ALL
            .iter()
            .map(|&preset| NvapiValueOption {
                wire: preset.as_wire().to_owned(),
                label: preset.label().to_owned(),
                dword: preset.to_dword(),
                supported_by_context: true,
            })
            .collect()
    }

    fn parse_wire(&self, wire: &str) -> Option<u32> {
        DlssRenderPreset::from_wire(wire).map(|p| p.to_dword())
    }

    fn format_wire(&self, dword: u32) -> Option<String> {
        DlssRenderPreset::from_dword(dword).map(|p| p.as_wire().to_owned())
    }

    fn label_for_dword(&self, dword: u32) -> Option<String> {
        DlssRenderPreset::from_dword(dword).map(|p| p.label().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_dword_roundtrip() {
        for &preset in DlssRenderPreset::ALL {
            let dword = preset.to_dword();
            let back = DlssRenderPreset::from_dword(dword).unwrap();
            assert_eq!(preset, back);
        }
    }

    #[test]
    fn preset_wire_roundtrip() {
        for &preset in DlssRenderPreset::ALL {
            let wire = preset.as_wire();
            let back = DlssRenderPreset::from_wire(wire).unwrap();
            assert_eq!(preset, back);
        }
    }

    #[test]
    fn preset_default_values() {
        assert_eq!(DlssRenderPreset::Default.to_dword(), 0);
        assert_eq!(DlssRenderPreset::Default.as_wire(), "default");
        assert_eq!(DlssRenderPreset::PresetA.to_dword(), 1);
        assert_eq!(DlssRenderPreset::PresetK.to_dword(), 11);
        assert_eq!(DlssRenderPreset::PresetL.to_dword(), 12);
        assert_eq!(DlssRenderPreset::PresetM.to_dword(), 13);
        assert_eq!(DlssRenderPreset::PresetN.to_dword(), 14);
        assert_eq!(DlssRenderPreset::PresetO.to_dword(), 15);
        assert_eq!(DlssRenderPreset::Latest.to_dword(), 0x00ff_ffff);
        assert_eq!(DlssRenderPreset::Latest.as_wire(), "latest");
    }

    #[test]
    fn preset_all_includes_new_presets() {
        // Default(0) + Presets A-O (1-15) + Latest (0x00ffffff) = 17 entries.
        assert_eq!(DlssRenderPreset::ALL.len(), 17);
        // The first 16 entries span dwords 0-15 sequentially.
        let sequential: Vec<u32> = DlssRenderPreset::ALL[..16]
            .iter()
            .map(|p| p.to_dword())
            .collect();
        assert_eq!(sequential, (0u32..16).collect::<Vec<_>>());
        // The last entry is the special "Latest" sentinel.
        assert_eq!(DlssRenderPreset::ALL[16].to_dword(), 0x00ff_ffff);
    }

    #[test]
    fn preset_from_dword_out_of_range() {
        // 16 is not a defined preset (gap between PresetO=15 and Latest=0x00ffffff).
        assert!(DlssRenderPreset::from_dword(16).is_none());
        assert!(DlssRenderPreset::from_dword(u32::MAX).is_none());
        // But 15 and 0x00ffffff are valid.
        assert!(DlssRenderPreset::from_dword(15).is_some());
        assert!(DlssRenderPreset::from_dword(0x00ff_ffff).is_some());
    }

    #[test]
    fn preset_from_wire_unknown() {
        assert!(DlssRenderPreset::from_wire("unknown").is_none());
        assert!(DlssRenderPreset::from_wire("").is_none());
    }

    #[test]
    fn render_preset_setting_implements_trait_correctly() {
        use crate::setting::{NvapiSetting, NvapiValueType, SettingContext};
        use std::collections::HashMap;
        use std::path::PathBuf;

        let setting: &dyn NvapiSetting = &DlssRenderPresetSetting;
        assert_eq!(setting.key(), "dlss_sr_render_preset");
        assert_eq!(setting.nvapi_id(), DLSS_SR_RENDER_PRESET);
        assert_eq!(setting.value_type(), NvapiValueType::Dword);
        assert_eq!(setting.dll_kind(), Some(crate::setting::DlssDllKind::Sr));

        let ctx = SettingContext {
            game_install_dir: PathBuf::from("/tmp"),
            dlls: HashMap::new(),
            effective_exe: None,
        };
        let options = setting.enumerate_values(&ctx);
        // Default + A-O (15 letter presets) + Latest = 17
        assert_eq!(options.len(), 17);
        assert_eq!(options[0].wire, "default");
        assert_eq!(options[0].dword, 0);
        assert_eq!(options[15].wire, "o");
        assert_eq!(options[15].dword, 15);
        assert_eq!(options[16].wire, "latest");
        assert_eq!(options[16].dword, 0x00ff_ffff);
        assert!(options.iter().all(|o| o.supported_by_context));
    }

    #[test]
    fn render_preset_setting_parse_format_roundtrip() {
        use crate::setting::NvapiSetting;

        let setting = DlssRenderPresetSetting;
        for wire in ["default", "a", "f", "k", "l", "m", "n", "o", "latest"] {
            let dword = setting.parse_wire(wire).unwrap();
            assert_eq!(setting.format_wire(dword).as_deref(), Some(wire));
        }
        assert!(setting.parse_wire("zz").is_none());
        // 16 is still an unmapped raw dword.
        assert!(setting.format_wire(16).is_none());
    }

    #[test]
    fn render_preset_setting_labels_match_enum() {
        use crate::setting::NvapiSetting;

        let setting = DlssRenderPresetSetting;
        assert_eq!(setting.label_for_dword(0).as_deref(), Some("Default"));
        assert_eq!(setting.label_for_dword(6).as_deref(), Some("Preset F"));
        assert_eq!(setting.label_for_dword(13).as_deref(), Some("Preset M"));
        assert_eq!(setting.label_for_dword(15).as_deref(), Some("Preset O"));
        assert_eq!(
            setting.label_for_dword(0x00ff_ffff).as_deref(),
            Some("Latest")
        );
        // 16 is still unmapped.
        assert!(setting.label_for_dword(16).is_none());
    }
}
