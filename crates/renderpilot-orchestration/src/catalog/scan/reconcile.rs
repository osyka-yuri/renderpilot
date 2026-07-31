//! Catalog identity merge for a freshly discovered install.

use std::collections::HashMap;

use renderpilot_application::{AppResult, ArtifactRepository};
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentId, GameId, GameIdentity, GameInstallation,
    InstallKey, Launcher, LibraryArtifact, LibraryComponent, RootAuthority,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

use crate::catalog::install_paths;

/// Snapshot of catalog installs keyed by [`install_paths::install_path_match_key`].
///
/// Loaded once per scan so multi-root parent scans do not call `list_games`
/// once per derived install.
pub(crate) struct CatalogInstallIndex {
    by_install_path: HashMap<InstallKey, GameInstallation>,
    by_game_id: HashMap<GameId, GameInstallation>,
    components_by_game: HashMap<GameId, HashMap<ComponentId, LibraryComponent>>,
    local_artifacts_by_game: HashMap<GameId, HashMap<ArtifactId, LibraryArtifact>>,
}

impl CatalogInstallIndex {
    pub(crate) fn load(storage: &SqliteStorage) -> Result<Self, ServiceError> {
        let games = storage.list_games()?;
        let mut by_install_path = HashMap::with_capacity(games.len());
        let mut by_game_id = HashMap::with_capacity(games.len());

        for game in games {
            let key = game.install_key().clone();
            by_install_path.entry(key).or_insert_with(|| game.clone());
            by_game_id.insert(game.id().clone(), game);
        }

        let mut components_by_game =
            HashMap::<GameId, HashMap<ComponentId, LibraryComponent>>::new();
        for component in storage.list_all_components()? {
            components_by_game
                .entry(component.game_id().clone())
                .or_default()
                .insert(component.id().clone(), component);
        }

        let mut local_artifacts_by_game =
            HashMap::<GameId, HashMap<ArtifactId, LibraryArtifact>>::new();
        for artifact in storage.list_artifacts()? {
            if artifact.trust_level() != ArtifactTrustLevel::LocalObserved {
                continue;
            }
            if let Some(game_id) = artifact.source_game_id() {
                local_artifacts_by_game
                    .entry(game_id.clone())
                    .or_default()
                    .insert(artifact.id().clone(), artifact);
            }
        }

        Ok(Self {
            by_install_path,
            by_game_id,
            components_by_game,
            local_artifacts_by_game,
        })
    }

    pub(super) fn game(&self, game_id: &GameId) -> Option<&GameInstallation> {
        self.by_game_id.get(game_id)
    }

    pub(super) fn components(
        &self,
        game_id: &GameId,
    ) -> Option<&HashMap<ComponentId, LibraryComponent>> {
        self.components_by_game.get(game_id)
    }

