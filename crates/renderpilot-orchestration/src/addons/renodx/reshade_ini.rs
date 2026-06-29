//! `ReShade.ini` schema vocabulary and the merge transforms RenoDX applies.
//!
//! Centralizes the INI section/key names RenoDX both **reads** (path resolution and
//! add-on state in [`super::reshade`]) and **writes** (the install and DLSS-Fix
//! merge strategies here), so the two sides cannot drift. The transforms produce a
//! [`MergeStrategy`] for the shared [`addons::engine`](crate::addons::engine); the
//! host-detection model and the INI *reads* live in [`super::reshade`].

use crate::addons::engine::{IniSection, IniSectionRemoval, MergeStrategy};

use super::types::ReshadeIniTweaks;

pub(super) const ADDON_SECTION: &str = "ADDON";
pub(super) const INSTALL_SECTION: &str = "INSTALL";
pub(super) const DLSS_FIX_SECTION: &str = "RENODX-DLSSFIX";
pub(super) const ADDON_PATH_KEY: &str = "AddonPath";
pub(super) const BASE_PATH_KEY: &str = "BasePath";
pub(super) const DISABLED_ADDONS_KEY: &str = "DisabledAddons";
pub(super) const LOAD_FROM_DLL_MAIN_KEY: &str = "LoadFromDllMain";

/// Builds the merge strategy for RenoDX's required `ReShade.ini` tweaks.
#[must_use]
pub(super) fn ini_merge_strategy(tweaks: &ReshadeIniTweaks) -> MergeStrategy {
    let mut addon_keys: Vec<(String, String)> = Vec::new();
    if !tweaks.disabled_addons.is_empty() {
        addon_keys.push((
            DISABLED_ADDONS_KEY.to_owned(),
            tweaks.disabled_addons.join(","),
        ));
    }
    if let Some(addon_path) = &tweaks.addon_path {
        addon_keys.push((ADDON_PATH_KEY.to_owned(), addon_path.clone()));
    }
    if let Some(dlss_fix) = &tweaks.dlss_fix {
        addon_keys.push((
            LOAD_FROM_DLL_MAIN_KEY.to_owned(),
            dlss_fix.addon_file_name.clone(),
        ));
    }

    let mut sections = vec![IniSection {
        name: ADDON_SECTION.to_owned(),
        keys: addon_keys,
    }];

    if let Some(dlss_fix) = &tweaks.dlss_fix {
        sections.push(IniSection {
            name: DLSS_FIX_SECTION.to_owned(),
            keys: vec![
                ("DLSSPath".to_owned(), dlss_fix.dlss_path.clone()),
                (
                    "StreamlinePath".to_owned(),
                    dlss_fix.streamline_path.clone(),
                ),
            ],
        });
    }

    MergeStrategy::IniSetKeys { sections }
}

/// Builds the merge strategy to remove DLSS-Fix keys from `ReShade.ini`.
#[must_use]
pub(super) fn ini_remove_dlss_fix_strategy() -> MergeStrategy {
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

#[cfg(test)]
mod tests {
    use super::super::types::DlssFixIniTweaks;
    use super::*;

    fn tweaks() -> ReshadeIniTweaks {
        ReshadeIniTweaks::renodx_defaults()
    }

    #[test]
    fn ini_merge_strategy_carries_only_the_set_keys() {
        let MergeStrategy::IniSetKeys { sections } = ini_merge_strategy(&tweaks()) else {
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

        let MergeStrategy::IniSetKeys { sections, .. } = ini_merge_strategy(&ReshadeIniTweaks {
            disabled_addons: Vec::new(),
            addon_path: None,
            dlss_fix: None,
        }) else {
            panic!("expected IniSetKeys")
        };
        assert!(sections[0].keys.is_empty());
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
}
