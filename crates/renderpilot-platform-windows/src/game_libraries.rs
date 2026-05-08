//! Registry and manifest based discovery of game installation/library roots.
//!
//! Discovers directories for common Windows game launchers:
//! Steam, Epic Games Store, GOG Galaxy, EA App / Origin, and Ubisoft Connect.

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

const REGISTRY_READ_FLAGS: &[u32] = &[
    KEY_READ,
    KEY_READ | KEY_WOW64_64KEY,
    KEY_READ | KEY_WOW64_32KEY,
];

/// Discovers known game installation/library directories.
///
/// Checks Steam, Epic Games Store, GOG Galaxy, EA App / Origin, and Ubisoft
/// Connect locations.
///
/// Only existing directories are returned. Invalid registry entries, missing
/// files, malformed launcher manifests, and inaccessible paths are ignored.
pub fn discover_game_library_roots() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    candidates.extend(discover_steam_libraries());
    candidates.extend(discover_epic_games_libraries());
    candidates.extend(discover_gog_libraries());
    candidates.extend(discover_ea_libraries());
    candidates.extend(discover_ubisoft_libraries());

    existing_unique_dirs(candidates)
}

fn discover_steam_libraries() -> Vec<PathBuf> {
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

    let mut libraries = Vec::new();

    for steam_root in steam_roots {
        libraries.push(steam_root.join("steamapps").join("common"));

        let libraryfolders_vdf = steam_root.join("steamapps").join("libraryfolders.vdf");

        let Ok(content) = fs::read_to_string(libraryfolders_vdf) else {
            continue;
        };

        for library_root in parse_steam_library_roots(&content) {
            libraries.push(library_root.join("steamapps").join("common"));
        }
    }

    libraries
}

fn discover_epic_games_libraries() -> Vec<PathBuf> {
    #[derive(Debug, Deserialize)]
    struct EpicManifest {
        #[serde(rename = "InstallLocation")]
        install_location: Option<String>,
    }

    let mut libraries = Vec::new();

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
                libraries.push(path);
            }
        }
    }

    push_env_join(&mut libraries, "ProgramFiles", &["Epic Games"]);
    libraries.push(PathBuf::from(r"C:\Program Files\Epic Games"));

    libraries
}

fn discover_gog_libraries() -> Vec<PathBuf> {
    let mut libraries = Vec::new();

    libraries.extend(discover_registry_paths(
        HKEY_LOCAL_MACHINE,
        &[
            r"SOFTWARE\GOG.com\Games",
            r"SOFTWARE\WOW6432Node\GOG.com\Games",
        ],
        &["path", "Path"],
    ));

    libraries.extend(discover_registry_paths(
        HKEY_CURRENT_USER,
        &[r"Software\GOG.com\Games"],
        &["path", "Path"],
    ));

    push_env_join(
        &mut libraries,
        "ProgramFiles(x86)",
        &["GOG Galaxy", "Games"],
    );

    libraries.push(PathBuf::from(r"C:\Program Files (x86)\GOG Galaxy\Games"));

    libraries
}

fn discover_ea_libraries() -> Vec<PathBuf> {
    let mut libraries = discover_registry_paths(
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

    push_env_join(&mut libraries, "ProgramFiles", &["EA Games"]);
    push_env_join(&mut libraries, "ProgramFiles(x86)", &["Origin Games"]);

    libraries.push(PathBuf::from(r"C:\Program Files\EA Games"));
    libraries.push(PathBuf::from(r"C:\Program Files (x86)\Origin Games"));

    libraries
}

fn discover_ubisoft_libraries() -> Vec<PathBuf> {
    let mut libraries = discover_registry_paths(
        HKEY_LOCAL_MACHINE,
        &[
            r"SOFTWARE\Ubisoft\Launcher\Installs",
            r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs",
        ],
        &["InstallDir", "Install Dir", "InstallPath", "Path"],
    );

    push_env_join(
        &mut libraries,
        "ProgramFiles(x86)",
        &["Ubisoft", "Ubisoft Game Launcher", "games"],
    );

    libraries.push(PathBuf::from(
        r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games",
    ));

    libraries
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
