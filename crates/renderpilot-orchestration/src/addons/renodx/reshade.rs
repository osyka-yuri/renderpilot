//! RenoDX-specific ReShade add-on state on top of the shared host scan.
//!
//! Tool-agnostic host detection and path resolution live in
//! [`crate::addons::reshade::scan`]. This module owns only RenoDX-shaped bits:
//! the `renodx-*.addon*` on-disk/config state and the detector for the minimal
//! `ReShade.ini` a RenoDX no-effects install writes.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::addons::renodx::install::DLSS_FIX_FILE_PREFIX;
use crate::addons::reshade::ini_schema::{ADDON_PATH_KEY, ADDON_SECTION, DISABLED_ADDONS_KEY};
use crate::addons::reshade::scan::{
    ReshadePaths, load_ini, read_addon_config_state, reshade_ini_path, split_ini_list,
};

/// The default `DisabledAddons` list a RenoDX no-effects install writes.
const DEFAULT_DISABLED_ADDONS: &[&str] = &["Generic Depth", "Effect Runtime Sync"];

/// State of the RenoDX add-on on disk/config for a detected host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenoDxAddonState {
    /// Whether a matching `renodx-*.addon*` file exists.
    pub present_on_disk: bool,
    /// The path the current install expects to use.
    pub expected_path: PathBuf,
    /// First matching add-on discovered in the effective add-on path.
    pub discovered_path: Option<PathBuf>,
    /// Whether `ReShade.ini` appears to allow the add-on to load. `None` means
    /// the config did not carry enough information to decide.
    pub enabled_by_config: Option<bool>,
    /// How ReShade is expected to load add-ons for this config.
    pub load_mode: RenoDxAddonLoadMode,
}

/// How the add-on will be discovered by ReShade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenoDxAddonLoadMode {
    /// ReShade will search the add-on directory.
    AutoSearch,
    /// `[ADDON] LoadFromDllMain` names an add-on.
    LoadFromDllMain,
    /// The mode cannot be determined.
    Unknown,
}

/// Returns the existing `ReShade.ini` path when it matches the minimal config
/// RenderPilot writes for a no-effects RenoDX install.
#[must_use]
pub(crate) fn renderpilot_minimal_ini_path(game_dir: &Path) -> Option<PathBuf> {
    let path = reshade_ini_path(game_dir)?;
    is_renderpilot_minimal_ini(&path).then_some(path)
}

/// Computes the RenoDX add-on state from disk and `ReShade.ini`.
#[must_use]
pub fn renodx_addon_state(paths: &ReshadePaths, addon_file_name: &str) -> RenoDxAddonState {
    let expected_path = paths.effective_addon_path.join(addon_file_name);
    let discovered_path = discover_renodx_addon(&paths.effective_addon_path);
    let ini_state = paths
        .ini_path
        .as_deref()
        .and_then(load_ini)
        .map(|ini| read_addon_config_state(&ini));
    // Soft heuristic: ReShade's `DisabledAddons` lists add-on *titles*, which for
    // RenoDX usually match the add-on file stem; we compare against the file name
    // (expected and discovered) as the best signal available without loading the
    // add-on. Used only for an informational "disabled" hint, never to gate logic.
    let enabled_by_config = ini_state.as_ref().and_then(|state| {
        if state.disabled_addons.iter().any(|value| {
            value.eq_ignore_ascii_case(addon_file_name)
                || discovered_path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .is_some_and(|name| value.eq_ignore_ascii_case(&name.to_string_lossy()))
        }) {
            Some(false)
        } else if state.has_addon_section {
            Some(true)
        } else {
            None
        }
    });
    let load_mode = ini_state
        .as_ref()
        .map(|state| {
            if state.load_from_dll_main.is_some() {
                RenoDxAddonLoadMode::LoadFromDllMain
            } else if state.has_addon_section || paths.ini_path.is_none() {
                RenoDxAddonLoadMode::AutoSearch
            } else {
                RenoDxAddonLoadMode::Unknown
            }
        })
        .unwrap_or(RenoDxAddonLoadMode::AutoSearch);

    RenoDxAddonState {
        present_on_disk: expected_path.is_file() || discovered_path.is_some(),
        expected_path,
        discovered_path,
        enabled_by_config,
        load_mode,
    }
}

