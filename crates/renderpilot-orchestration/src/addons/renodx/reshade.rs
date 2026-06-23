//! ReShade host orchestration for a RenoDX install.
//!
//! RenoDX is a ReShade add-on, so a working install needs an add-on-capable
//! ReShade host next to the game executable. This module owns the host-specific
//! logic that the install flow drives:
//!
//! * **Detection** — is a ReShade host already present, and is it one *we*
//!   installed (so we may remove it) or a *foreign* one (which we reuse and must
//!   never clobber)? Ownership is recorded in a [`ReshadeMarker`] sentinel.
//! * **`reshade.ini` merge** — a pure, additive transform that sets only the
//!   keys RenoDX requires (`DisabledAddons`, and `AddonPath` when a non-default
//!   search path is needed) while preserving every other section, key, comment,
//!   and blank line, so a foreign config survives.
//!
//! Filesystem writes, backups, and journaling live in the install flow; this
//! module keeps the transforms pure and the detection read-only.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::addons::engine::{IniSection, IniSectionRemoval, MergeStrategy};

use super::types::ReshadeIniTweaks;

/// File name of the ownership sentinel RenderPilot writes when it installs the
/// ReShade host itself.
pub const MARKER_FILE_NAME: &str = "renderpilot-renodx.json";

/// Conventional ReShade configuration file name, used when creating one.
pub const RESHADE_INI_FILE_NAME: &str = "ReShade.ini";
/// Alias kept for the local case-insensitive lookups below.
const RESHADE_INI: &str = RESHADE_INI_FILE_NAME;
/// ReShade engine DLL names that signal a ReShade install regardless of the
/// proxy DLL name chosen.
const RESHADE_ENGINE_DLLS: &[&str] = &["reshade64.dll", "reshade32.dll"];
/// INI section RenoDX's required tweaks live under.
const ADDON_SECTION: &str = "ADDON";
/// INI section the DLSS-Fix add-on reads for its DLL path configuration.
const DLSS_FIX_SECTION: &str = "RENODX-DLSSFIX";

/// Ownership record written next to the game executable when RenderPilot
/// installs the ReShade host, so an uninstall can distinguish a host it may
/// remove from a foreign one it must leave intact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReshadeMarker {
    /// Marker schema version.
    pub schema_version: u32,
    /// Proxy DLL name RenderPilot installed the host as (for example `dxgi.dll`).
    pub proxy_dll: String,
    /// ReShade host version installed, when known.
    pub reshade_version: Option<String>,
    /// Add-on file name placed alongside the host.
    pub addon_file_name: String,
}

impl ReshadeMarker {
    /// Current marker schema version.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Creates a marker for a host RenderPilot is installing.
    #[must_use]
    pub fn new(
        proxy_dll: impl Into<String>,
        reshade_version: Option<String>,
        addon_file_name: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            proxy_dll: proxy_dll.into(),
            reshade_version,
            addon_file_name: addon_file_name.into(),
        }
    }
}

/// Current ReShade host state in a game folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReshadeState {
    /// No ReShade host is present; the install must provide one.
    Absent,
    /// A foreign ReShade host (user- or tool-installed) is present and must be
    /// reused without modification beyond an additive, backed-up `ReShade.ini`
    /// merge.
    Foreign,
    /// A host RenderPilot previously installed is present, described by its
    /// sentinel.
    Managed(Box<ReshadeMarker>),
}

/// Returns the path of the ownership marker within `game_dir`.
#[must_use]
pub fn marker_path(game_dir: &Path) -> PathBuf {
    game_dir.join(MARKER_FILE_NAME)
}