    fn find_by_install_path(&self, install_path: &str) -> Option<&GameInstallation> {
        let key = install_paths::install_path_match_key(install_path)?;
        self.by_install_path.get(&key)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn contains_install_path(&self, install_path: &std::path::Path) -> bool {
        self.contains_install_path_str(&install_path.to_string_lossy())
    }

    #[cfg(windows)]
    pub(crate) fn game_id_for_install_path(
        &self,
        install_path: &std::path::Path,
    ) -> Option<&GameId> {
        self.find_by_install_path(&install_path.to_string_lossy())
            .map(GameInstallation::id)
    }

    pub(super) fn contains_install_path_str(&self, install_path: &str) -> bool {
        self.find_by_install_path(install_path).is_some()
    }

    pub(super) fn card_facts_changed(
        &self,
        game: &GameInstallation,
        components: &[LibraryComponent],
        artifacts: &[LibraryArtifact],
    ) -> bool {
        self.find_by_install_path(game.install_path().as_str()) != Some(game)
            || !self.components_match(game.id(), components)
            || !self.local_artifacts_match(game.id(), artifacts)
    }

    fn components_match(&self, game_id: &GameId, components: &[LibraryComponent]) -> bool {
        let existing = self.components_by_game.get(game_id);
        let existing_len = existing.map_or(0, HashMap::len);
        existing_len == components.len()
            && components.iter().all(|component| {
                existing.and_then(|items| items.get(component.id())) == Some(component)
            })
    }

    fn local_artifacts_match(&self, game_id: &GameId, artifacts: &[LibraryArtifact]) -> bool {
        let existing = self.local_artifacts_by_game.get(game_id);
        let existing_len = existing.map_or(0, HashMap::len);
        existing_len == artifacts.len()
            && artifacts.iter().all(|artifact| {
                existing.and_then(|items| items.get(artifact.id())) == Some(artifact)
            })
    }
}

/// Reuses an existing catalog row when the install path already matches one.
///
/// This keeps stable game ids (covers, add-ons, operations) and prevents a
/// folder re-scan from demoting a Steam/GOG/Epic game to `Manual` when
/// launcher metadata is temporarily unavailable.
pub(super) fn reconcile_game_with_catalog(
    catalog_index: &CatalogInstallIndex,
    discovered: GameInstallation,
) -> GameInstallation {
    match catalog_index.find_by_install_path(discovered.install_path().as_str()) {
        Some(existing) => merge_scan_game_with_existing(existing, &discovered),
        None => discovered,
    }
}

/// Merges a freshly discovered install with a catalog row for the same path.
///
/// Policy:
/// - Always keep the existing `GameId` (foreign keys stay valid).
/// - Prefer a fresh non-`Manual` launcher; fill missing external_id from existing.
/// - Never demote an existing non-`Manual` row to `Manual` when metadata vanishes.
/// - Platform, runtime, install path, and executable candidates come from discovery.
pub(super) fn merge_scan_game_with_existing(
    existing: &GameInstallation,
    discovered: &GameInstallation,
) -> GameInstallation {
    let discovered_launcher = discovered.identity().launcher();
    let existing_launcher = existing.identity().launcher();

    let (launcher, external_id, title) = if discovered_launcher != Launcher::Manual {
        (
            discovered_launcher,
            discovered
                .identity()
                .external_id()
                .map(str::to_owned)
                .or_else(|| existing.identity().external_id().map(str::to_owned)),
            discovered.identity().title().to_owned(),
        )
    } else if existing_launcher != Launcher::Manual {
        (
            existing_launcher,
            existing.identity().external_id().map(str::to_owned),
            existing.identity().title().to_owned(),
        )
    } else {
        (
            Launcher::Manual,
            None,
            discovered.identity().title().to_owned(),
        )
    };

    let identity =
        build_reconciled_identity(existing.id(), &title, launcher, external_id.as_deref())
            .unwrap_or_else(|| existing.identity().clone());

    let root_authority =
        reconcile_root_authority(existing.root_authority(), discovered.root_authority());
    let mut game = GameInstallation::new(
        identity,
        discovered.platform(),
        discovered.runtime(),
        discovered.install_path().clone(),
    )
    .with_root_authority(root_authority);

    for candidate in discovered.executable_candidates() {
        game = game.with_executable_candidate(candidate.clone());
    }
    if let Some(confirmed) = discovered.confirmed_executable() {
        game = game.with_confirmed_executable(confirmed.clone());
    } else if existing.install_key() == discovered.install_key()
        && let Some(confirmed) = existing.confirmed_executable()
    {
        game = game.with_confirmed_executable(confirmed.clone());
    }

    game
}

fn reconcile_root_authority(existing: RootAuthority, discovered: RootAuthority) -> RootAuthority {
    match (existing, discovered) {
        (RootAuthority::UserConfirmed, _) | (_, RootAuthority::UserConfirmed) => {
            RootAuthority::UserConfirmed
        }
        (RootAuthority::LauncherManifest, _) | (_, RootAuthority::LauncherManifest) => {
            RootAuthority::LauncherManifest
        }
        (RootAuthority::Legacy, RootAuthority::Legacy) => RootAuthority::Legacy,
    }
}

fn build_reconciled_identity(
    id: &GameId,
    title: &str,
    launcher: Launcher,
    external_id: Option<&str>,
) -> Option<GameIdentity> {
    let identity = GameIdentity::new(id.clone(), title, launcher).ok()?;
    match external_id {
        Some(external_id) => identity.with_external_id(external_id).ok(),
        None => Some(identity),
    }
}

pub(super) fn build_library_components(
    game: &GameInstallation,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<LibraryComponent>> {
    // Components and artifacts share the same `(directory, family)` grouping,
    // so every detected bundle has one coherent component and artifact shape.
    renderpilot_detection::group_into_components(game, libraries)
}

pub(super) fn build_library_artifacts(
    game_id: &GameId,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<LibraryArtifact>> {
    renderpilot_detection::group_into_artifacts(game_id, libraries)
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{ComponentRepository, GameRepository};
    use renderpilot_domain::{
        ComponentId, ComponentKind, GameId, GameIdentity, GameInstallation, GameRuntime, Launcher,
        LibraryComponent, LibraryTechnology, PathRef, Platform, RootAuthority, Swappability,
    };
    use renderpilot_storage_sqlite::SqliteStorage;

    use super::{CatalogInstallIndex, merge_scan_game_with_existing};

    #[test]
    fn catalog_index_distinguishes_noop_scan_from_changed_game_facts() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let existing = sample_install(
            "manual:C:/Games/Stable",
            "Stable",
            Launcher::Manual,
            None,
            "C:/Games/Stable",
        );
        storage.upsert_game(&existing).expect("seed game");
        let index = CatalogInstallIndex::load(&storage).expect("catalog index");

        assert!(index.contains_install_path(std::path::Path::new("C:/Games/Stable")));
        assert!(!index.contains_install_path(std::path::Path::new("C:/Games/Missing")));
        assert!(!index.card_facts_changed(&existing, &[], &[]));

        let renamed = sample_install(
            "manual:C:/Games/Stable",
            "Renamed",
            Launcher::Manual,
            None,
            "C:/Games/Stable",
        );
        assert!(index.card_facts_changed(&renamed, &[], &[]));
    }

    #[test]
    fn catalog_index_compares_component_sets_independent_of_query_order() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game = sample_install(
            "manual:C:/Games/Stable",
            "Stable",
            Launcher::Manual,
            None,
            "C:/Games/Stable",
        );
        let first = sample_component(
            &game,
            "component:first",
            LibraryTechnology::DlssSuperResolution,
        );
        let second = sample_component(
            &game,
            "component:second",
            LibraryTechnology::NvidiaStreamline,
        );
        storage.upsert_game(&game).expect("seed game");
        storage
            .replace_components_for_game(game.id(), &[first.clone(), second.clone()])
            .expect("seed components");

        let index = CatalogInstallIndex::load(&storage).expect("catalog index");

        assert!(!index.card_facts_changed(&game, &[second, first], &[]));
    }

    #[test]
    fn merge_preserves_existing_non_manual_launcher_when_rescan_is_manual() {
        let existing = sample_install(
            "manual:D:/Games/SteamGame",
            "Cyberpunk 2077",
            Launcher::Steam,
            Some("1091500"),
            "D:/Games/SteamGame",
        );
        let discovered = sample_install(
            "manual:D:/Games/SteamGame",
            "SteamGame",
            Launcher::Manual,
            None,
            "D:/Games/SteamGame",
        );

        let merged = merge_scan_game_with_existing(&existing, &discovered);

        assert_eq!(merged.id().as_str(), "manual:D:/Games/SteamGame");
        assert_eq!(merged.identity().launcher(), Launcher::Steam);
        assert_eq!(merged.identity().external_id(), Some("1091500"));
        assert_eq!(merged.identity().title(), "Cyberpunk 2077");
    }

    #[test]
    fn merge_prefers_fresh_launcher_detection_over_manual_catalog_row() {
        let existing = sample_install(
            "manual:D:/Games/GogGame",
            "GogGame",
            Launcher::Manual,
            None,
            "D:/Games/GogGame",
        );
        let discovered = sample_install(
            "manual:D:/Games/GogGame",
            "The Witcher 3",
            Launcher::Gog,
            Some("1207659999"),
            "D:/Games/GogGame",
        )
        .with_root_authority(RootAuthority::LauncherManifest);

        let merged = merge_scan_game_with_existing(&existing, &discovered);

        assert_eq!(merged.identity().launcher(), Launcher::Gog);
        assert_eq!(
            merged.id(),
            existing.id(),
            "launcher promotion keeps GameId"
        );
        assert_eq!(merged.root_authority(), RootAuthority::LauncherManifest);
        assert_eq!(merged.identity().external_id(), Some("1207659999"));
        assert_eq!(merged.identity().title(), "The Witcher 3");
    }

    #[test]
    fn launcher_refresh_preserves_user_confirmed_root_authority() {
        let existing = sample_install(
            "game:stable",
            "Black Flag",
            Launcher::Manual,
            None,
            "C:/Games/Black Flag",
        )
        .with_root_authority(RootAuthority::UserConfirmed);
        let discovered = sample_install(
            "game:provisional",
            "Assassin's Creed IV Black Flag",
            Launcher::Ubisoft,
            Some("273"),
            "C:/Games/Black Flag",
        )
        .with_root_authority(RootAuthority::LauncherManifest);

        let merged = merge_scan_game_with_existing(&existing, &discovered);

        assert_eq!(merged.id(), existing.id());
        assert_eq!(merged.identity().launcher(), Launcher::Ubisoft);
        assert_eq!(merged.identity().external_id(), Some("273"));
        assert_eq!(merged.root_authority(), RootAuthority::UserConfirmed);
    }

    #[test]
    fn generic_merge_only_carries_confirmed_executable_for_the_same_install_key() {
        let confirmed = PathRef::new("CustomLauncher.exe").expect("executable");
        let existing = sample_install(
            "game:stable",
            "Black Flag",
            Launcher::Manual,
            None,
            "C:/Games/Black Flag",
        )
        .with_confirmed_executable(confirmed.clone());
        let exact_refresh = sample_install(
            "game:provisional",
            "Black Flag",
            Launcher::Manual,
            None,
            "C:/Games/Black Flag",
        );
        let root_correction = sample_install(
            "game:provisional",
            "Black Flag",
            Launcher::Manual,
            None,
            "C:/Games",
        );

        let refreshed = merge_scan_game_with_existing(&existing, &exact_refresh);
        let corrected = merge_scan_game_with_existing(&existing, &root_correction);

        assert_eq!(refreshed.confirmed_executable(), Some(&confirmed));
        assert_eq!(refreshed.executable_candidates(), &[confirmed]);
        assert_eq!(corrected.confirmed_executable(), None);
        assert!(corrected.executable_candidates().is_empty());
    }

    #[test]
    fn merge_keeps_existing_id_when_discovered_id_string_differs() {
        let existing = sample_install(
            "manual:d:/games/steamgame",
            "Cyberpunk 2077",
            Launcher::Steam,
            Some("1091500"),
            "d:/games/steamgame",
        );
        let discovered = sample_install(
            "manual:D:/Games/SteamGame",
            "Cyberpunk 2077",
            Launcher::Steam,
            Some("1091500"),
            "D:/Games/SteamGame",
        );

        let merged = merge_scan_game_with_existing(&existing, &discovered);

        assert_eq!(merged.id().as_str(), "manual:d:/games/steamgame");
        assert_eq!(merged.install_path().as_str(), "D:/Games/SteamGame");
        assert_eq!(merged.identity().launcher(), Launcher::Steam);
        assert_eq!(merged.identity().external_id(), Some("1091500"));
    }

    #[test]
    fn merge_prefers_discovered_launcher_when_switching_stores() {
        let existing = sample_install(
            "manual:D:/Games/Shared",
            "Shared Title",
            Launcher::Steam,
            Some("1"),
            "D:/Games/Shared",
        );
        let discovered = sample_install(
            "manual:D:/Games/Shared",
            "Shared Title GOG",
            Launcher::Gog,
            Some("99"),
            "D:/Games/Shared",
        );

        let merged = merge_scan_game_with_existing(&existing, &discovered);

        assert_eq!(merged.id().as_str(), "manual:D:/Games/Shared");
        assert_eq!(merged.identity().launcher(), Launcher::Gog);
        assert_eq!(merged.identity().external_id(), Some("99"));
        assert_eq!(merged.identity().title(), "Shared Title GOG");
    }

    #[test]
    fn merge_both_manual_keeps_existing_id_and_discovered_title() {
        let existing = sample_install(
            "manual:D:/Games/OldName",
            "Old Title",
            Launcher::Manual,
            None,
            "D:/Games/OldName",
        );
        let discovered = sample_install(
            "manual:D:/Games/NewName",
            "New Title",
            Launcher::Manual,
            None,
            "D:/Games/NewName",
        );

        let merged = merge_scan_game_with_existing(&existing, &discovered);

        assert_eq!(merged.id().as_str(), "manual:D:/Games/OldName");
        assert_eq!(merged.identity().launcher(), Launcher::Manual);
        assert_eq!(merged.identity().external_id(), None);
        assert_eq!(merged.identity().title(), "New Title");
        assert_eq!(merged.install_path().as_str(), "D:/Games/NewName");
    }

    fn sample_install(
        id: &str,
        title: &str,
        launcher: Launcher,
        external_id: Option<&str>,
        install_path: &str,
    ) -> GameInstallation {
        let mut identity =
            GameIdentity::new(GameId::new(id).expect("id"), title, launcher).expect("identity");

        if let Some(external_id) = external_id {
            identity = identity.with_external_id(external_id).expect("external id");
        }

        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(install_path).expect("path"),
        )
    }

    fn sample_component(
        game: &GameInstallation,
        id: &str,
        technology: LibraryTechnology,
    ) -> LibraryComponent {
        LibraryComponent::new(
            ComponentId::new(id).expect("component id"),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            technology,
            Swappability::Swappable,
        )
    }
}
