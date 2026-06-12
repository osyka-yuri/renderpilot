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
//!
//! Code is split by concern: `launchers` holds the per-launcher knowledge of
//! where each store records its games, `registry` the Windows registry access,
//! and `paths` the path normalization / dedup / Steam-VDF parsing helpers. This
//! module owns the public API and the cross-launcher aggregation.

mod launchers;
mod paths;
mod registry;

use std::{fs, path::PathBuf};

use crate::steam_appmanifest::steam_install_dirs_in_steamapps;

use self::launchers::{
    discover_ea_libraries, discover_epic_games_libraries, discover_gog_libraries,
    discover_steam_libraries, discover_ubisoft_libraries,
};
use self::paths::existing_unique_dirs;

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

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::paths::{comparable_path_key, strip_verbatim_prefix};
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
            library_roots: vec![library_root],
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

        // Replicate the canonical (long-name) path format produced by `finalize`
        // via `normalize_existing_dir`. On certain Windows environments (such as CI runners),
        // `env::temp_dir()` may return an 8.3 short path (e.g., `RUNNER~1`).
        // This short path would mismatch the long format (`runneradmin`) returned by `fs::canonicalize`.
        // By canonicalizing only the base path, callers relying on the joined directory's non-existence
        // will still correctly observe a missing path.
        let base = fs::canonicalize(env::temp_dir())
            .map(strip_verbatim_prefix)
            .unwrap_or_else(|_| env::temp_dir());

        base.join(format!("renderpilot-game-libs-{label}-{nanos}"))
    }
}