/// Reads the ownership marker from `game_dir`, if a valid one is present.
///
/// A stale or corrupt marker is treated as absent: callers that need to know
/// whether RenderPilot owns the host also check for the marker file's existence
/// elsewhere, so a parse failure here simply means "not managed by us".
#[must_use]
pub fn read_marker(game_dir: &Path) -> Option<ReshadeMarker> {
    let path = marker_path(game_dir);
    if !path.is_file() {
        return None;
    }
    let bytes = fs::read(&path).ok()?;
    let marker = serde_json::from_slice::<ReshadeMarker>(&bytes).ok()?;
    if marker.schema_version != ReshadeMarker::SCHEMA_VERSION {
        log::warn!(
            "RenoDX marker at `{}` has unsupported schema version {}; treating as absent",
            path.display(),
            marker.schema_version
        );
        return None;
    }
    Some(marker)
}

/// Detects the ReShade host state in a game folder.
///
/// A RenderPilot-managed host (its sentinel) takes precedence; otherwise a
/// `ReShade.ini` or a `ReShade{64,32}.dll` indicates a foreign host.
#[must_use]
pub fn detect_reshade(game_dir: &Path) -> ReshadeState {
    if let Some(marker) = read_marker(game_dir) {
        return ReshadeState::Managed(Box::new(marker));
    }
    if reshade_ini_path(game_dir).is_some() || has_reshade_engine_dll(game_dir) {
        return ReshadeState::Foreign;
    }
    ReshadeState::Absent
}

/// Returns the path to an existing `ReShade.ini` in `game_dir`, matched
/// case-insensitively (the file name casing varies across installs).
#[must_use]
pub fn reshade_ini_path(game_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(game_dir).ok()?;
    for entry in entries.flatten() {
        // Skip (don't abort on) entries whose type can't be read, so one
        // unreadable sibling never hides a real `ReShade.ini` later in the scan.
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(RESHADE_INI)
        {
            return Some(entry.path());
        }
    }
    None
}

fn has_reshade_engine_dll(game_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(game_dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        entry.file_type().ok().is_some_and(|kind| kind.is_file())
            && RESHADE_ENGINE_DLLS.contains(&name.as_str())
    })
}

