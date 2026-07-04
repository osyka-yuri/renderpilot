//! `ReShade.ini` schema vocabulary and the additive merge transform.
//!
//! Centralizes the INI section/key names ReShade uses, which the host-detection
//! reads ([`super::scan`]) and the install merge writes share, so the two sides
//! cannot drift. The transform produces a [`MergeStrategy`] for the shared
//! [`addons::engine`](crate::addons::engine). Tool-specific *removal* strategies
//! (e.g. RenoDX's uninstall strip) live with the tool.

use crate::addons::engine::{IniSection, MergeStrategy};
use crate::addons::reshade::types::ReshadeIniTweaks;

pub(crate) const ADDON_SECTION: &str = "ADDON";
pub(crate) const INSTALL_SECTION: &str = "INSTALL";
pub(crate) const DLSS_FIX_SECTION: &str = "RENODX-DLSSFIX";
pub(crate) const ADDON_PATH_KEY: &str = "AddonPath";
pub(crate) const BASE_PATH_KEY: &str = "BasePath";
pub(crate) const DISABLED_ADDONS_KEY: &str = "DisabledAddons";
pub(crate) const LOAD_FROM_DLL_MAIN_KEY: &str = "LoadFromDllMain";

/// Builds the additive merge strategy for a tool's required `ReShade.ini` tweaks.
/// An all-empty [`ReshadeIniTweaks`] (Luma's case) still yields an `[ADDON]`
/// section with no keys — callers gate on whether any key is actually set before
/// emitting an ini op at all.
#[must_use]
pub(crate) fn ini_merge_strategy(tweaks: &ReshadeIniTweaks) -> MergeStrategy {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::reshade::types::DlssFixIniTweaks;

    fn tweaks() -> ReshadeIniTweaks {
        ReshadeIniTweaks {
            disabled_addons: vec!["Generic Depth".to_owned(), "Effect Runtime Sync".to_owned()],
            addon_path: None,
            dlss_fix: None,
        }
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
}
