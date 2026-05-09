//! Registry and manifest based discovery of game install folders.
//!
//! Discovers per-game install directories for common Windows game launchers:
//! Steam, Epic Games Store, GOG Galaxy, EA App / Origin, and Ubisoft Connect.
//!
//! Internally, sources are split into two kinds:
//!
//! * Per-game install paths come straight from per-app registry keys or per-app
//!   manifest files. Each path already points at one game install folder.
//! * Launcher library roots are container folders (e.g. `steamapps/common`) that
//!   hold many games as their immediate sub-directories. Their children are
//!   enumerated and surfaced as individual game install paths.
//!
//! This separation prevents launcher container folders themselves from being
//! treated as a single "game" by the catalog scan.

use serde::Deserialize;
use std::{
    collections::HashSet,
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};
use winreg::{
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY},
    RegKey, HKEY,
};

use crate::steam_appmanifest::steam_install_dirs_in_steamapps;

const REGISTRY_READ_FLAGS: &[u32] = &[
    KEY_READ,
    KEY_READ | KEY_WOW64_64KEY,
    KEY_READ | KEY_WOW64_32KEY,
];

/// Game sources discovered from common Windows launchers.
#[derive(Debug, Default, Clone)]
pub struct DiscoveredGameSources {
    /// Per-game install folders ready for scanning.
    ///
    /// Includes both per-app paths (registry / per-game manifests) and the
    /// immediate sub-directories of every discovered launcher library root.
    /// Launcher library roots themselves are NOT included.
    pub install_paths: Vec<PathBuf>,
    /// Existing launcher library roots (e.g. `steamapps/common`,
    /// `Program Files/EA Games`).
    ///
    /// Useful for catalog-side cleanup of stale entries that were created when
    /// a launcher root was previously persisted as a single game.
    pub library_roots: Vec<PathBuf>,
}

/// Discovers game sources from common Windows launchers.
///
/// Returned paths in [`DiscoveredGameSources::install_paths`] are existing
/// directories that point at one game install each. Launcher library roots
/// are returned separately in [`DiscoveredGameSources::library_roots`] and
/// are never used as scan targets directly.
///
/// Only existing directories are returned. Invalid registry entries, missing
/// files, malformed launcher manifests, and inaccessible paths are ignored.
pub fn discover_game_sources() -> DiscoveredGameSources {
    let mut sources = DiscoveredSources::default();

    sources.merge(discover_steam_libraries());
    sources.merge(discover_epic_games_libraries());
    sources.merge(discover_gog_libraries());
    sources.merge(discover_ea_libraries());
    sources.merge(discover_ubisoft_libraries());

    sources.finalize()
}

/// Internal collection of discovery results, split by source kind.
#[derive(Debug, Default)]
struct DiscoveredSources {
    /// Paths that already point at one game install folder (per-app
    /// registry keys, Epic per-game manifests, etc.).
    game_installs: Vec<PathBuf>,
    /// Container folders whose immediate sub-directories are game installs.
    /// All children are enumerated unconditionally.
    library_roots: Vec<PathBuf>,
    /// Steam `steamapps/common` paths.
    ///
    /// Children are filtered to only those whose folder name appears as
    /// `installdir` in some `appmanifest_*.acf` in the parent `steamapps/`
    /// directory. This keeps Steam runtime / shared sub-folders such as
    /// `Steam Controller Configs`, `Steamworks Common Redistributables`,
    /// or `Steamworks Shared` out of the install path list.
    steam_common_roots: Vec<PathBuf>,
}

impl DiscoveredSources {
    fn merge(&mut self, other: DiscoveredSources) {
        self.game_installs.extend(other.game_installs);
        self.library_roots.extend(other.library_roots);
        self.steam_common_roots.extend(other.steam_common_roots);
    }

    fn finalize(self) -> DiscoveredGameSources {
        let library_roots = existing_unique_dirs(self.library_roots.iter().cloned());
        let steam_common_roots = existing_unique_dirs(self.steam_common_roots.iter().cloned());

        let regular_children = enumerate_library_root_children(&library_roots);
        let steam_children = enumerate_steam_common_root_children(&steam_common_roots);

        let combined = self
            .game_installs
            .into_iter()
            .chain(regular_children)
            .chain(steam_children);
        let install_paths = existing_unique_dirs(combined);

        let all_library_roots = library_roots
            .into_iter()
            .chain(steam_common_roots)
            .collect::<Vec<_>>();

        DiscoveredGameSources {
            install_paths,
            library_roots: all_library_roots,
        }
    }
}

