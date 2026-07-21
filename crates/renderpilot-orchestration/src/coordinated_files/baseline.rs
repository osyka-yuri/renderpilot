//! Classic `.bak` baseline resolution and conflict vocabulary.

use std::fmt;
use std::path::{Path, PathBuf};

use renderpilot_application::AppError;
use renderpilot_domain::{
    ComponentFile, GraphicsTechnology, ManagedAddonFile, ManagedFileBaseline, ManagedFileMode,
    PathRef, Sha256Hash,
};

/// The trustworthy source selected for one pre-mutation file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedBaseline {
    /// A catalog baseline already exists and was verified against disk.
    RecordedBaseline(ComponentFile),
    /// An unrecorded classic sidecar was verified and adopted.
    ExistingSidecarBaseline(ComponentFile),
    /// No sidecar exists; the current live bytes become the first baseline.
    FreshLiveBaseline(ComponentFile),
    /// An add-on owns the live file and recorded that the path was originally absent.
    AddonOwnedAbsent,
}

impl ResolvedBaseline {
    pub(crate) fn file(&self) -> Option<&ComponentFile> {
        match self {
            Self::RecordedBaseline(file)
            | Self::ExistingSidecarBaseline(file)
            | Self::FreshLiveBaseline(file) => Some(file),
            Self::AddonOwnedAbsent => None,
        }
    }
}

/// Typed reason why disk state cannot safely be interpreted as a baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BaselineConflict {
    OutsideGameRoot(PathBuf),
    NotAFile(PathBuf),
    Empty(PathBuf),
    Unreadable(PathBuf, String),
    MissingRecordedBytes(PathBuf),
    MissingRecordedHash(PathBuf),
    HashMismatch {
        path: PathBuf,
        expected: Sha256Hash,
        actual: Sha256Hash,
    },
    UnexpectedSidecarForAbsentBaseline(PathBuf),
    InvalidPath(String),
    MissingActiveFile(PathBuf),
    MissingActiveHash(PathBuf),
    ActiveHashMismatch {
        path: PathBuf,
        catalog: Sha256Hash,
        managed: Option<Sha256Hash>,
        actual: Sha256Hash,
    },
}

impl fmt::Display for BaselineConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutsideGameRoot(path) => write!(
                formatter,
                "baseline path is outside the game root: {}",
                path.display()
            ),
            Self::NotAFile(path) => {
                write!(
                    formatter,
                    "baseline path is not a regular file: {}",
                    path.display()
                )
            }
            Self::Empty(path) => write!(formatter, "baseline file is empty: {}", path.display()),
            Self::Unreadable(path, detail) => {
                write!(
                    formatter,
                    "cannot read baseline {}: {detail}",
                    path.display()
                )
            }
            Self::MissingRecordedBytes(path) => write!(
                formatter,
                "recorded baseline bytes are missing for {}",
                path.display()
            ),
            Self::MissingRecordedHash(path) => write!(
                formatter,
                "recorded baseline has no integrity hash for {}",
                path.display()
            ),
            Self::HashMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "baseline hash mismatch for {}: expected {expected}, got {actual}",
                path.display()
            ),
            Self::UnexpectedSidecarForAbsentBaseline(path) => write!(
                formatter,
                "{} exists although the recorded pre-owner baseline was absent",
                path.display()
            ),
            Self::InvalidPath(detail) => formatter.write_str(detail),
            Self::MissingActiveFile(path) => {
                write!(
                    formatter,
                    "active component file is missing: {}",
                    path.display()
                )
            }
            Self::MissingActiveHash(path) => write!(
                formatter,
                "active component has no recorded hash for {}",
                path.display()
            ),
            Self::ActiveHashMismatch {
                path,
                catalog,
                managed,
                actual,
            } => {
                write!(
                    formatter,
                    "active file hash mismatch for {}: catalog expected {catalog}",
                    path.display()
                )?;
                if let Some(managed) = managed {
                    write!(formatter, ", managed owner expected {managed}")?;
                }
                write!(formatter, ", got {actual}")
            }
        }
    }
}

