use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    GameId, GameRuntime, InstallKey, InstallRoot, Launcher, PathRef, PathRefError, Platform,
    text::{RequiredTextError, normalize_required_text},
};

/// Evidence that established the installation root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootAuthority {
    /// A launcher manifest or equivalent authoritative inventory entry.
    LauncherManifest,
    /// The user explicitly confirmed this directory as one installation root.
    UserConfirmed,
    /// A row created before explicit root authority was persisted.
    #[default]
    Legacy,
}

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
    #[serde(rename = "install_path")]
    install_root: InstallRoot,
    root_authority: RootAuthority,
    executable_candidates: Vec<PathRef>,
    #[serde(default)]
    confirmed_executable: Option<PathRef>,
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
            install_root: InstallRoot::new(install_path),
            root_authority: RootAuthority::Legacy,
            executable_candidates: Vec::new(),
            confirmed_executable: None,
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
        self.install_root.path()
    }

    /// Returns the typed installation root.
    pub const fn install_root(&self) -> &InstallRoot {
        &self.install_root
    }

    /// Returns the canonical identity of the physical installation root.
    pub fn install_key(&self) -> &InstallKey {
        self.install_root.key()
    }

    /// Returns the evidence that established the installation root.
    pub fn root_authority(&self) -> RootAuthority {
        self.root_authority
    }

    /// Sets the evidence that established the installation root.
    #[must_use]
    pub fn with_root_authority(mut self, authority: RootAuthority) -> Self {
        self.root_authority = authority;
        self
    }

    /// Returns candidate executable paths.
    pub fn executable_candidates(&self) -> &[PathRef] {
        &self.executable_candidates
    }

    /// Returns the executable explicitly confirmed by the user, if any.
    pub fn confirmed_executable(&self) -> Option<&PathRef> {
        self.confirmed_executable.as_ref()
    }

    /// Adds an executable candidate and returns the updated installation.
    pub fn with_executable_candidate(mut self, candidate: PathRef) -> Self {
        self.executable_candidates.push(candidate);
        self
    }

    /// Records an explicit user choice and keeps it in the executable
    /// candidate set used by compatibility analysis.
    #[must_use]
    pub fn with_confirmed_executable(mut self, executable: PathRef) -> Self {
        if !self.executable_candidates.contains(&executable) {
            self.executable_candidates.push(executable.clone());
        }
        self.confirmed_executable = Some(executable);
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

    #[test]
    fn install_key_is_case_and_separator_insensitive_but_game_id_is_not_path_based() {
        let make = |id: &str, path: &str| {
            GameInstallation::new(
                GameIdentity::new(GameId::new(id).expect("id"), "Game", Launcher::Manual)
                    .expect("identity"),
                Platform::Windows,
                GameRuntime::NativeWindows,
                PathRef::new(path).expect("path"),
            )
        };
        let first = make("game:stable", r"C:\Games\Black Flag\\");
        let same_install = make("game:other", "c:/games/black flag");

        assert_eq!(first.install_key(), same_install.install_key());
        assert_ne!(first.id(), same_install.id());
        assert_eq!(first.id().as_str(), "game:stable");
    }

    #[test]
    fn confirmed_executable_is_an_explicit_candidate_without_duplication() {
        let executable = PathRef::new("bin/GameLauncher.exe").expect("path");
        let installation = GameInstallation::new(
            GameIdentity::new(
                GameId::new("game:confirmed-exe").expect("id"),
                "Game",
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new("C:/Games/Game").expect("install path"),
        )
        .with_executable_candidate(executable.clone())
        .with_confirmed_executable(executable.clone());

        assert_eq!(installation.confirmed_executable(), Some(&executable));
        assert_eq!(installation.executable_candidates(), &[executable]);
    }
}