fn enumerate_library_root_children(library_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut children = Vec::new();

    for root in library_roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };

        for entry in entries.filter_map(Result::ok) {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                children.push(entry.path());
            }
        }
    }

    children
}

/// Enumerates direct sub-directories of each Steam `steamapps/common` root,
/// keeping only those that match an `installdir` declared by a manifest in
/// the sibling `steamapps/` directory.
fn enumerate_steam_common_root_children(common_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut children = Vec::new();

    for common in common_roots {
        let Some(steamapps_dir) = common.parent() else {
            continue;
        };

        let allowed_installdirs = steam_install_dirs_in_steamapps(steamapps_dir);

        if allowed_installdirs.is_empty() {
            continue;
        }

        let Ok(entries) = fs::read_dir(common) else {
            continue;
        };

        for entry in entries.filter_map(Result::ok) {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if !file_type.is_dir() {
                continue;
            }

            let Some(name) = entry.file_name().to_str().map(str::to_ascii_lowercase) else {
                continue;
            };

            if allowed_installdirs.contains(&name) {
                children.push(entry.path());
            }
        }
    }

    children
}

fn discover_steam_libraries() -> DiscoveredSources {
    let steam_roots = read_registry_values(
        HKEY_CURRENT_USER,
        &[r"Software\Valve\Steam"],
        &["SteamPath", "InstallPath"],
    )
    .into_iter()
    .chain(read_registry_values(
        HKEY_LOCAL_MACHINE,
        &[r"SOFTWARE\Valve\Steam", r"SOFTWARE\WOW6432Node\Valve\Steam"],
        &["SteamPath", "InstallPath"],
    ));

    let mut steam_common_roots = Vec::new();

    for steam_root in steam_roots {
        steam_common_roots.push(steam_root.join("steamapps").join("common"));

        let libraryfolders_vdf = steam_root.join("steamapps").join("libraryfolders.vdf");

        let Ok(content) = fs::read_to_string(libraryfolders_vdf) else {
            continue;
        };

        for library_root in parse_steam_library_roots(&content) {
            steam_common_roots.push(library_root.join("steamapps").join("common"));
        }
    }

    DiscoveredSources {
        game_installs: Vec::new(),
        library_roots: Vec::new(),
        steam_common_roots,
    }
}

fn discover_epic_games_libraries() -> DiscoveredSources {
    #[derive(Debug, Deserialize)]
    struct EpicManifest {
        #[serde(rename = "InstallLocation")]
        install_location: Option<String>,
    }

    let mut game_installs = Vec::new();

    let manifests_dir = env_path("PROGRAMDATA")
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests");

    if let Ok(entries) = fs::read_dir(manifests_dir) {
        for entry in entries.filter_map(Result::ok) {
            let manifest_path = entry.path();

            if !has_extension_ignore_ascii_case(&manifest_path, "item") {
                continue;
            }

            let Ok(content) = fs::read_to_string(manifest_path) else {
                continue;
            };

            let Ok(manifest) = serde_json::from_str::<EpicManifest>(&content) else {
                continue;
            };

            let Some(install_location) = manifest.install_location else {
                continue;
            };

            if let Some(path) = path_from_string(&install_location) {
                game_installs.push(path);
            }
        }
    }

    let mut library_roots = Vec::new();

    push_env_join(&mut library_roots, "ProgramFiles", &["Epic Games"]);
    library_roots.push(PathBuf::from(r"C:\Program Files\Epic Games"));

    DiscoveredSources {
        game_installs,
        library_roots,
        ..DiscoveredSources::default()
    }
}

fn discover_gog_libraries() -> DiscoveredSources {
    let mut game_installs = Vec::new();

    game_installs.extend(discover_registry_paths(
        HKEY_LOCAL_MACHINE,
        &[
            r"SOFTWARE\GOG.com\Games",
            r"SOFTWARE\WOW6432Node\GOG.com\Games",
        ],
        &["path", "Path"],
    ));

    game_installs.extend(discover_registry_paths(
        HKEY_CURRENT_USER,
        &[r"Software\GOG.com\Games"],
        &["path", "Path"],
    ));

    let mut library_roots = Vec::new();

    push_env_join(
        &mut library_roots,
        "ProgramFiles(x86)",
        &["GOG Galaxy", "Games"],
    );

    library_roots.push(PathBuf::from(r"C:\Program Files (x86)\GOG Galaxy\Games"));

    DiscoveredSources {
        game_installs,
        library_roots,
        ..DiscoveredSources::default()
    }
}

