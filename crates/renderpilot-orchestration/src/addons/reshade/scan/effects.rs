//! Classifies the user-visible content of an existing ReShade setup.
//!
//! The classifier is deliberately stricter than the old boolean effect probe:
//! only a fully readable setup without effects, presets, textures, or foreign
//! add-ons is safe to adopt. An incomplete scan is never mistaken for an empty
//! setup.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::paths::{resolve_config_path, resolve_paths, same_path, split_ini_list};

const GENERAL_SECTION: &str = "GENERAL";
const EFFECT_SEARCH_PATHS_KEY: &str = "EffectSearchPaths";
const TEXTURE_SEARCH_PATHS_KEY: &str = "TextureSearchPaths";
const PRESET_PATH_KEYS: &[&str] = &["PresetPath", "CurrentPresetPath"];
const DEFAULT_PRESET_FILE_NAME: &str = "ReShadePreset.ini";
const EFFECT_EXTENSIONS: &[&str] = &["fx", "fxh"];
const TEXTURE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "dds", "bmp", "tga"];
const ADDON_EXTENSIONS: &[&str] = &["addon", "addon32", "addon64"];
const EFFECT_SCAN_DEPTH_LIMIT: usize = 4;
const EFFECT_SCAN_ENTRY_LIMIT: usize = 512;

/// Whether an existing ReShade tree can safely be treated as an empty runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReshadeContent {
    /// No user payload was found and every relevant location was readable.
    Empty,
    /// Effects, presets, textures, or an add-on not owned by the current tool exist.
    UserContent,
    /// A relevant path could not be inspected completely, so adoption is unsafe.
    Indeterminate,
}

impl ReshadeContent {
    #[must_use]
    pub(crate) const fn is_empty(self) -> bool {
        matches!(self, Self::Empty)
    }
}

/// Classifies effects, presets, textures, and add-ons visible to ReShade.
///
/// `allowed_addon_names` is used only by DB-loss recovery, where the add-on
/// payload already identifies the current tool. Every other add-on remains user
/// content even when it is disabled in the INI.
#[must_use]
pub(crate) fn assess_reshade_content(
    game_dir: &Path,
    allowed_addon_names: &[&str],
) -> ReshadeContent {
    let paths = resolve_paths(game_dir, None);
    let ini = match paths.ini_path.as_deref() {
        Some(path) => match fs::read_to_string(path) {
            Ok(text) => Some(crate::addons::ini::Ini::parse(&text)),
            Err(error) => {
                log::debug!(
                    "ReShade content scan: failed to read `{}`: {error}",
                    path.display()
                );
                return ReshadeContent::Indeterminate;
            }
        },
        None => None,
    };

    let mut roots = standard_effect_roots(game_dir);
    if !same_path(game_dir, &paths.effective_base_path) {
        roots.extend(standard_effect_roots(&paths.effective_base_path));
    }
    if let Some(ini) = ini.as_ref() {
        match configured_preset_exists(&paths.effective_base_path, ini, paths.ini_path.as_deref()) {
            Ok(true) => return ReshadeContent::UserContent,
            Ok(false) => {}
            Err(()) => return ReshadeContent::Indeterminate,
        }
        roots.extend(configured_effect_roots(&paths.effective_base_path, ini));
    }
    for dir in [game_dir, paths.effective_base_path.as_path()] {
        match case_insensitive_file_exists(dir, DEFAULT_PRESET_FILE_NAME) {
            Ok(true) => return ReshadeContent::UserContent,
            Ok(false) => {}
            Err(()) => return ReshadeContent::Indeterminate,
        }
    }
    for dir in [game_dir, paths.effective_base_path.as_path()] {
        match has_direct_effect_file(dir) {
            Ok(true) => return ReshadeContent::UserContent,
            Ok(false) => {}
            Err(()) => return ReshadeContent::Indeterminate,
        }
    }

    for root in deduplicate_paths(roots) {
        match contains_effect_asset(&root) {
            Ok(true) => return ReshadeContent::UserContent,
            Ok(false) => {}
            Err(()) => return ReshadeContent::Indeterminate,
        }
    }

    match contains_foreign_addon(&paths.effective_addon_path, allowed_addon_names) {
        Ok(true) => ReshadeContent::UserContent,
        Ok(false) => ReshadeContent::Empty,
        Err(()) => ReshadeContent::Indeterminate,
    }
}

