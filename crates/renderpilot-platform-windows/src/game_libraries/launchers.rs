//! Per-launcher discovery: where each supported Windows launcher records its
//! installed games (Steam, Epic, GOG, EA App / Origin, Ubisoft Connect).
//!
//! Each function returns a [`DiscoveredSources`] describing that launcher's
//! per-game install paths and/or library container roots; the aggregation and
//! child enumeration happen in the parent module.

use std::{fs, path::PathBuf};

use serde::Deserialize;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use super::paths::{
    env_path, has_extension_ignore_ascii_case, parse_steam_library_roots, path_from_string,
    push_env_join,
};
use super::registry::{discover_registry_paths, read_registry_values};
use super::DiscoveredSources;

pub(super) fn discover_steam_libraries() -> DiscoveredSources {
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

pub(super) fn discover_epic_games_libraries() -> DiscoveredSources {
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

pub(super) fn discover_gog_libraries() -> DiscoveredSources {
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

pub(super) fn discover_ea_libraries() -> DiscoveredSources {
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

pub(super) fn discover_ubisoft_libraries() -> DiscoveredSources {
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