/// Builds the merge strategy for RenoDX's required `ReShade.ini` tweaks.
///
/// Sets `DisabledAddons` and `AddonPath` (when a non-default search path is
/// needed) in the `[ADDON]` section; when a DLSS-Fix is installed, also sets
/// `LoadFromDllMain` in `[ADDON]` and adds a `[RENODX-DLSSFIX]` section with the
/// resolved DLL paths. The engine's [`MergeStrategy::IniSetKeys`] applies them
/// additively, preserving every other section, key, comment, and blank line,
/// with CRLF output.
#[must_use]
pub fn ini_merge_strategy(tweaks: &ReshadeIniTweaks) -> MergeStrategy {
    let mut addon_keys: Vec<(String, String)> = Vec::new();
    if !tweaks.disabled_addons.is_empty() {
        addon_keys.push((
            "DisabledAddons".to_owned(),
            tweaks.disabled_addons.join(","),
        ));
    }
    if let Some(addon_path) = &tweaks.addon_path {
        addon_keys.push(("AddonPath".to_owned(), addon_path.clone()));
    }
    // A DLSS-Fix adds `LoadFromDllMain` to [ADDON] (so the host loads the
    // companion from its DllMain) — folded into the [ADDON] keys before the
    // section is built, so there is no positional mutation after the fact.
    if let Some(dlss_fix) = &tweaks.dlss_fix {
        addon_keys.push((
            "LoadFromDllMain".to_owned(),
            dlss_fix.addon_file_name.clone(),
        ));
    }

    let mut sections = vec![IniSection {
        name: ADDON_SECTION.to_owned(),
        keys: addon_keys,
    }];

    // ...plus a [RENODX-DLSSFIX] section carrying the resolved DLL paths.
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

/// Builds the merge strategy to remove DLSS-Fix keys from `ReShade.ini`: removes
/// `LoadFromDllMain` from the `[ADDON]` section and removes the entire
/// `[RENODX-DLSSFIX]` section.
#[must_use]
pub fn ini_remove_dlss_fix_strategy() -> MergeStrategy {
    MergeStrategy::IniRemoveKeys {
        sections: vec![
            IniSectionRemoval {
                name: ADDON_SECTION.to_owned(),
                keys: vec!["LoadFromDllMain".to_owned()],
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
    use tempfile::tempdir;

    fn tweaks() -> ReshadeIniTweaks {
        ReshadeIniTweaks::renodx_defaults()
    }

    #[test]
    fn ini_merge_strategy_carries_only_the_set_keys() {
        // The byte-exact merge behavior is covered by the engine; here we assert
        // RenoDX hands the engine exactly the `[ADDON]` keys it should set.
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

        // An empty tweak set yields no keys (nothing to merge).
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
        // Two sections: [ADDON] with LoadFromDllMain, [RENODX-DLSSFIX] with paths.
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "ADDON");
        assert!(sections[0]
            .keys
            .iter()
            .any(|(k, v)| k == "LoadFromDllMain" && v == "renodx-dlssfix.addon64"));
        assert_eq!(sections[1].name, "RENODX-DLSSFIX");
        assert_eq!(
            sections[1].keys,
            vec![
                ("DLSSPath".to_owned(), r"C:\Game\nvngx_dlss.dll".to_owned()),
                (
                    "StreamlinePath".to_owned(),
                    r"C:\Game\sl.interposer.dll".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn detect_returns_absent_for_clean_folder() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("game.exe"), b"x").expect("write");
        assert_eq!(detect_reshade(dir.path()), ReshadeState::Absent);
    }

    #[test]
    fn detect_returns_foreign_for_existing_reshade_ini() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("ReShade.ini"), b"[GENERAL]\r\n").expect("write");
        assert_eq!(detect_reshade(dir.path()), ReshadeState::Foreign);
    }

    #[test]
    fn detect_returns_foreign_for_engine_dll_without_ini() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("ReShade64.dll"), b"x").expect("write");
        assert_eq!(detect_reshade(dir.path()), ReshadeState::Foreign);
    }

    #[test]
    fn detect_returns_managed_when_marker_present() {
        let dir = tempdir().expect("tempdir");
        let marker = ReshadeMarker::new("dxgi.dll", Some("6.7.3".to_owned()), "renodx-x.addon64");
        fs::write(
            marker_path(dir.path()),
            serde_json::to_vec(&marker).expect("serialize"),
        )
        .expect("write");
        // Even with a foreign-looking ini present, our marker wins.
        fs::write(dir.path().join("ReShade.ini"), b"x").expect("write");

        assert_eq!(
            detect_reshade(dir.path()),
            ReshadeState::Managed(Box::new(marker))
        );
    }

    #[test]
    fn marker_round_trips_through_disk() {
        let dir = tempdir().expect("tempdir");
        let marker = ReshadeMarker::new("d3d11.dll", None, "renodx-y.addon64");
        fs::write(
            marker_path(dir.path()),
            serde_json::to_vec(&marker).expect("serialize"),
        )
        .expect("write");
        assert_eq!(read_marker(dir.path()), Some(marker));
    }

    #[test]
    fn read_marker_rejects_unknown_schema() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            marker_path(dir.path()),
            br#"{"schema_version":999,"proxy_dll":"dxgi.dll","addon_file_name":"x"}"#,
        )
        .expect("write");
        assert_eq!(read_marker(dir.path()), None);
    }

    #[test]
    fn ini_remove_dlss_fix_strategy_strips_loadfromdllmain_and_section() {
        let strategy = ini_remove_dlss_fix_strategy();
        let base = "[ADDON]\r\nAddonPath=.\r\nLoadFromDllMain=renodx-dlssfix.addon64\r\n\
                    [RENODX-DLSSFIX]\r\nDLSSPath=C:\\d.dll\r\nStreamlinePath=C:\\s.dll\r\n";
        let merged = strategy.apply(base);
        // The [ADDON] section survives with its other keys; only LoadFromDllMain is
        // removed, and the entire [RENODX-DLSSFIX] section is gone.
        assert!(merged.contains("AddonPath=."));
        assert!(!merged.contains("LoadFromDllMain"));
        assert!(!merged.contains("RENODX-DLSSFIX"));
        assert!(!merged.contains("DLSSPath"));
    }
}