fn has_direct_effect_file(dir: &Path) -> Result<bool, ()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(()),
    };
    for entry in entries {
        let entry = entry.map_err(|_| ())?;
        if entry.file_type().map_err(|_| ())?.is_file()
            && extension_matches(&entry.path(), EFFECT_EXTENSIONS)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn configured_preset_exists(
    base_path: &Path,
    ini: &crate::addons::ini::Ini,
    ini_path: Option<&Path>,
) -> Result<bool, ()> {
    for key in PRESET_PATH_KEYS {
        let Some(raw) = ini.get(GENERAL_SECTION, key) else {
            continue;
        };
        let path = resolve_config_path(base_path, raw);
        if ini_path.is_some_and(|ini| same_path(&path, ini)) {
            continue;
        }
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(()),
        }
    }
    Ok(false)
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

fn configured_effect_roots(base_path: &Path, ini: &crate::addons::ini::Ini) -> Vec<PathBuf> {
    [EFFECT_SEARCH_PATHS_KEY, TEXTURE_SEARCH_PATHS_KEY]
        .into_iter()
        .filter_map(|key| ini.get(GENERAL_SECTION, key))
        .flat_map(split_ini_list)
        .map(|raw| resolve_config_path(base_path, &raw))
        .collect()
}

fn deduplicate_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique: Vec<PathBuf> = Vec::new();
    for path in paths {
        if !unique.iter().any(|existing| same_path(existing, &path)) {
            unique.push(path);
        }
    }
    unique
}