fn discover_ea_libraries() -> DiscoveredSources {
    let game_installs = discover_registry_paths(
        HKEY_LOCAL_MACHINE,
        &[
            r"SOFTWARE\Electronic Arts\EA Games",
            r"SOFTWARE\WOW6432Node\Electronic Arts\EA Games",
            r"SOFTWARE\EA Games",
            r"SOFTWARE\WOW6432Node\EA Games",
            r"SOFTWARE\Origin Games",
            r"SOFTWARE\WOW6432Node\Origin Games",
        ],
        &["Install Dir", "InstallDir", "InstallPath", "Path"],
    );

    let mut library_roots = Vec::new();

    push_env_join(&mut library_roots, "ProgramFiles", &["EA Games"]);
    push_env_join(&mut library_roots, "ProgramFiles(x86)", &["Origin Games"]);

    library_roots.push(PathBuf::from(r"C:\Program Files\EA Games"));
    library_roots.push(PathBuf::from(r"C:\Program Files (x86)\Origin Games"));

    DiscoveredSources {
        game_installs,
        library_roots,
        ..DiscoveredSources::default()
    }
}

fn discover_ubisoft_libraries() -> DiscoveredSources {
    let game_installs = discover_registry_paths(
        HKEY_LOCAL_MACHINE,
        &[
            r"SOFTWARE\Ubisoft\Launcher\Installs",
            r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs",
        ],
        &["InstallDir", "Install Dir", "InstallPath", "Path"],
    );

    let mut library_roots = Vec::new();

    push_env_join(
        &mut library_roots,
        "ProgramFiles(x86)",
        &["Ubisoft", "Ubisoft Game Launcher", "games"],
    );

    library_roots.push(PathBuf::from(
        r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games",
    ));

    DiscoveredSources {
        game_installs,
        library_roots,
        ..DiscoveredSources::default()
    }
}

fn discover_registry_paths(
    hive: HKEY,
    base_key_paths: &[&str],
    value_names: &[&str],
) -> Vec<PathBuf> {
    let mut paths = read_registry_values(hive, base_key_paths, value_names);

    for base_key_path in base_key_paths {
        for base_key in open_registry_keys(hive, base_key_path) {
            for subkey_name in base_key.enum_keys().filter_map(Result::ok) {
                let Ok(subkey) = base_key.open_subkey_with_flags(subkey_name, KEY_READ) else {
                    continue;
                };

                paths.extend(read_path_values(&subkey, value_names));
            }
        }
    }

    paths
}

fn read_registry_values(hive: HKEY, key_paths: &[&str], value_names: &[&str]) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for key_path in key_paths {
        for key in open_registry_keys(hive, key_path) {
            paths.extend(read_path_values(&key, value_names));
        }
    }

    paths
}

fn open_registry_keys(hive: HKEY, key_path: &str) -> Vec<RegKey> {
    let root = RegKey::predef(hive);

    REGISTRY_READ_FLAGS
        .iter()
        .filter_map(|flags| root.open_subkey_with_flags(key_path, *flags).ok())
        .collect()
}

fn read_path_values(key: &RegKey, value_names: &[&str]) -> Vec<PathBuf> {
    value_names
        .iter()
        .filter_map(|value_name| key.get_value::<String, _>(*value_name).ok())
        .filter_map(|value| path_from_string(&value))
        .collect()
}

fn parse_steam_library_roots(content: &str) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for line in content.lines().map(str::trim) {
        let tokens = quoted_tokens(line);

        if tokens.len() < 2 {
            continue;
        }

        let key = &tokens[0];
        let value = &tokens[1];

        // Current Steam format:
        // "path" "D:\\SteamLibrary"
        if key.eq_ignore_ascii_case("path") {
            if let Some(path) = path_from_string(value) {
                roots.push(path);
            }

            continue;
        }

        // Legacy Steam format:
        // "1" "D:\\SteamLibrary"
        if key.parse::<u32>().is_ok() && looks_like_windows_path(value) {
            if let Some(path) = path_from_string(value) {
                roots.push(path);
            }
        }
    }

    roots
}

