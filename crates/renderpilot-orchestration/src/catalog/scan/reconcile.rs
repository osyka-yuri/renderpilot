//! Catalog identity merge for a freshly discovered install.

use std::collections::HashMap;

use renderpilot_application::AppResult;
use renderpilot_detection::DetectedLibraryFile;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GraphicsComponent, Launcher, LibraryArtifact,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

use super::paths;

/// Snapshot of catalog installs keyed by [`paths::install_path_match_key`].
///
/// Loaded once per scan so multi-root parent scans do not call `list_games`
/// once per derived install.
pub(super) struct CatalogInstallIndex {
    by_install_path: HashMap<String, GameInstallation>,
}

impl CatalogInstallIndex {
    pub(super) fn load(storage: &SqliteStorage) -> Result<Self, ServiceError> {
        let games = storage.list_games()?;
        let mut by_install_path = HashMap::with_capacity(games.len());

        for game in games {
            let key = paths::install_path_match_key(game.install_path().as_str());
            by_install_path.entry(key).or_insert(game);
        }

        Ok(Self { by_install_path })
    }

    fn find_by_install_path(&self, install_path: &str) -> Option<&GameInstallation> {
        self.by_install_path
            .get(&paths::install_path_match_key(install_path))
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

    let mut game = GameInstallation::new(
        identity,
        discovered.platform(),
        discovered.runtime(),
        discovered.install_path().clone(),
    );

    for candidate in discovered.executable_candidates() {
        game = game.with_executable_candidate(candidate.clone());
    }

    game
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

pub(super) fn build_graphics_components(
    game: &GameInstallation,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<GraphicsComponent>> {
    // Components and artifacts are grouped by the same `(directory, family)` rule
    // so a detected bundle (e.g. FSR 4's three DLLs) yields one component and one
    // matching artifact instead of three independent single-file entries.
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
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };

    use super::merge_scan_game_with_existing;

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
        );

        let merged = merge_scan_game_with_existing(&existing, &discovered);

        assert_eq!(merged.identity().launcher(), Launcher::Gog);
        assert_eq!(merged.identity().external_id(), Some("1207659999"));
        assert_eq!(merged.identity().title(), "The Witcher 3");
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
}