/// Returns `Err` when a relevant existing root cannot be fully inspected.
fn contains_effect_asset(root: &Path) -> Result<bool, ()> {
    match fs::symlink_metadata(root) {
        Ok(_) => {
            let mut remaining = EFFECT_SCAN_ENTRY_LIMIT;
            contains_effect_asset_inner(root, 0, &mut remaining)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(()),
    }
}

fn contains_effect_asset_inner(
    path: &Path,
    depth: usize,
    remaining: &mut usize,
) -> Result<bool, ()> {
    if *remaining == 0 {
        return Err(());
    }
    *remaining -= 1;

    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if metadata.file_type().is_file() {
        return Ok(extension_matches(path, EFFECT_EXTENSIONS)
            || extension_matches(path, TEXTURE_EXTENSIONS));
    }
    if depth >= EFFECT_SCAN_DEPTH_LIMIT || !metadata.file_type().is_dir() {
        return Ok(false);
    }

    let entries = fs::read_dir(path).map_err(|_| ())?;
    for entry in entries {
        let entry = entry.map_err(|_| ())?;
        if contains_effect_asset_inner(&entry.path(), depth + 1, remaining)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn contains_foreign_addon(addon_dir: &Path, allowed_addon_names: &[&str]) -> Result<bool, ()> {
    match fs::symlink_metadata(addon_dir) {
        Ok(metadata) if !metadata.file_type().is_dir() => return Err(()),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(()),
    }

    let entries = fs::read_dir(addon_dir).map_err(|_| ())?;
    for entry in entries {
        let entry = entry.map_err(|_| ())?;
        let file_type = entry.file_type().map_err(|_| ())?;
        if file_type.is_file()
            && extension_matches(&entry.path(), ADDON_EXTENSIONS)
            && !allowed_addon_names.iter().any(|allowed| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(allowed)
            })
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn case_insensitive_file_exists(dir: &Path, expected_name: &str) -> Result<bool, ()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(()),
    };
    for entry in entries {
        let entry = entry.map_err(|_| ())?;
        if entry.file_type().map_err(|_| ())?.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(expected_name)
        {
            return Ok(true);
        }
    }
    Ok(false)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn dll_ini_and_logs_are_an_empty_runtime() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("dxgi.dll"), b"host").expect("host");
        fs::write(
            dir.path().join("ReShade.ini"),
            "[GENERAL]\r\nNoPreset=1\r\n",
        )
        .expect("ini");
        fs::write(dir.path().join("ReShade.log"), b"log").expect("log");

        assert_eq!(
            assess_reshade_content(dir.path(), &[]),
            ReshadeContent::Empty
        );
    }

    #[test]
    fn standard_preset_or_effect_assets_are_user_content() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("ReShadePreset.ini"), b"preset").expect("preset");
        assert_eq!(
            assess_reshade_content(dir.path(), &[]),
            ReshadeContent::UserContent
        );

        fs::remove_file(dir.path().join("ReShadePreset.ini")).expect("remove");
        fs::create_dir_all(dir.path().join("reshade-shaders").join("Shaders")).expect("dirs");
        fs::write(
            dir.path()
                .join("reshade-shaders")
                .join("Shaders")
                .join("x.fx"),
            b"effect",
        )
        .expect("effect");
        assert_eq!(
            assess_reshade_content(dir.path(), &[]),
            ReshadeContent::UserContent
        );
    }

    #[test]
    fn configured_external_content_and_foreign_addons_are_user_content() {
        let dir = tempdir().expect("tempdir");
        let external = tempdir().expect("external");
        fs::write(external.path().join("x.fx"), b"effect").expect("effect");
        fs::write(
            dir.path().join("ReShade.ini"),
            format!(
                "[GENERAL]\r\nEffectSearchPaths={}\r\n[ADDON]\r\nAddonPath=addons\r\n",
                external.path().display()
            ),
        )
        .expect("ini");
        assert_eq!(
            assess_reshade_content(dir.path(), &[]),
            ReshadeContent::UserContent
        );

        fs::remove_file(external.path().join("x.fx")).expect("remove effect");
        fs::create_dir(dir.path().join("addons")).expect("addons");
        fs::write(dir.path().join("addons").join("foreign.addon64"), b"addon").expect("addon");
        assert_eq!(
            assess_reshade_content(dir.path(), &[]),
            ReshadeContent::UserContent
        );
        assert_eq!(
            assess_reshade_content(dir.path(), &["foreign.addon64"]),
            ReshadeContent::Empty
        );
    }

    #[test]
    fn standard_assets_under_a_configured_base_path_are_user_content() {
        let dir = tempdir().expect("tempdir");
        let shaders = dir
            .path()
            .join("base")
            .join("reshade-shaders")
            .join("Shaders");
        fs::create_dir_all(&shaders).expect("dirs");
        fs::write(shaders.join("base.fx"), b"effect").expect("effect");
        fs::write(
            dir.path().join("ReShade.ini"),
            "[INSTALL]\r\nBasePath=base\r\n",
        )
        .expect("ini");

        assert_eq!(
            assess_reshade_content(dir.path(), &[]),
            ReshadeContent::UserContent
        );
    }

    #[test]
    fn scan_limit_is_indeterminate_instead_of_empty() {
        let dir = tempdir().expect("tempdir");
        let shaders = dir.path().join("reshade-shaders").join("Shaders");
        fs::create_dir_all(&shaders).expect("dirs");
        for index in 0..EFFECT_SCAN_ENTRY_LIMIT {
            fs::write(shaders.join(format!("unrelated-{index}.txt")), b"x").expect("file");
        }

        assert_eq!(
            assess_reshade_content(dir.path(), &[]),
            ReshadeContent::Indeterminate
        );
    }
}
