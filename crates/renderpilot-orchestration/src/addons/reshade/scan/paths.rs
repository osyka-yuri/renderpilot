//! `ReShade.ini` path resolution and reads: where the effective base/add-on
//! paths are, and the generic `[ADDON]` config state a tool's own add-on-state
//! derivation interprets.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::addons::canonicalize_best_effort;
use crate::addons::ini::Ini;
use crate::addons::reshade::ini_schema::{
    ADDON_PATH_KEY, ADDON_SECTION, BASE_PATH_KEY, DISABLED_ADDONS_KEY, INSTALL_SECTION,
    LOAD_FROM_DLL_MAIN_KEY,
};

/// Conventional ReShade configuration file name, used when creating one.
pub const RESHADE_INI_FILE_NAME: &str = "ReShade.ini";
const RESHADE_INI: &str = RESHADE_INI_FILE_NAME;

/// Environment override ReShade honours for its base path (config/log/add-on
/// search root) when no `[INSTALL] BasePath` is set.
const RESHADE_BASE_PATH_OVERRIDE_ENV: &str = "RESHADE_BASE_PATH_OVERRIDE";

/// Effective ReShade paths derived from `ReShade.ini` and the host location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReshadePaths {
    /// Existing INI path, if present.
    pub ini_path: Option<PathBuf>,
    /// Effective base path used by ReShade.
    pub effective_base_path: PathBuf,
    /// Effective add-on search path.
    pub effective_addon_path: PathBuf,
    /// Whether `[ADDON] AddonPath` came from an absolute path.
    pub addon_path_is_absolute: bool,
}

/// Returns the path to an existing `ReShade.ini` in `game_dir`, matched
/// case-insensitively.
#[must_use]
pub fn reshade_ini_path(game_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(game_dir).ok()?;
    for entry in entries.flatten() {
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

/// Resolves ReShade's effective base and add-on paths.
#[must_use]
pub fn resolve_paths(game_dir: &Path, host_path: Option<&Path>) -> ReshadePaths {
    let default_base = host_path
        .and_then(Path::parent)
        .unwrap_or(game_dir)
        .to_path_buf();
    let ini_path = reshade_ini_path(&default_base).or_else(|| reshade_ini_path(game_dir));
    let ini = ini_path.as_deref().and_then(load_ini);

    let base_raw = ini
        .as_ref()
        .and_then(|ini| ini.get(INSTALL_SECTION, BASE_PATH_KEY))
        .map(str::to_owned)
        .or_else(|| env::var(RESHADE_BASE_PATH_OVERRIDE_ENV).ok());
    let effective_base_path = base_raw
        .as_deref()
        .map(|raw| resolve_config_path(&default_base, raw))
        .unwrap_or(default_base);

    // `[ADDON] AddonPath` is config-only — ReShade has no environment override for it.
    let addon_raw = ini
        .as_ref()
        .and_then(|ini| ini.get(ADDON_SECTION, ADDON_PATH_KEY))
        .map(str::to_owned);
    let addon_path_is_absolute = addon_raw
        .as_deref()
        .is_some_and(|raw| Path::new(raw.trim().trim_matches('"')).is_absolute());
    let effective_addon_path = addon_raw
        .as_deref()
        .map(|raw| resolve_config_path(&effective_base_path, raw))
        .unwrap_or_else(|| effective_base_path.clone());

    ReshadePaths {
        ini_path,
        effective_base_path,
        effective_addon_path,
        addon_path_is_absolute,
    }
}

/// Deletes `ReShade.log` and rotated `ReShade.log1..N` files near `base_path`.
pub fn remove_reshade_logs_best_effort(base_path: &Path) {
    for path in reshade_log_paths(base_path) {
        if let Err(error) = fs::remove_file(&path) {
            log::warn!(
                "failed to remove ReShade log `{}` during add-on cleanup: {error}",
                path.display()
            );
        }
    }
}

pub(super) fn reshade_log_paths(base_path: &Path) -> impl Iterator<Item = PathBuf> {
    let entries = fs::read_dir(base_path).ok();
    entries
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            is_reshade_log_name(&name).then(|| entry.path())
        })
}

fn is_reshade_log_name(name: &str) -> bool {
    if name == "reshade.log" {
        return true;
    }
    name.strip_prefix("reshade.log").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// Generic `[ADDON]` config state read from a `ReShade.ini`. Tool-agnostic; a
/// tool's own add-on-state derivation interprets these fields against its add-on
/// file name.
#[derive(Debug, Clone, Default)]
pub(crate) struct AddonConfigState {
    pub(crate) has_addon_section: bool,
    pub(crate) disabled_addons: Vec<String>,
    pub(crate) load_from_dll_main: Option<String>,
}

pub(crate) fn read_addon_config_state(ini: &Ini) -> AddonConfigState {
    AddonConfigState {
        has_addon_section: ini.has_section(ADDON_SECTION),
        disabled_addons: ini
            .get(ADDON_SECTION, DISABLED_ADDONS_KEY)
            .map(split_ini_list)
            .unwrap_or_default(),
        load_from_dll_main: ini
            .get(ADDON_SECTION, LOAD_FROM_DLL_MAIN_KEY)
            .map(str::to_owned),
    }
}

pub(crate) fn split_ini_list(value: &str) -> Vec<String> {
    value
        .split([',', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Reads and parses a `ReShade.ini`, returning `None` when it cannot be read.
pub(crate) fn load_ini(path: &Path) -> Option<Ini> {
    fs::read_to_string(path).ok().map(|text| Ini::parse(&text))
}

pub(super) fn resolve_config_path(base: &Path, raw: &str) -> PathBuf {
    let raw = raw.trim().trim_matches('"');
    let candidate = Path::new(raw);
    let path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    };
    canonicalize_best_effort(&path)
}

/// Path equality after best-effort canonicalization, so `.`/relative forms and
/// symlinks compare equal when the targets exist on disk.
#[must_use]
pub(crate) fn same_path(left: &Path, right: &Path) -> bool {
    canonicalize_best_effort(left) == canonicalize_best_effort(right)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn resolve_paths_reads_base_and_relative_addon_path() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("base")).expect("base");
        fs::write(
            dir.path().join("ReShade.ini"),
            "[INSTALL]\r\nBasePath=base\r\n[ADDON]\r\nAddonPath=addons\r\n",
        )
        .expect("ini");

        let paths = resolve_paths(dir.path(), Some(&dir.path().join("dxgi.dll")));

        assert!(paths.effective_base_path.ends_with("base"));
        assert!(
            paths
                .effective_addon_path
                .ends_with(Path::new("base").join("addons"))
        );
        assert!(!paths.addon_path_is_absolute);
    }

    #[test]
    fn remove_reshade_logs_removes_rotated_logs_only() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("ReShade.log"), b"x").expect("log");
        fs::write(dir.path().join("reshade.log1"), b"x").expect("log1");
        fs::write(dir.path().join("reshade.log.old"), b"x").expect("old");

        remove_reshade_logs_best_effort(dir.path());

        assert!(!dir.path().join("ReShade.log").exists());
        assert!(!dir.path().join("reshade.log1").exists());
        assert!(dir.path().join("reshade.log.old").exists());
    }
}
