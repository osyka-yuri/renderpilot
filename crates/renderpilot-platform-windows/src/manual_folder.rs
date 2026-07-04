use std::path::{Path, PathBuf};

use renderpilot_application::{AppError, AppResult, GameSourceProvider};
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};

use crate::install_identity::detect_install_identity;
use crate::path_normalize::canonicalize_install_dir;

/// Game source backed by one user-selected install folder.
///
/// The folder may be a true manual install or a Steam / GOG / Epic directory
/// discovered via [`crate::detect_install_identity`]. Game ids stay on the
/// stable `manual:<path>` scheme so catalog rows remain comparable across
/// auto-scan and folder re-scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualFolderGameSource {
    folder: PathBuf,
}

impl ManualFolderGameSource {
    /// Creates a manual folder source.
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }

    /// Returns the configured folder path.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Discovers the single manual game installation represented by this folder.
    pub fn discover_game(&self) -> AppResult<GameInstallation> {
        self.build_game_installation()
    }

    fn build_game_installation(&self) -> AppResult<GameInstallation> {
        if !self.folder.exists() {
            return Err(AppError::invalid_input(format!(
                "game folder does not exist: {}",
                self.folder.display()
            )));
        }

        if !self.folder.is_dir() {
            return Err(AppError::invalid_input(format!(
                "game folder is not a directory: {}",
                self.folder.display()
            )));
        }

        // Match auto-discovery path form so a re-scan of the same install
        // reuses the same `manual:<path>` game id.
        let folder = canonicalize_install_dir(&self.folder);

        let path_text = folder.to_string_lossy();
        let install_path = PathRef::new(path_text.as_ref())
            .map_err(|error| AppError::invalid_input(error.to_string()))?;
        let folder_title = folder_title(&folder);

        let identity = game_identity_for_install_folder(&folder, &install_path, folder_title)?;

        let installation = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            install_path,
        );

        // Populate executable candidates from the install dir so the
        // NVAPI layer has something to query later. Only the
        // not-rejected ones are persisted — the full ranked list
        // (with rejection reasons) is recomputed on demand for the
        // UI override picker.
        #[cfg(windows)]
        let installation = {
            let mut installation = installation;
            for candidate in crate::executable_detection::detect_executable_candidates(&folder) {
                if candidate.rejection.is_some() {
                    continue;
                }
                if let Ok(path_ref) = PathRef::new(&candidate.relative_path) {
                    installation = installation.with_executable_candidate(path_ref);
                }
            }
            installation
        };

        Ok(installation)
    }
}

impl GameSourceProvider for ManualFolderGameSource {
    fn name(&self) -> &str {
        "manual-folder"
    }

    fn discover_games(&self) -> AppResult<Vec<GameInstallation>> {
        Ok(vec![self.discover_game()?])
    }
}

fn folder_title(folder: &Path) -> String {
    folder
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| folder.display().to_string())
}

fn game_identity_for_install_folder(
    folder: &Path,
    install_path: &PathRef,
    folder_title: String,
) -> AppResult<GameIdentity> {
    let game_id = GameId::new(format!("manual:{}", install_path.as_str()))
        .map_err(|error| AppError::invalid_input(error.to_string()))?;

    if let Some(detected) = detect_install_identity(folder) {
        let title = detected
            .display_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(folder_title);

        let identity = GameIdentity::new(game_id, title, detected.launcher)
            .map_err(|error| AppError::invalid_input(error.to_string()))?;

        return match detected.external_id {
            Some(external_id) => identity
                .with_external_id(external_id)
                .map_err(|error| AppError::invalid_input(error.to_string())),
            None => Ok(identity),
        };
    }

    GameIdentity::new(game_id, folder_title, Launcher::Manual)
        .map_err(|error| AppError::invalid_input(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use renderpilot_application::GameSourceProvider;
    use renderpilot_domain::Launcher;
    use tempfile::tempdir;

    use super::ManualFolderGameSource;

    #[test]
    fn manual_folder_source_builds_manual_game_installation() {
        let root = tempdir().expect("temp dir");
        let folder = root.path().join("manual-game-source");
        fs::create_dir_all(&folder).expect("temp folder should be created");

        let source = ManualFolderGameSource::new(&folder);
        let game = source.discover_game().expect("folder should be valid");

        assert_eq!(game.identity().launcher(), Launcher::Manual);
        assert_eq!(game.identity().title(), "manual-game-source");

        let games = source.discover_games().expect("game list should be valid");

        assert_eq!(games, vec![game]);
    }

    #[test]
    fn manual_folder_source_rejects_missing_folder() {
        let root = tempdir().expect("temp dir");
        let folder = root.path().join("missing-manual-game-source");
        let source = ManualFolderGameSource::new(&folder);

        let error = source
            .discover_game()
            .expect_err("missing folder should fail");

        assert!(error.message().contains("game folder does not exist"));
    }

    #[test]
    fn steam_install_folder_is_tagged_steam_not_manual() {
        let root = tempdir().expect("temp dir");
        let steamapps = root.path().join("steamapps");
        let game_dir = steamapps.join("common").join("TestGameDir");
        fs::create_dir_all(&game_dir).expect("dirs");
        fs::write(
            steamapps.join("appmanifest_1234567.acf"),
            r#""AppState"
{
    "appid"        "1234567"
    "installdir"  "TestGameDir"
    "name"        "My Test Game"
}
"#,
        )
        .expect("acf");

        let game = ManualFolderGameSource::new(&game_dir)
            .discover_game()
            .expect("steam folder should be valid");

        assert_eq!(game.identity().launcher(), Launcher::Steam);
        assert_eq!(game.identity().external_id(), Some("1234567"));
        assert_eq!(game.identity().title(), "My Test Game");
    }

    #[test]
    fn gog_install_folder_is_tagged_gog_not_manual() {
        let root = tempdir().expect("temp dir");
        let folder = root.path().join("gog-manual-source");
        fs::create_dir_all(&folder).expect("dir");
        fs::write(
            folder.join("goggame-999.info"),
            r#"{
                "gameId": "999",
                "name": "GOG Sample",
                "playTasks": [{ "type": "FileTask", "isPrimary": true, "path": "Game.exe" }]
            }"#,
        )
        .expect("info");

        let game = ManualFolderGameSource::new(&folder)
            .discover_game()
            .expect("gog folder should be valid");

        assert_eq!(game.identity().launcher(), Launcher::Gog);
        assert_eq!(game.identity().external_id(), Some("999"));
        assert_eq!(game.identity().title(), "GOG Sample");
    }
}
