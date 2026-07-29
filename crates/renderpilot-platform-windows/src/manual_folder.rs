use std::path::{Path, PathBuf};

use renderpilot_application::{AppError, AppResult, GameSourceProvider};
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform, RootAuthority,
};

use crate::install_identity::{InstallIdentityDetails, detect_install_identity};
use crate::path_normalize::canonicalize_install_path;

/// Game source backed by one user-selected install folder.
///
/// The folder may be a true manual install or a Steam / GOG / Epic directory
/// discovered via [`crate::detect_install_identity`]. New identities are
/// opaque and path-independent; catalog reconciliation preserves them across
/// subsequent scans of the same install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualFolderGameSource {
    folder: PathBuf,
    known_identity: Option<InstallIdentityDetails>,
    game_id: GameId,
    root_authority: RootAuthority,
    explicit_executable: Option<PathBuf>,
}

impl ManualFolderGameSource {
    /// Creates a manual folder source.
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
            known_identity: None,
            game_id: GameId::generate(),
            root_authority: RootAuthority::Legacy,
            explicit_executable: None,
        }
    }

    /// Uses launcher metadata already resolved by an authoritative discovery adapter.
    #[must_use]
    pub fn with_known_identity(mut self, identity: InstallIdentityDetails) -> Self {
        self.known_identity = Some(identity);
        self
    }

    /// Uses a catalog identity allocated independently from the install path.
    #[must_use]
    pub fn with_game_id(mut self, game_id: GameId) -> Self {
        self.game_id = game_id;
        self
    }

    /// Records the evidence that established this source's root.
    #[must_use]
    pub fn with_root_authority(mut self, authority: RootAuthority) -> Self {
        self.root_authority = authority;
        self
    }

    /// Preserves an executable explicitly confirmed by the user even when
    /// ranking classifies it as a launcher/helper.
    #[must_use]
    pub fn with_explicit_executable(mut self, executable: PathBuf) -> Self {
        self.explicit_executable = Some(executable);
        self
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

        // Match auto-discovery path form so catalog reconciliation can locate
        // an existing install independent of the source's provisional id.
        let folder = canonicalize_install_path(&self.folder).map_err(|error| {
            AppError::invalid_input(format!(
                "game folder could not be resolved to a stable filesystem identity: {} ({error})",
                self.folder.display()
            ))
        })?;

        let path_text = folder.to_string_lossy();
        let install_path = PathRef::new(path_text.as_ref())
            .map_err(|error| AppError::invalid_input(error.to_string()))?;
        let folder_title = folder_title(&folder);

        let identity = game_identity_for_install_folder(
            &folder,
            folder_title,
            self.known_identity.clone(),
            self.game_id.clone(),
        )?;

        let installation = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            install_path,
        )
        .with_root_authority(self.root_authority);

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

        let installation = if let Some(executable) = &self.explicit_executable {
            let metadata = std::fs::symlink_metadata(executable).map_err(|error| {
                AppError::invalid_input(format!(
                    "explicit executable is no longer readable: {} ({error})",
                    executable.display()
                ))
            })?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(AppError::invalid_input(format!(
                    "explicit executable must be a regular file inside the installation root: {}",
                    executable.display()
                )));
            }
            let executable = canonicalize_install_path(executable).map_err(|error| {
                AppError::invalid_input(format!(
                    "explicit executable could not be resolved to a stable filesystem identity: {} ({error})",
                    executable.display()
                ))
            })?;
            let relative = executable.strip_prefix(&folder).map_err(|_| {
                AppError::invalid_input(format!(
                    "explicit executable is outside the installation root: {}",
                    executable.display()
                ))
            })?;
            #[cfg(windows)]
            if !crate::executable_detection::is_readable_windows_pe_executable(&executable) {
                return Err(AppError::invalid_input(format!(
                    "explicit executable is no longer a readable Windows PE file: {}",
                    executable.display()
                )));
            }
            let relative = PathRef::new(relative.to_string_lossy().replace('\\', "/"))
                .map_err(|error| AppError::invalid_input(error.to_string()))?;
            installation.with_confirmed_executable(relative)
        } else {
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
    folder_title: String,
    known_identity: Option<InstallIdentityDetails>,
    game_id: GameId,
) -> AppResult<GameIdentity> {
    if let Some(detected) = known_identity.or_else(|| detect_install_identity(folder)) {
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
    use crate::InstallIdentityDetails;

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
    fn provisional_game_identity_is_opaque_and_stable_for_one_source() {
        let root = tempdir().expect("temp dir");
        let source = ManualFolderGameSource::new(root.path());
        let first = source.discover_game().expect("first");
        let repeated = source.discover_game().expect("repeated");
        let independent = ManualFolderGameSource::new(root.path())
            .discover_game()
            .expect("independent");

        assert!(first.id().as_str().starts_with("game:"));
        assert!(
            !first
                .id()
                .as_str()
                .contains(&root.path().to_string_lossy().to_string())
        );
        assert_eq!(first.id(), repeated.id());
        assert_ne!(first.id(), independent.id());
    }

    #[test]
    fn explicit_executable_is_recorded_with_user_confirmation_provenance() {
        let root = tempdir().expect("temp dir");
        let executable = root.path().join("CustomLauncher.exe");
        fs::copy(
            std::env::current_exe().expect("test executable"),
            &executable,
        )
        .expect("executable");

        let game = ManualFolderGameSource::new(root.path())
            .with_explicit_executable(executable)
            .discover_game()
            .expect("game");

        assert_eq!(
            game.confirmed_executable().map(|path| path.as_str()),
            Some("CustomLauncher.exe")
        );
        assert_eq!(
            game.executable_candidates(),
            &[game.confirmed_executable().unwrap().clone()]
        );
    }

    #[cfg(windows)]
    #[test]
    fn explicit_executable_is_revalidated_when_the_game_is_discovered() {
        let root = tempdir().expect("temp dir");
        let executable = root.path().join("CustomLauncher.exe");
        fs::copy(
            std::env::current_exe().expect("test executable"),
            &executable,
        )
        .expect("executable");
        let source =
            ManualFolderGameSource::new(root.path()).with_explicit_executable(executable.clone());

        fs::write(&executable, b"not a PE file anymore").expect("replace executable");

        let error = source
            .discover_game()
            .expect_err("stale executable evidence must be rejected");
        assert!(
            error
                .message()
                .contains("no longer a readable Windows PE file")
        );
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
    fn authoritative_identity_avoids_reopening_launcher_metadata() {
        let root = tempdir().expect("temp dir");
        let game_dir = root
            .path()
            .join("steamapps")
            .join("common")
            .join("IndexedGame");
        fs::create_dir_all(&game_dir).expect("dirs");

        let game = ManualFolderGameSource::new(&game_dir)
            .with_known_identity(InstallIdentityDetails {
                launcher: Launcher::Steam,
                external_id: Some("765".to_owned()),
                display_name: Some("Indexed Game".to_owned()),
            })
            .discover_game()
            .expect("prefetched identity should be sufficient without an appmanifest");

        assert_eq!(game.identity().launcher(), Launcher::Steam);
        assert_eq!(game.identity().external_id(), Some("765"));
        assert_eq!(game.identity().title(), "Indexed Game");
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
