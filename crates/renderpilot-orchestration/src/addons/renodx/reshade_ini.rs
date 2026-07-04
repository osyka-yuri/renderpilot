//! RenoDX's `ReShade.ini` uninstall/DLSS-Fix removal transforms.
//!
//! The `[ADDON]`/`[INSTALL]` schema constants and the additive
//! `ini_merge_strategy` write transform are shared at
//! [`crate::addons::reshade::ini_schema`]. This module owns only the
//! RenoDX-shaped *removal* strategies used on uninstall.

use crate::addons::engine::{IniSectionRemoval, MergeStrategy};
use crate::addons::reshade::ini_schema::{
    ADDON_PATH_KEY, ADDON_SECTION, DISABLED_ADDONS_KEY, DLSS_FIX_SECTION, LOAD_FROM_DLL_MAIN_KEY,
};

/// Builds the merge strategy to remove DLSS-Fix keys from `ReShade.ini`.
#[must_use]
pub(crate) fn ini_remove_dlss_fix_strategy() -> MergeStrategy {
    MergeStrategy::IniRemoveKeys {
        sections: vec![
            IniSectionRemoval {
                name: ADDON_SECTION.to_owned(),
                keys: vec![LOAD_FROM_DLL_MAIN_KEY.to_owned()],
            },
            IniSectionRemoval {
                name: DLSS_FIX_SECTION.to_owned(),
                keys: Vec::new(),
            },
        ],
    }
}

/// Builds the merge strategy an uninstall applies to a `ReShade.ini` RenoDX did
/// not create from scratch (so it is never blanket-deleted): removes exactly the
/// keys/sections RenoDX itself ever writes there — `[ADDON]` `DisabledAddons`,
/// `AddonPath`, and (when a DLSS-Fix companion was installed) `LoadFromDllMain`
/// plus the whole `[RENODX-DLSSFIX]` section — leaving every other key, section,
/// comment, and blank line (including the user's own settings) untouched.
#[must_use]
pub(crate) fn ini_remove_renodx_strategy() -> MergeStrategy {
    MergeStrategy::IniRemoveKeys {
        sections: vec![
            IniSectionRemoval {
                name: ADDON_SECTION.to_owned(),
                keys: vec![
                    DISABLED_ADDONS_KEY.to_owned(),
                    ADDON_PATH_KEY.to_owned(),
                    LOAD_FROM_DLL_MAIN_KEY.to_owned(),
                ],
            },
            IniSectionRemoval {
                name: DLSS_FIX_SECTION.to_owned(),
                keys: Vec::new(),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::types::renodx_ini_defaults;
    use crate::addons::reshade::ini_schema::ini_merge_strategy;
    use crate::addons::reshade::types::{DlssFixIniTweaks, ReshadeIniTweaks};

    #[test]
    fn ini_merge_strategy_carries_only_the_set_keys() {
        let MergeStrategy::IniSetKeys { sections } = ini_merge_strategy(&renodx_ini_defaults())
        else {
            panic!("expected IniSetKeys")
        };
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "ADDON");
        assert_eq!(
            sections[0].keys,
            vec![(
                "DisabledAddons".to_owned(),
                "Generic Depth,Effect Runtime Sync".to_owned()
            )]
        );
    }

    #[test]
    fn ini_merge_strategy_adds_dlss_fix_sections_when_present() {
        let tweaks = ReshadeIniTweaks {
            disabled_addons: vec!["Generic Depth".to_owned()],
            addon_path: None,
            dlss_fix: Some(DlssFixIniTweaks {
                addon_file_name: "renodx-dlssfix.addon64".to_owned(),
                dlss_path: r"C:\Game\nvngx_dlss.dll".to_owned(),
                streamline_path: r"C:\Game\sl.interposer.dll".to_owned(),
            }),
        };
        let MergeStrategy::IniSetKeys { sections } = ini_merge_strategy(&tweaks) else {
            panic!("expected IniSetKeys")
        };
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "ADDON");
        assert!(
            sections[0]
                .keys
                .iter()
                .any(|(k, v)| k == "LoadFromDllMain" && v == "renodx-dlssfix.addon64")
        );
        assert_eq!(sections[1].name, "RENODX-DLSSFIX");
    }

    #[test]
    fn ini_remove_dlss_fix_strategy_strips_loadfromdllmain_and_section() {
        let strategy = ini_remove_dlss_fix_strategy();
        let base = "[ADDON]\r\nAddonPath=.\r\nLoadFromDllMain=renodx-dlssfix.addon64\r\n\
                    [RENODX-DLSSFIX]\r\nDLSSPath=C:\\d.dll\r\nStreamlinePath=C:\\s.dll\r\n";
        let merged = strategy.apply(base);
        assert!(merged.contains("AddonPath=."));
        assert!(!merged.contains("LoadFromDllMain"));
        assert!(!merged.contains("RENODX-DLSSFIX"));
    }

    #[test]
    fn ini_remove_renodx_strategy_strips_only_renodx_keys() {
        let strategy = ini_remove_renodx_strategy();
        let base = "; user comment\r\n\
                    [GENERAL]\r\nPreset=mine.ini\r\n\r\n\
                    [ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n\
                    AddonPath=.\r\nLoadFromDllMain=renodx-dlssfix.addon64\r\n\
                    UserAddonKey=keep-me\r\n\r\n\
                    [RENODX-DLSSFIX]\r\nDLSSPath=C:\\d.dll\r\nStreamlinePath=C:\\s.dll\r\n";
        let merged = strategy.apply(base);

        // User settings outside RenoDX's own keys/section survive untouched.
        assert!(merged.contains("; user comment"));
        assert!(merged.contains("[GENERAL]"));
        assert!(merged.contains("Preset=mine.ini"));
        assert!(merged.contains("UserAddonKey=keep-me"));
        // RenoDX's own keys and section are gone.
        assert!(!merged.contains("DisabledAddons"));
        assert!(!merged.contains("AddonPath"));
        assert!(!merged.contains("LoadFromDllMain"));
        assert!(!merged.contains("RENODX-DLSSFIX"));
    }

    #[test]
    fn ini_remove_renodx_strategy_is_a_no_op_on_a_foreign_config() {
        let strategy = ini_remove_renodx_strategy();
        let base = "[GENERAL]\r\nPreset=mine.ini\r\n";
        assert_eq!(strategy.apply(base), base);
    }
}
