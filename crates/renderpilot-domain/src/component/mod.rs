mod hash;

#[cfg(test)]
mod tests;

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    text::{normalize_required_text, RequiredTextError},
    ArtifactId, ComponentId, ComponentKind, GameId, GraphicsTechnology, PathRef, Swappability,
    Version,
};

pub use self::hash::{Sha256Digest, Sha256Hash};

/// Detected graphics component associated with a game installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicsComponent {
    id: ComponentId,
    game_id: GameId,
    kind: ComponentKind,
    technology: GraphicsTechnology,
    swappability: Swappability,
    files: Vec<ComponentFile>,
}

impl GraphicsComponent {
    /// Creates a graphics component with required identity and classification data.
    pub fn new(
        id: ComponentId,
        game_id: GameId,
        kind: ComponentKind,
        technology: GraphicsTechnology,
        swappability: Swappability,
    ) -> Self {
        Self {
            id,
            game_id,
            kind,
            technology,
            swappability,
            files: Vec::new(),
        }
    }

    /// Returns the stable component identifier.
    pub fn id(&self) -> &ComponentId {
        &self.id
    }

    /// Returns the game this component belongs to.
    pub fn game_id(&self) -> &GameId {
        &self.game_id
    }

    /// Returns the component kind.
    pub fn kind(&self) -> ComponentKind {
        self.kind
    }

    /// Returns the detected graphics technology.
    pub fn technology(&self) -> GraphicsTechnology {
        self.technology
    }

    /// Returns the replacement policy for this component.
    pub fn swappability(&self) -> Swappability {
        self.swappability
    }

    /// Returns files that make up this component.
    pub fn files(&self) -> &[ComponentFile] {
        &self.files
    }

    /// Adds a component file and returns the updated component.
    pub fn with_file(mut self, file: ComponentFile) -> Self {
        self.files.push(file);
        self
    }
}

/// File that belongs to a detected graphics component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentFile {
    path: PathRef,
    version: Option<Version>,
    sha256: Option<Sha256Hash>,
    /// Basename this file must be installed *as*, when it differs from the file's
    /// own name. Used by curated packages such as the FSR 4 upgrade, where the
    /// loader DLL is installed as `amd_fidelityfx_dx12.dll`. `None` = own name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    install_as: Option<String>,
}

impl ComponentFile {
    /// Creates a component file from a normalized path reference.
    pub fn new(path: PathRef) -> Self {
        Self {
            path,
            version: None,
            sha256: None,
            install_as: None,
        }
    }

    /// Returns the file path.
    pub fn path(&self) -> &PathRef {
        &self.path
    }

    /// Returns the optional file version.
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }

    /// Returns the optional SHA-256 hash.
    pub fn sha256(&self) -> Option<&Sha256Hash> {
        self.sha256.as_ref()
    }

    /// Returns the basename this file must be installed as, when it differs from
    /// the file's own name.
    pub fn install_as(&self) -> Option<&str> {
        self.install_as.as_deref()
    }

    /// Sets a file version and returns the updated file.
    pub fn with_version(mut self, version: Version) -> Self {
        self.version = Some(version);
        self
    }

    /// Sets a SHA-256 hash and returns the updated file.
    pub fn with_sha256(mut self, sha256: Sha256Hash) -> Self {
        self.sha256 = Some(sha256);
        self
    }

    /// Sets the install-as target basename and returns the updated file.
    pub fn with_install_as(mut self, install_as: impl Into<String>) -> Self {
        self.install_as = Some(install_as.into());
        self
    }
}

/// Library artifact available in the local replacement library.
///
/// An artifact is a *bundle* of one or more files that are swapped together as a
/// unit (e.g. the three AMD FSR 4 DLLs, or the Nvidia Streamline interposer and
/// its siblings). `files[0]` is the primary/representative file and `file_name`
/// is its name; callers are responsible for placing the representative first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryArtifact {
    id: ArtifactId,
    technology: GraphicsTechnology,
    file_name: String,
    files: Vec<ComponentFile>,
    source: Option<String>,
    source_game_id: Option<GameId>,
    trust_level: ArtifactTrustLevel,
}

