use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    text::{normalize_required_text, RequiredTextError},
    GameId, GameRuntime, Launcher, PathRef, PathRefError, Platform,
};

/// Stable identity and user-facing title for a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameIdentity {
    id: GameId,
    title: String,
    launcher: Launcher,
    external_id: Option<String>,
}

impl GameIdentity {
    /// Creates a game identity with a required title.
    pub fn new(
        id: GameId,
        title: impl Into<String>,
        launcher: Launcher,
    ) -> Result<Self, GameModelError> {
        Ok(Self {
            id,
            title: normalize_required_text("title", title)?,
            launcher,
            external_id: None,
        })
    }

    /// Returns the stable game identifier.
    pub fn id(&self) -> &GameId {
        &self.id
    }

    /// Returns the display title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the launcher or source that owns this identity.
    pub fn launcher(&self) -> Launcher {
        self.launcher
    }

    /// Returns the optional launcher-specific external ID.
    pub fn external_id(&self) -> Option<&str> {
        self.external_id.as_deref()
    }

    /// Sets a normalized launcher-specific external ID.
    pub fn with_external_id(
        mut self,
        external_id: impl Into<String>,
    ) -> Result<Self, GameModelError> {
        self.external_id = Some(normalize_required_text("external_id", external_id)?);
        Ok(self)
    }
}

/// Discovered game installation with normalized metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameInstallation {
    identity: GameIdentity,
    platform: Platform,
    runtime: GameRuntime,
    install_path: PathRef,
    executable_candidates: Vec<PathRef>,
}

impl GameInstallation {
    /// Creates a game installation from scanner output.
    pub fn new(
        identity: GameIdentity,
        platform: Platform,
        runtime: GameRuntime,
        install_path: PathRef,
    ) -> Self {
        Self {
            identity,
            platform,
            runtime,
            install_path,
            executable_candidates: Vec::new(),
        }
    }

    /// Returns the game identity.
    pub fn identity(&self) -> &GameIdentity {
        &self.identity
    }

    /// Returns the stable game identifier.
    pub fn id(&self) -> &GameId {
        self.identity.id()
    }

    /// Returns the installation platform.
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Returns the runtime used by the game.
    pub fn runtime(&self) -> GameRuntime {
        self.runtime
    }

    /// Returns the root installation path.
    pub fn install_path(&self) -> &PathRef {
        &self.install_path
    }

    /// Returns candidate executable paths.
    pub fn executable_candidates(&self) -> &[PathRef] {
        &self.executable_candidates
    }

    /// Adds an executable candidate and returns the updated installation.
    pub fn with_executable_candidate(mut self, candidate: PathRef) -> Self {
        self.executable_candidates.push(candidate);
        self
    }
}

/// Error returned when game model data is malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameModelError {
    /// A required text field is empty after trimming whitespace.
    EmptyText(&'static str),
    /// A path reference is invalid.
    InvalidPathRef(PathRefError),
}

impl fmt::Display for GameModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyText(field) => write!(formatter, "{field} cannot be empty"),
            Self::InvalidPathRef(error) => error.fmt(formatter),
        }
    }
}

impl Error for GameModelError {}

impl From<PathRefError> for GameModelError {
    fn from(error: PathRefError) -> Self {
        Self::InvalidPathRef(error)
    }
}

impl From<RequiredTextError> for GameModelError {
    fn from(error: RequiredTextError) -> Self {
        Self::EmptyText(error.field())
    }
}

#[cfg(test)]
mod tests {
    use crate::{GameId, GameRuntime, Launcher, PathRef, Platform};

    use super::{GameIdentity, GameInstallation, GameModelError};

    #[test]
    fn game_identity_normalizes_title_and_external_id() {
        let identity = GameIdentity::new(
            GameId::new("steam:1091500").expect("valid id"),
            " Cyberpunk 2077 ",
            Launcher::Steam,
        )
        .expect("valid identity")
        .with_external_id(" 1091500 ")
        .expect("valid external id");

        assert_eq!(identity.title(), "Cyberpunk 2077");
        assert_eq!(identity.external_id(), Some("1091500"));
    }

    #[test]
    fn game_identity_rejects_blank_title() {
        let error = GameIdentity::new(
            GameId::new("steam:1091500").expect("valid id"),
            " ",
            Launcher::Steam,
        )
        .expect_err("title should be required");

        assert_eq!(error, GameModelError::EmptyText("title"));
    }

    #[test]
    fn game_installation_keeps_normalized_path_refs() {
        let identity = GameIdentity::new(
            GameId::new("steam:1091500").expect("valid id"),
            "Cyberpunk 2077",
            Launcher::Steam,
        )
        .expect("valid identity");

        let installation = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(r"C:\Games\Cyberpunk 2077").expect("valid path"),
        )
        .with_executable_candidate(
            PathRef::new(r"C:\Games\Cyberpunk 2077\bin\x64\Cyberpunk2077.exe")
                .expect("valid executable"),
        );

        assert_eq!(
            installation.install_path().as_str(),
            "C:/Games/Cyberpunk 2077"
        );
        assert_eq!(installation.executable_candidates().len(), 1);
    }
}