fn discover_renodx_addon(addon_dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(addon_dir).ok()?;
    entries.flatten().find_map(|entry| {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            return None;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        let is_renodx_addon = crate::addons::renodx::tool::is_renodx_addon_file_name(&name);
        let is_dlss_fix = name.starts_with(DLSS_FIX_FILE_PREFIX);
        (is_renodx_addon && !is_dlss_fix).then(|| entry.path())
    })
}

fn is_renderpilot_minimal_ini(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let mut current_section: Option<String> = None;
    let mut saw_owned_key = false;

    for raw in text.lines() {
        let line = raw.trim().trim_end_matches('\r');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
            let section = line[1..line.len() - 1].trim();
            if !section.eq_ignore_ascii_case(ADDON_SECTION) {
                return false;
            }
            current_section = Some(section.to_owned());
            continue;
        }
        if !current_section
            .as_deref()
            .is_some_and(|section| section.eq_ignore_ascii_case(ADDON_SECTION))
        {
            return false;
        }
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        let key = key.trim();
        let value = value.trim();
        if key.eq_ignore_ascii_case(DISABLED_ADDONS_KEY) {
            if !is_default_disabled_addons(value) {
                return false;
            }
            saw_owned_key = true;
        } else if key.eq_ignore_ascii_case(ADDON_PATH_KEY) {
            if value.trim_matches('"') != "." {
                return false;
            }
            saw_owned_key = true;
        } else {
            return false;
        }
    }

    saw_owned_key
}

fn is_default_disabled_addons(value: &str) -> bool {
    let values = split_ini_list(value);
    values.len() == DEFAULT_DISABLED_ADDONS.len()
        && DEFAULT_DISABLED_ADDONS.iter().all(|expected| {
            values
                .iter()
                .any(|value| value.eq_ignore_ascii_case(expected))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn renodx_addon_state_reports_disabled_by_config() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("ReShade.ini"),
            "[ADDON]\r\nDisabledAddons=renodx-cp2077.addon64\r\n",
        )
        .expect("ini");
        std::fs::write(dir.path().join("renodx-cp2077.addon64"), b"x").expect("addon");
        let paths = crate::addons::reshade::scan::resolve_paths(
            dir.path(),
            Some(&dir.path().join("dxgi.dll")),
        );

        let state = renodx_addon_state(&paths, "renodx-cp2077.addon64");

        assert!(state.present_on_disk);
        assert_eq!(state.enabled_by_config, Some(false));
    }

    #[test]
    fn renodx_addon_state_does_not_report_dlss_fix_file_as_main_addon() {
        let dir = tempdir().expect("tempdir");
        // Only the DLSS-Fix companion is on disk; no real per-game addon exists.
        std::fs::write(dir.path().join("renodx-dlssfix.addon64"), b"x").expect("addon");
        let paths = crate::addons::reshade::scan::resolve_paths(
            dir.path(),
            Some(&dir.path().join("dxgi.dll")),
        );

        let state = renodx_addon_state(&paths, "renodx-cp2077.addon64");

        assert!(!state.present_on_disk);
        assert!(state.discovered_path.is_none());
    }

    #[test]
    fn renodx_addon_state_discovers_real_addon_even_with_dlss_fix_present() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("renodx-dlssfix.addon64"), b"x").expect("dlssfix");
        std::fs::write(dir.path().join("renodx-othertitle.addon64"), b"x").expect("addon");
        let paths = crate::addons::reshade::scan::resolve_paths(
            dir.path(),
            Some(&dir.path().join("dxgi.dll")),
        );

        // Expected file name differs from what's on disk, forcing discovery to run.
        let state = renodx_addon_state(&paths, "renodx-cp2077.addon64");

        assert!(state.present_on_disk);
        assert_eq!(
            state.discovered_path.as_deref().and_then(Path::file_name),
            Some(std::ffi::OsStr::new("renodx-othertitle.addon64"))
        );
    }
}
