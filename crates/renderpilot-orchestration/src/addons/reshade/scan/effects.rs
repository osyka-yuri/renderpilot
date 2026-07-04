//! Detects whether a game folder already has a user ReShade effects/preset
//! setup, so a tool's default `DisabledAddons` write can leave it alone.

use std::fs;
use std::path::{Path, PathBuf};

use super::paths::{load_ini, resolve_config_path, resolve_paths, same_path, split_ini_list};

const GENERAL_SECTION: &str = "GENERAL";
const EFFECT_SEARCH_PATHS_KEY: &str = "EffectSearchPaths";
const TEXTURE_SEARCH_PATHS_KEY: &str = "TextureSearchPaths";
const PRESET_PATH_KEYS: &[&str] = &["PresetPath", "CurrentPresetPath"];
const EFFECT_EXTENSIONS: &[&str] = &["fx", "fxh"];
const TEXTURE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "dds", "bmp", "tga"];
const EFFECT_SCAN_DEPTH_LIMIT: usize = 4;
const EFFECT_SCAN_ENTRY_LIMIT: usize = 512;

/// Returns whether the game folder appears to contain user ReShade effects,
/// textures, or presets. Used before writing a tool's default `DisabledAddons`:
/// empty ReShade setups should have bundled effects disabled, while an existing
/// effects setup is left alone.
#[must_use]
pub(crate) fn has_user_effect_assets(game_dir: &Path) -> bool {
    has_direct_effect_file(game_dir)
        || configured_preset_exists(game_dir)
        || standard_effect_roots(game_dir)
            .into_iter()
            .any(|root| contains_effect_asset(&root))
        || configured_effect_roots(game_dir)
            .into_iter()
            .any(|root| contains_effect_asset(&root))
}

fn has_direct_effect_file(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && extension_matches(&entry.path(), EFFECT_EXTENSIONS)
        })
    })
}

fn configured_preset_exists(game_dir: &Path) -> bool {
    let paths = resolve_paths(game_dir, None);
    let Some(ini_path) = paths.ini_path.as_deref() else {
        return false;
    };
    let Some(ini) = load_ini(ini_path) else {
        return false;
    };
    PRESET_PATH_KEYS.iter().any(|key| {
        ini.get(GENERAL_SECTION, key)
            .map(|raw| resolve_config_path(&paths.effective_base_path, raw))
            .is_some_and(|path| path.is_file() && !same_path(&path, ini_path))
    })
}

fn standard_effect_roots(game_dir: &Path) -> Vec<PathBuf> {
    [
        game_dir.join("reshade-shaders").join("Shaders"),
        game_dir.join("reshade-shaders").join("Textures"),
        game_dir.join("Shaders"),
        game_dir.join("Textures"),
    ]
    .into_iter()
    .collect()
}

fn configured_effect_roots(game_dir: &Path) -> Vec<PathBuf> {
    let paths = resolve_paths(game_dir, None);
    let Some(ini_path) = paths.ini_path.as_deref() else {
        return Vec::new();
    };
    let Some(ini) = load_ini(ini_path) else {
        return Vec::new();
    };
    [EFFECT_SEARCH_PATHS_KEY, TEXTURE_SEARCH_PATHS_KEY]
        .into_iter()
        .filter_map(|key| ini.get(GENERAL_SECTION, key))
        .flat_map(split_ini_list)
        .map(|raw| resolve_config_path(&paths.effective_base_path, &raw))
        .filter(|path| !same_path(path, game_dir))
        .collect()
}

fn contains_effect_asset(root: &Path) -> bool {
    let mut remaining = EFFECT_SCAN_ENTRY_LIMIT;
    contains_effect_asset_inner(root, 0, &mut remaining)
}

fn contains_effect_asset_inner(path: &Path, depth: usize, remaining: &mut usize) -> bool {
    if *remaining == 0 {
        return false;
    }
    *remaining -= 1;

    if path.is_file() {
        return extension_matches(path, EFFECT_EXTENSIONS)
            || extension_matches(path, TEXTURE_EXTENSIONS);
    }
    if depth >= EFFECT_SCAN_DEPTH_LIMIT || !path.is_dir() {
        return false;
    }
    fs::read_dir(path).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| contains_effect_asset_inner(&entry.path(), depth + 1, remaining))
    })
}

fn extension_matches(path: &Path, expected: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            expected
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}