impl std::error::Error for BaselineConflict {}

impl From<BaselineConflict> for AppError {
    fn from(error: BaselineConflict) -> Self {
        AppError::invalid_input(error.to_string())
    }
}

/// Resolves one live path against catalog and add-on ownership facts.
pub(crate) struct BaselineResolver<'a> {
    game_root: &'a Path,
    managed_files: &'a [ManagedAddonFile],
    technology: GraphicsTechnology,
}

impl<'a> BaselineResolver<'a> {
    pub(crate) fn new(
        game_root: &'a Path,
        managed_files: &'a [ManagedAddonFile],
        technology: GraphicsTechnology,
    ) -> Self {
        Self {
            game_root,
            managed_files,
            technology,
        }
    }

    pub(crate) fn resolve(
        &self,
        live_path: &Path,
        recorded: Option<&ComponentFile>,
    ) -> Result<ResolvedBaseline, BaselineConflict> {
        self.require_inside_game(live_path)?;
        let sidecar = crate::fs::backup_path(live_path)
            .map_err(|error| BaselineConflict::InvalidPath(error.to_string()))?;
        self.require_inside_game(&sidecar)?;

        let binding = self.binding_for(live_path);
        if let Some(binding) = binding {
            self.validate_binding_sidecar(binding, &sidecar)?;
        }

        if let Some(recorded) = recorded {
            self.resolve_recorded(live_path, &sidecar, recorded, binding)
        } else if binding.is_some_and(|entry| {
            entry.mode() == ManagedFileMode::Owned
                && matches!(entry.baseline(), ManagedFileBaseline::Absent)
        }) {
            Ok(ResolvedBaseline::AddonOwnedAbsent)
        } else if sidecar.exists() {
            let file = component_file_from_disk(&sidecar, live_path, self.technology)?;
            Ok(ResolvedBaseline::ExistingSidecarBaseline(file))
        } else {
            let file = component_file_from_disk(live_path, live_path, self.technology)?;
            Ok(ResolvedBaseline::FreshLiveBaseline(file))
        }
    }

    fn resolve_recorded(
        &self,
        live_path: &Path,
        sidecar: &Path,
        recorded: &ComponentFile,
        binding: Option<&ManagedAddonFile>,
    ) -> Result<ResolvedBaseline, BaselineConflict> {
        let expected = recorded
            .sha256()
            .cloned()
            .ok_or_else(|| BaselineConflict::MissingRecordedHash(live_path.to_path_buf()))?;

        let bytes_path = if sidecar.exists() {
            sidecar
        } else if binding.is_some_and(|entry| {
            entry.mode() == ManagedFileMode::Reused
                || matches!(entry.baseline(), ManagedFileBaseline::Absent)
        }) {
            return Err(BaselineConflict::MissingRecordedBytes(
                sidecar.to_path_buf(),
            ));
        } else {
            live_path
        };
        let actual = verified_hash(bytes_path)?;
        if actual != expected {
            return Err(BaselineConflict::HashMismatch {
                path: bytes_path.to_path_buf(),
                expected,
                actual,
            });
        }

        let mut refreshed = ComponentFile::new(recorded.path().clone()).with_sha256(actual);
        if let Some(install_as) = recorded.install_as() {
            refreshed = refreshed.with_install_as(install_as);
        }
        Ok(ResolvedBaseline::RecordedBaseline(
            super::with_observed_metadata(refreshed, self.technology, bytes_path),
        ))
    }

