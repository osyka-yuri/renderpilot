//! Registry and manifest based discovery of game install folders.
//!
//! Discovers per-game install directories for common Windows game launchers:
//! Steam, Epic Games Store, GOG Galaxy, EA App / Origin, and Ubisoft Connect.
//!
//! Internally, sources are split into two kinds:
//!
//! * Per-game install paths come straight from per-app registry keys or per-app
//!   manifest files. Each path already points at one game install folder.
//! * Launcher library roots are retained only as cleanup scope metadata.
//!   Children are never guessed to be games. Steam roots are resolved through
//!   `appmanifest_*.acf`; other launchers contribute only per-game records.
//!
//! This separation prevents launcher container folders themselves from being
//! treated as a single "game" by the catalog scan.
//!
//! Code is split by concern: `launchers` holds the per-launcher knowledge of
//! where each store records its games, `registry` the Windows registry access,
//! and `paths` the path normalization / dedup / Steam-VDF parsing helpers. This
//! module owns the public API and the cross-launcher aggregation.

mod launch_exe;
mod launchers;
mod paths;
mod registry;

pub use launch_exe::launcher_launch_executable;

use std::{collections::BTreeMap, fs, path::PathBuf};

use crate::install_identity::InstallIdentityDetails;
use crate::steam_appmanifest::{SteamScanSourceFingerprint, try_steam_manifest_index};
use renderpilot_domain::{InstallKey, Launcher};

use self::launchers::{
    discover_ea_libraries, discover_epic_games_libraries, discover_gog_libraries,
    discover_steam_libraries, discover_ubisoft_libraries,
};
use self::paths::{comparable_path_key, existing_unique_dirs, normalize_existing_dir};

/// Game sources discovered from common Windows launchers.
#[derive(Debug, Default, Clone)]
pub struct DiscoveredGameSources {
    /// Exact installs returned by launcher records/manifests, including the
    /// provider evidence that established each root.
    pub installs: Vec<DiscoveredInstall>,
    /// Existing launcher library roots (e.g. `steamapps/common`,
    /// `Program Files/EA Games`).
    ///
    /// Useful for catalog-side cleanup of stale entries that were created when
    /// a launcher root was previously persisted as a single game.
    pub library_roots: Vec<PathBuf>,
    /// Roots whose complete child and manifest enumeration succeeded. Only
    /// these roots are safe inputs for pruning disappeared catalog children.
    pub authoritative_library_roots: Vec<PathBuf>,
}

/// Exact installation target produced by a launcher provider.
#[derive(Debug, Clone)]
pub struct DiscoveredInstall {
    /// Confirmed game installation root.
    pub install_path: PathBuf,
    /// Launcher identity established by the provider record.
    pub identity: InstallIdentityDetails,
    /// Optional launcher checkpoint for avoiding an unchanged full scan.
    pub checkpoint: Option<SteamScanSourceFingerprint>,
}

impl DiscoveredInstall {
    pub(super) fn launcher(install_path: PathBuf, launcher: Launcher) -> Self {
        Self {
            install_path,
            identity: InstallIdentityDetails {
                launcher,
                external_id: None,
                display_name: None,
            },
            checkpoint: None,
        }
    }

    pub(super) fn with_identity(install_path: PathBuf, identity: InstallIdentityDetails) -> Self {
        Self {
            install_path,
            identity,
            checkpoint: None,
        }
    }

    fn with_checkpoint(mut self, checkpoint: SteamScanSourceFingerprint) -> Self {
        self.checkpoint = Some(checkpoint);
        self
    }

    fn evidence_rank(&self) -> (bool, bool, bool) {
        (
            self.checkpoint.is_some(),
            self.identity.external_id.is_some(),
            self.identity.display_name.is_some(),
        )
    }
}

/// Discovers game sources from common Windows launchers.
///
/// Returned installs point at existing game directories. Launcher library
/// roots are returned separately in [`DiscoveredGameSources::library_roots`]
/// and are never used as scan targets directly.
///
/// Only existing directories are returned. Invalid registry entries, missing
/// files, malformed launcher manifests, and inaccessible paths are ignored.
pub fn discover_game_sources() -> DiscoveredGameSources {
    let mut sources = DiscoveredSources::default();
    for provider in LAUNCHER_INSTALL_PROVIDERS {
        sources.merge(provider.discover());
    }

    sources.finalize()
}