fn quoted_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();

    let mut in_quotes = false;
    let mut escaped = false;

    for ch in line.chars() {
        if !in_quotes {
            if ch == '"' {
                in_quotes = true;
                token.clear();
                escaped = false;
            }

            continue;
        }

        if escaped {
            match ch {
                '\\' => token.push('\\'),
                '"' => token.push('"'),
                'n' => token.push('\n'),
                't' => token.push('\t'),
                other => {
                    token.push('\\');
                    token.push(other);
                }
            }

            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => {
                tokens.push(std::mem::take(&mut token));
                in_quotes = false;
            }
            other => token.push(other),
        }
    }

    tokens
}

fn existing_unique_dirs(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();

    for path in paths {
        let Some(path) = normalize_existing_dir(&path) else {
            continue;
        };

        let key = comparable_path_key(&path);

        if seen.insert(key) {
            unique.push(path);
        }
    }

    unique
}

fn normalize_existing_dir(path: &Path) -> Option<PathBuf> {
    let metadata = fs::metadata(path).ok()?;

    if !metadata.is_dir() {
        return None;
    }

    let normalized = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    Some(strip_verbatim_prefix(normalized))
}

fn comparable_path_key(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('/', "\\");

    while value.ends_with('\\') {
        value.pop();
    }

    value.to_ascii_lowercase()
}

fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let replacement = {
        let value = path.to_string_lossy();

        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            Some(PathBuf::from(format!(r"\\{rest}")))
        } else {
            value.strip_prefix(r"\\?\").map(PathBuf::from)
        }
    };

    replacement.unwrap_or(path)
}

fn path_from_string(value: &str) -> Option<PathBuf> {
    let value = value.trim_matches('\0').trim().trim_matches('"').trim();

    if value.is_empty() {
        return None;
    }

    Some(PathBuf::from(expand_percent_env_vars(value)))
}

fn expand_percent_env_vars(value: &str) -> String {
    let mut result = String::new();
    let mut rest = value;

    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);

        let after_start = &rest[start + 1..];

        let Some(end) = after_start.find('%') else {
            result.push_str(&rest[start..]);
            return result;
        };

        let name = &after_start[..end];

        if name.is_empty() {
            result.push_str("%%");
        } else if let Some(env_value) = env::var_os(name) {
            result.push_str(&env_value.to_string_lossy());
        } else {
            result.push('%');
            result.push_str(name);
            result.push('%');
        }

        rest = &after_start[end + 1..];
    }

    result.push_str(rest);
    result
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).map(PathBuf::from)
}

fn push_env_join(paths: &mut Vec<PathBuf>, env_name: &str, components: &[&str]) {
    let Some(mut path) = env_path(env_name) else {
        return;
    };

    for component in components {
        path.push(component);
    }

    paths.push(path);
}

fn has_extension_ignore_ascii_case(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case(expected))
}