    fn validate_binding_sidecar(
        &self,
        binding: &ManagedAddonFile,
        sidecar: &Path,
    ) -> Result<(), BaselineConflict> {
        if binding.mode() != ManagedFileMode::Owned {
            return Ok(());
        }

        match binding.baseline() {
            ManagedFileBaseline::Absent if sidecar.exists() => Err(
                BaselineConflict::UnexpectedSidecarForAbsentBaseline(sidecar.to_path_buf()),
            ),
            ManagedFileBaseline::Absent => Ok(()),
            ManagedFileBaseline::Present { sha256 } => {
                if !sidecar.exists() {
                    return Err(BaselineConflict::MissingRecordedBytes(
                        sidecar.to_path_buf(),
                    ));
                }
                let actual = verified_hash(sidecar)?;
                if &actual != sha256 {
                    return Err(BaselineConflict::HashMismatch {
                        path: sidecar.to_path_buf(),
                        expected: sha256.clone(),
                        actual,
                    });
                }
                Ok(())
            }
        }
    }

    fn binding_for(&self, path: &Path) -> Option<&ManagedAddonFile> {
        self.managed_files
            .iter()
            .find(|binding| crate::paths::same_path(Path::new(binding.path().as_str()), path))
    }

    fn require_inside_game(&self, path: &Path) -> Result<(), BaselineConflict> {
        let root = crate::paths::canonical_candidate(self.game_root).map_err(|error| {
            BaselineConflict::Unreadable(self.game_root.to_path_buf(), error.to_string())
        })?;
        let candidate = crate::paths::canonical_candidate(path)
            .map_err(|error| BaselineConflict::Unreadable(path.to_path_buf(), error.to_string()))?;
        if crate::paths::is_within(&candidate, &root) {
            Ok(())
        } else {
            Err(BaselineConflict::OutsideGameRoot(path.to_path_buf()))
        }
    }
}

/// Resolves the complete immutable baseline for a component.
pub(crate) fn resolve_component_baseline(
    game_root: &Path,
    technology: GraphicsTechnology,
    current: &[ComponentFile],
    recorded: Option<&[ComponentFile]>,
    managed_files: &[ManagedAddonFile],
) -> Result<Vec<ComponentFile>, BaselineConflict> {
    let resolver = BaselineResolver::new(game_root, managed_files, technology);
    if let Some(recorded) = recorded {
        return recorded
            .iter()
            .map(|file| {
                let path = Path::new(file.path().as_str());
                resolver
                    .resolve(path, Some(file))?
                    .file()
                    .cloned()
                    .ok_or_else(|| BaselineConflict::MissingRecordedBytes(path.to_path_buf()))
            })
            .collect();
    }

    current
        .iter()
        .filter_map(
            |file| match resolver.resolve(Path::new(file.path().as_str()), None) {
                Ok(ResolvedBaseline::AddonOwnedAbsent) => None,
                Ok(resolved) => resolved.file().cloned().map(Ok),
                Err(error) => Some(Err(error)),
            },
        )
        .collect()
}

fn component_file_from_disk(
    bytes_path: &Path,
    live_path: &Path,
    technology: GraphicsTechnology,
) -> Result<ComponentFile, BaselineConflict> {
    let sha256 = verified_hash(bytes_path)?;
    let path = PathRef::new(live_path.to_string_lossy().as_ref())
        .map_err(|error| BaselineConflict::InvalidPath(error.to_string()))?;
    let file = ComponentFile::new(path).with_sha256(sha256);
    Ok(super::with_observed_metadata(file, technology, bytes_path))
}

/// Maps [`crate::fs::sha256_of_non_empty_file`] into [`BaselineConflict`] vocabulary.
pub(crate) fn verified_hash(path: &Path) -> Result<Sha256Hash, BaselineConflict> {
    crate::fs::sha256_of_non_empty_file(path).map_err(|error| match error {
        crate::fs::NonEmptyFileError::Unreadable { path, detail } => {
            BaselineConflict::Unreadable(path, detail)
        }
        crate::fs::NonEmptyFileError::NotAFile(path) => BaselineConflict::NotAFile(path),
        crate::fs::NonEmptyFileError::Empty(path) => BaselineConflict::Empty(path),
        crate::fs::NonEmptyFileError::HashFailed(detail) => {
            BaselineConflict::Unreadable(path.to_path_buf(), detail)
        }
    })
}