/// Internal collection of discovery results, split by source kind.
#[derive(Debug, Default)]
struct DiscoveredSources {
    /// Paths that already point at one game install folder (per-app
    /// registry keys, Epic per-game manifests, etc.).
    game_installs: Vec<DiscoveredInstall>,
    /// Container folders retained as cleanup scope only. Their children are
    /// never inferred to be installations.
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

/// Provider boundary: implementations return launcher-record evidence and
/// never infer installs from directory layout.
trait LauncherInstallProvider {
    fn discover(&self) -> DiscoveredSources;
}

#[derive(Clone, Copy)]
struct FunctionLauncherInstallProvider {
    discover: fn() -> DiscoveredSources,
}

impl LauncherInstallProvider for FunctionLauncherInstallProvider {
    fn discover(&self) -> DiscoveredSources {
        (self.discover)()
    }
}

const LAUNCHER_INSTALL_PROVIDERS: [FunctionLauncherInstallProvider; 5] = [
    FunctionLauncherInstallProvider {
        discover: discover_steam_libraries,
    },
    FunctionLauncherInstallProvider {
        discover: discover_epic_games_libraries,
    },
    FunctionLauncherInstallProvider {
        discover: discover_gog_libraries,
    },
    FunctionLauncherInstallProvider {
        discover: discover_ea_libraries,
    },
    FunctionLauncherInstallProvider {
        discover: discover_ubisoft_libraries,
    },
];

impl DiscoveredSources {
    fn merge(&mut self, other: DiscoveredSources) {
        self.game_installs.extend(other.game_installs);
        self.library_roots.extend(other.library_roots);
        self.steam_common_roots.extend(other.steam_common_roots);
    }

