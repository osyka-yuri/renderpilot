use std::path::{Path, PathBuf};

use renderpilot_application::{AppError, AppResult, GameSourceProvider};
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
};

use crate::steam_appmanifest::steam_install_details;

/// Manual game source backed by one user-selected folder.
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

        let path_text = self.folder.to_string_lossy();
        let install_path = PathRef::new(path_text.as_ref())
            .map_err(|error| AppError::invalid_input(error.to_string()))?;
        let folder_title = folder_title(&self.folder);

        let identity =
            game_identity_for_manual_install_folder(&self.folder, &install_path, folder_title)?;

        let mut installation = GameInstallation::new(
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
        {
            for candidate in crate::executable_detection::detect_executable_candidates(&self.folder)
            {
                if candidate.rejection.is_some() {
                    continue;
                }
                if let Ok(path_ref) = PathRef::new(&candidate.relative_path) {
                    installation = installation.with_executable_candidate(path_ref);
                }
            }
        }

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

fn game_identity_for_manual_install_folder(
    folder: &Path,
    install_path: &PathRef,
    folder_title: String,
) -> AppResult<GameIdentity> {
    let game_id = GameId::new(format!("manual:{}", install_path.as_str()))
        .map_err(|error| AppError::invalid_input(error.to_string()))?;

    if let Some(steam) = steam_install_details(folder) {
        let title = steam
            .display_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| folder_title.clone());

        GameIdentity::new(game_id, title, Launcher::Steam)
            .map_err(|error| AppError::invalid_input(error.to_string()))?
            .with_external_id(steam.app_id)
            .map_err(|error| AppError::invalid_input(error.to_string()))
    } else {
        GameIdentity::new(game_id, folder_title, Launcher::Manual)
            .map_err(|error| AppError::invalid_input(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use renderpilot_application::GameSourceProvider;
    use renderpilot_domain::Launcher;

    use super::ManualFolderGameSource;

    #[test]
    fn manual_folder_source_builds_manual_game_installation() {
        let folder = temp_game_folder("manual-game-source");
        fs::create_dir_all(&folder).expect("temp folder should be created");

        let source = ManualFolderGameSource::new(&folder);
        let game = source.discover_game().expect("folder should be valid");

        assert_eq!(game.identity().launcher(), Launcher::Manual);
        let expected_title = folder
            .file_name()
            .and_then(|name| name.to_str())
            .expect("temp folder should have a UTF-8 file name");
        assert_eq!(game.identity().title(), expected_title);

        let games = source.discover_games().expect("game list should be valid");

        assert_eq!(games, vec![game]);

        fs::remove_dir_all(folder).expect("temp folder should be removed");
    }

    #[test]
    fn manual_folder_source_rejects_missing_folder() {
        let folder = temp_game_folder("missing-manual-game-source");
        let source = ManualFolderGameSource::new(&folder);

        let error = source
            .discover_game()
            .expect_err("missing folder should fail");

        assert!(error.message().contains("game folder does not exist"));
    }

    fn temp_game_folder(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();

        std::env::temp_dir().join(format!("renderpilot-{name}-{nanos}"))
    }
}