fn looks_like_windows_path(value: &str) -> bool {
    let bytes = value.as_bytes();

    let drive_absolute = bytes.len() >= 3 && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');

    drive_absolute || value.starts_with(r"\\")
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn enumerate_library_root_children_lists_immediate_subdirectories() {
        let root = temp_dir("library-root-children");
        fs::create_dir_all(root.join("GameA")).expect("GameA dir");
        fs::create_dir_all(root.join("GameB")).expect("GameB dir");
        fs::write(root.join("readme.txt"), b"not a game").expect("non-dir entry");

        let children = enumerate_library_root_children(std::slice::from_ref(&root));

        let mut sorted: Vec<String> = children
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        sorted.sort();

        assert_eq!(sorted, vec!["GameA".to_owned(), "GameB".to_owned()]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn enumerate_library_root_children_skips_missing_roots() {
        let root = temp_dir("library-root-missing");
        // Do NOT create the directory.

        let children = enumerate_library_root_children(&[root]);

        assert!(
            children.is_empty(),
            "missing root should produce no children"
        );
    }

    #[test]
    fn finalize_returns_per_game_paths_and_library_root_children() {
        let root = temp_dir("discovered-sources-merge");
        let library_root = root.join("LauncherLibrary");
        let registry_game = root.join("RegistryGame");
        let library_game_a = library_root.join("LibraryGameA");
        let library_game_b = library_root.join("LibraryGameB");

        fs::create_dir_all(&registry_game).expect("registry game dir");
        fs::create_dir_all(&library_game_a).expect("library game A dir");
        fs::create_dir_all(&library_game_b).expect("library game B dir");

        let sources = DiscoveredSources {
            game_installs: vec![registry_game.clone()],
            library_roots: vec![library_root.clone()],
            ..DiscoveredSources::default()
        };

        let finalized = sources.finalize();
        let mut keys: Vec<String> = finalized
            .install_paths
            .iter()
            .map(|p| comparable_path_key(p))
            .collect();
        keys.sort();

        let mut expected = vec![
            comparable_path_key(&registry_game),
            comparable_path_key(&library_game_a),
            comparable_path_key(&library_game_b),
        ];
        expected.sort();

        assert_eq!(keys, expected);
        assert!(
            !keys
                .iter()
                .any(|key| key == &comparable_path_key(&library_root)),
            "library root itself must not be returned as a game install path"
        );

        assert_eq!(finalized.library_roots.len(), 1);
        assert_eq!(
            comparable_path_key(&finalized.library_roots[0]),
            comparable_path_key(&library_root),
            "library_roots should be retained for downstream catalog cleanup",
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finalize_keeps_only_steam_children_with_matching_appmanifest() {
        let root = temp_dir("steam-common-filter");
        let steamapps = root.join("steamapps");
        let common = steamapps.join("common");

        let real_game = common.join("RealGame");
        let runtime_a = common.join("Steam Controller Configs");
        let runtime_b = common.join("Steamworks Common Redistributables");
        let runtime_c = common.join("Steamworks Shared");

        fs::create_dir_all(&real_game).expect("real game dir");
        fs::create_dir_all(&runtime_a).expect("runtime A dir");
        fs::create_dir_all(&runtime_b).expect("runtime B dir");
        fs::create_dir_all(&runtime_c).expect("runtime C dir");

        fs::write(
            steamapps.join("appmanifest_555.acf"),
            r#""AppState"
{
    "appid" "555"
    "installdir" "RealGame"
    "name" "Real Game"
}
"#,
        )
        .expect("appmanifest");

        let sources = DiscoveredSources {
            steam_common_roots: vec![common.clone()],
            ..DiscoveredSources::default()
        };

        let finalized = sources.finalize();
        let install_keys: Vec<String> = finalized
            .install_paths
            .iter()
            .map(|p| comparable_path_key(p))
            .collect();

        assert_eq!(
            install_keys,
            vec![comparable_path_key(&real_game)],
            "only manifest-backed children should be returned, runtime sub-folders dropped",
        );

        let library_keys: Vec<String> = finalized
            .library_roots
            .iter()
            .map(|p| comparable_path_key(p))
            .collect();
        assert_eq!(
            library_keys,
            vec![comparable_path_key(&common)],
            "steam common root must still surface in library_roots for catalog cleanup",
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn enumerate_steam_common_root_children_returns_empty_when_no_manifests() {
        let root = temp_dir("steam-common-no-manifests");
        let steamapps = root.join("steamapps");
        let common = steamapps.join("common");
        fs::create_dir_all(common.join("OrphanFolder")).expect("orphan dir");

        let children = enumerate_steam_common_root_children(&[common]);

        assert!(
            children.is_empty(),
            "without appmanifests, no children should be considered games",
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finalize_deduplicates_overlap_between_registry_and_library_root() {
        let root = temp_dir("discovered-sources-dedup");
        let library_root = root.join("LauncherLibrary");
        let game = library_root.join("SharedGame");

        fs::create_dir_all(&game).expect("game dir");

        let sources = DiscoveredSources {
            game_installs: vec![game.clone()],
            library_roots: vec![library_root.clone()],
            ..DiscoveredSources::default()
        };

        let finalized = sources.finalize();

        assert_eq!(
            finalized.install_paths.len(),
            1,
            "duplicate per-game and library-root child should appear once",
        );
        assert_eq!(
            comparable_path_key(&finalized.install_paths[0]),
            comparable_path_key(&game),
        );

        let _ = fs::remove_dir_all(root);
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();

        env::temp_dir().join(format!("renderpilot-game-libs-{label}-{nanos}"))
    }
}