impl LibraryArtifact {
    /// Creates a library artifact from a non-empty bundle of files.
    ///
    /// Every file must carry a SHA-256 hash, and `files[0]` is treated as the
    /// primary file surfaced by [`LibraryArtifact::primary_file`] and the
    /// single-file convenience accessors.
    pub fn new(
        id: ArtifactId,
        technology: GraphicsTechnology,
        file_name: impl Into<String>,
        files: Vec<ComponentFile>,
        trust_level: ArtifactTrustLevel,
    ) -> Result<Self, ComponentError> {
        if files.is_empty() {
            return Err(ComponentError::EmptyArtifactFiles);
        }
        if files.iter().any(|file| file.sha256().is_none()) {
            return Err(ComponentError::MissingArtifactSha256);
        }

        Ok(Self {
            id,
            technology,
            file_name: normalize_required_text("artifact_file_name", file_name)?,
            files,
            source: None,
            source_game_id: None,
            trust_level,
        })
    }

    /// Returns the stable artifact identifier.
    pub fn id(&self) -> &ArtifactId {
        &self.id
    }

    /// Returns the artifact graphics technology.
    pub fn technology(&self) -> GraphicsTechnology {
        self.technology
    }

    /// Returns the artifact file name.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Returns every file that makes up this artifact bundle.
    pub fn files(&self) -> &[ComponentFile] {
        &self.files
    }

    /// Returns the primary (representative) file of the bundle.
    pub fn primary_file(&self) -> &ComponentFile {
        self.files
            .first()
            .expect("library artifact invariant violated: files must be non-empty")
    }

    /// Returns the primary artifact file path.
    pub fn path(&self) -> &PathRef {
        self.primary_file().path()
    }

    /// Returns the optional primary artifact version.
    pub fn version(&self) -> Option<&Version> {
        self.primary_file().version()
    }

    /// Returns the required primary artifact SHA-256 hash.
    pub fn sha256(&self) -> &Sha256Hash {
        self.primary_file()
            .sha256()
            .expect("library artifact invariant violated: sha256 must be present")
    }

    /// Returns the optional artifact source.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Returns the game this artifact was observed in, when known.
    pub fn source_game_id(&self) -> Option<&GameId> {
        self.source_game_id.as_ref()
    }

    /// Returns the trust level assigned to this artifact.
    pub fn trust_level(&self) -> ArtifactTrustLevel {
        self.trust_level
    }

    /// Sets a normalized artifact source.
    pub fn with_source(mut self, source: impl Into<String>) -> Result<Self, ComponentError> {
        self.source = Some(normalize_required_text("artifact_source", source)?);
        Ok(self)
    }

    /// Associates the artifact with the game it was discovered in.
    pub fn with_source_game_id(mut self, source_game_id: GameId) -> Self {
        self.source_game_id = Some(source_game_id);
        self
    }
}

/// Trust level assigned to an artifact in the local replacement library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactTrustLevel {
    /// Artifact was observed during a local game scan.
    LocalObserved,
    /// Artifact was imported directly by the user.
    UserImported,
    /// Artifact was downloaded from the remote manifest.
    ManifestDownloaded,
    /// Trust level is not known yet.
    Unknown,
}

impl ArtifactTrustLevel {
    /// Returns the stable text representation used by CLI output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalObserved => "local_observed",
            Self::UserImported => "user_imported",
            Self::ManifestDownloaded => "manifest_downloaded",
            Self::Unknown => "unknown",
        }
    }
}

/// Error returned when component data is malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentError {
    /// A required text field is empty after trimming whitespace.
    EmptyText(&'static str),
    /// A SHA-256 hash is not a 64-character hexadecimal string.
    InvalidSha256Hash,
    /// A library artifact was created without a required SHA-256 hash.
    MissingArtifactSha256,
    /// A library artifact was created without any files.
    EmptyArtifactFiles,
}

impl fmt::Display for ComponentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyText(field) => write!(formatter, "{field} cannot be empty"),
            Self::InvalidSha256Hash => {
                formatter.write_str("sha256 must be a 64-character hexadecimal string")
            }
            Self::MissingArtifactSha256 => {
                formatter.write_str("library artifact sha256 is required")
            }
            Self::EmptyArtifactFiles => {
                formatter.write_str("library artifact must contain at least one file")
            }
        }
    }
}

impl Error for ComponentError {}

impl From<RequiredTextError> for ComponentError {
    fn from(error: RequiredTextError) -> Self {
        Self::EmptyText(error.field())
    }
}