    fn finalize(self) -> DiscoveredGameSources {
        let library_roots = existing_unique_dirs(self.library_roots.iter().cloned());
        let steam_common_roots = existing_unique_dirs(self.steam_common_roots.iter().cloned());

        let steam = enumerate_steam_common_root_children(&steam_common_roots);

        let installs =
            existing_unique_installs(self.game_installs.into_iter().chain(steam.installs));

        let all_library_roots = library_roots
            .into_iter()
            .chain(steam_common_roots)
            .collect::<Vec<_>>();

        DiscoveredGameSources {
            installs,
            library_roots: all_library_roots,
            authoritative_library_roots: steam.authoritative_roots,
        }
    }
}

fn existing_unique_installs(
    installs: impl IntoIterator<Item = DiscoveredInstall>,
) -> Vec<DiscoveredInstall> {
    let mut by_path = BTreeMap::<InstallKey, DiscoveredInstall>::new();

    for mut install in installs {
        let Some(path) = normalize_existing_dir(&install.install_path) else {
            continue;
        };
        install.install_path = path;
        let Some(key) = comparable_path_key(&install.install_path) else {
            continue;
        };
        match by_path.get(&key) {
            Some(existing) if existing.evidence_rank() >= install.evidence_rank() => {}
            _ => {
                by_path.insert(key, install);
            }
        }
    }

    by_path.into_values().collect()
}

#[derive(Default)]
struct RootEnumeration {
    installs: Vec<DiscoveredInstall>,
    authoritative_roots: Vec<PathBuf>,
}

/// Enumerates direct sub-directories of each Steam `steamapps/common` root,
/// keeping only those that match an `installdir` declared by a manifest in
/// the sibling `steamapps/` directory.
fn enumerate_steam_common_root_children(common_roots: &[PathBuf]) -> RootEnumeration {
    let mut output = RootEnumeration::default();

    for common in common_roots {
        let Some(steamapps_dir) = common.parent() else {
            continue;
        };

        let Some(manifests) = try_steam_manifest_index(steamapps_dir) else {
            continue;
        };

        let Ok(entries) = fs::read_dir(common) else {
            continue;
        };

        let mut complete = true;
        for entry in entries {
            let Ok(entry) = entry else {
                complete = false;
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                complete = false;
                continue;
            };

            if !file_type.is_dir() {
                continue;
            }

            let Some(name) = entry.file_name().to_str().map(str::to_ascii_lowercase) else {
                continue;
            };

            if let Some(manifest) = manifests.get(&name) {
                let install_path = entry.path();
                output.installs.push(
                    DiscoveredInstall::with_identity(install_path, manifest.details.clone().into())
                        .with_checkpoint(manifest.checkpoint.clone()),
                );
            }
        }
        if complete {
            output.authoritative_roots.push(common.clone());
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::paths::comparable_path_key;
    use super::*;
    use crate::path_normalize::strip_verbatim_prefix;

    #[test]
    fn unmanifested_library_children_are_never_guessed_as_installs() {
        let root = temp_dir("library-root-children");
        fs::create_dir_all(root.join("GameA")).expect("GameA dir");
        fs::create_dir_all(root.join("GameB")).expect("GameB dir");
        fs::write(root.join("readme.txt"), b"not a game").expect("non-dir entry");

        let finalized = DiscoveredSources {
            library_roots: vec![root.clone()],
            ..DiscoveredSources::default()
        }
        .finalize();
        assert!(finalized.installs.is_empty());
        assert!(finalized.authoritative_library_roots.is_empty());
        assert_eq!(finalized.library_roots, vec![root.clone()]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_library_roots_are_ignored() {
        let root = temp_dir("library-root-missing");
        // Do NOT create the directory.

        let finalized = DiscoveredSources {
            library_roots: vec![root],
            ..DiscoveredSources::default()
        }
        .finalize();
        assert!(finalized.installs.is_empty());
        assert!(finalized.library_roots.is_empty());
    }

    #[test]
    fn finalize_returns_only_provider_confirmed_game_paths() {
        let root = temp_dir("discovered-sources-merge");
        let library_root = root.join("LauncherLibrary");
        let registry_game = root.join("RegistryGame");
        let library_game_a = library_root.join("LibraryGameA");
        let library_game_b = library_root.join("LibraryGameB");

        fs::create_dir_all(&registry_game).expect("registry game dir");
        fs::create_dir_all(&library_game_a).expect("library game A dir");
        fs::create_dir_all(&library_game_b).expect("library game B dir");

        let sources = DiscoveredSources {
            game_installs: vec![DiscoveredInstall::launcher(
                registry_game.clone(),
                Launcher::Gog,
            )],
            library_roots: vec![library_root.clone()],
            ..DiscoveredSources::default()
        };

        let finalized = sources.finalize();
        let mut keys: Vec<InstallKey> = finalized
            .installs
            .iter()
            .filter_map(|install| comparable_path_key(&install.install_path))
            .collect();
        keys.sort();

        let mut expected = vec![comparable_path_key(&registry_game).expect("registry game key")];
        expected.sort();

        assert_eq!(keys, expected);
        let library_key = comparable_path_key(&library_root).expect("library key");
        assert!(
            !keys.iter().any(|key| key == &library_key),
            "library root itself must not be returned as a game install path"
        );

        assert_eq!(finalized.library_roots.len(), 1);
        assert!(finalized.authoritative_library_roots.is_empty());
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
        let install_keys: Vec<InstallKey> = finalized
            .installs
            .iter()
            .filter_map(|install| comparable_path_key(&install.install_path))
            .collect();

        assert_eq!(
            install_keys,
            vec![comparable_path_key(&real_game).expect("real game key")],
            "only manifest-backed children should be returned, runtime sub-folders dropped",
        );
        assert_eq!(finalized.installs.len(), 1);
        assert_eq!(
            comparable_path_key(&finalized.installs[0].install_path),
            comparable_path_key(&real_game),
        );
        let checkpoint = finalized.installs[0]
            .checkpoint
            .as_ref()
            .expect("Steam install checkpoint");
        assert_eq!(checkpoint.source_key, "steam:555");
        assert_eq!(
            finalized.installs[0].identity.external_id.as_deref(),
            Some("555")
        );
        assert!(!checkpoint.fingerprint.is_empty());

        let library_keys: Vec<InstallKey> = finalized
            .library_roots
            .iter()
            .filter_map(|path| comparable_path_key(path))
            .collect();
        assert_eq!(
            library_keys,
            vec![comparable_path_key(&common).expect("common root key")],
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

        let enumeration = enumerate_steam_common_root_children(&[common]);

        assert!(
            enumeration.installs.is_empty(),
            "without appmanifests, no children should be considered games",
        );
        assert_eq!(enumeration.authoritative_roots.len(), 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finalize_deduplicates_overlap_between_registry_and_library_root() {
        let root = temp_dir("discovered-sources-dedup");
        let library_root = root.join("LauncherLibrary");
        let game = library_root.join("SharedGame");

        fs::create_dir_all(&game).expect("game dir");

        let sources = DiscoveredSources {
            game_installs: vec![DiscoveredInstall::launcher(game.clone(), Launcher::Gog)],
            library_roots: vec![library_root],
            ..DiscoveredSources::default()
        };

        let finalized = sources.finalize();

        assert_eq!(
            finalized.installs.len(),
            1,
            "provider-confirmed install should appear once",
        );
        assert_eq!(
            comparable_path_key(&finalized.installs[0].install_path),
            comparable_path_key(&game),
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn deduplication_preserves_the_strongest_provider_evidence() {
        let root = temp_dir("discovered-evidence-dedup");
        fs::create_dir_all(&root).expect("install");
        let sources = DiscoveredSources {
            game_installs: vec![
                DiscoveredInstall::launcher(root.clone(), Launcher::Epic),
                DiscoveredInstall::with_identity(
                    root.clone(),
                    InstallIdentityDetails {
                        launcher: Launcher::Epic,
                        external_id: Some("app-name".to_owned()),
                        display_name: Some("Display Name".to_owned()),
                    },
                ),
            ],
            ..DiscoveredSources::default()
        };

        let finalized = sources.finalize();

        assert_eq!(finalized.installs.len(), 1);
        assert_eq!(
            finalized.installs[0].identity.external_id.as_deref(),
            Some("app-name")
        );
        assert_eq!(
            finalized.installs[0].identity.display_name.as_deref(),
            Some("Display Name")
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
