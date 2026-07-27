//! Shared verification of locally registered catalog artifacts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use renderpilot_domain::{LibraryArtifact, Sha256Hash};
use renderpilot_storage_sqlite::{FileHashCacheRow, SqliteStorage};

use super::types::LibraryLocalState;

/// Memoizes file-integrity checks across packages that share content blobs.
#[derive(Default)]
pub(super) struct LocalArtifactVerifier {
    files: HashMap<(PathBuf, Sha256Hash), LibraryLocalState>,
    metadata_cache: HashMap<PathBuf, FileHashCacheRow>,
    updates: Vec<FileHashCacheRow>,
}

impl LocalArtifactVerifier {
    pub(super) fn load(storage: &SqliteStorage) -> Result<Self, crate::ServiceError> {
        let metadata_cache = storage
            .load_all_file_hash_cache()?
            .into_iter()
            .map(|row| (PathBuf::from(&row.path), row))
            .collect();
        Ok(Self {
            files: HashMap::new(),
            metadata_cache,
            updates: Vec::new(),
        })
    }

    pub(super) fn persist(self, storage: &SqliteStorage) -> Result<(), crate::ServiceError> {
        storage.save_file_hash_cache(&self.updates)?;
        Ok(())
    }

    /// Classifies the complete local registration without collapsing absent and corrupt files.
    pub(super) fn artifact_state(&mut self, artifact: &LibraryArtifact) -> LibraryLocalState {
        let mut result = LibraryLocalState::Verified;
        for file in artifact.files() {
            let Some(expected) = file.sha256() else {
                return LibraryLocalState::Corrupt;
            };
            match self.file_state(Path::new(file.path().as_str()), expected) {
                LibraryLocalState::Corrupt => return LibraryLocalState::Corrupt,
                LibraryLocalState::Missing => result = LibraryLocalState::Missing,
                LibraryLocalState::Verified => {}
                LibraryLocalState::Absent => return LibraryLocalState::Corrupt,
            }
        }
        result
    }

    fn file_state(&mut self, path: &Path, expected: &Sha256Hash) -> LibraryLocalState {
        let key = (path.to_path_buf(), expected.clone());
        if let Some(result) = self.files.get(&key) {
            return *result;
        }
        let result = match std::fs::metadata(path) {
            Ok(metadata) if !metadata.is_file() => LibraryLocalState::Corrupt,
            Ok(metadata) => {
                let modified_at = metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|duration| u64::try_from(duration.as_millis()).ok());
                if let (Some(modified_at), Some(cached)) =
                    (modified_at, self.metadata_cache.get(path))
                    && cached.size == metadata.len()
                    && cached.modified_at == modified_at
                {
                    if cached.sha256 == *expected {
                        LibraryLocalState::Verified
                    } else {
                        LibraryLocalState::Corrupt
                    }
                } else {
                    match crate::fs::sha256_of_non_empty_file(path) {
                        Ok(actual) => {
                            if let Some(modified_at) = modified_at {
                                let cache_row = FileHashCacheRow {
                                    path: path.to_string_lossy().into_owned(),
                                    size: metadata.len(),
                                    modified_at,
                                    sha256: actual.clone(),
                                    version: None,
                                };
                                self.metadata_cache
                                    .insert(path.to_path_buf(), cache_row.clone());
                                self.updates.push(cache_row);
                            }
                            if actual == *expected {
                                LibraryLocalState::Verified
                            } else {
                                LibraryLocalState::Corrupt
                            }
                        }
                        Err(_) => LibraryLocalState::Corrupt,
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                LibraryLocalState::Missing
            }
            Err(_) => LibraryLocalState::Corrupt,
        };
        self.files.insert(key, result);
        result
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, GraphicsTechnology, LibraryArtifact, PathRef,
    };

    use super::*;

    fn artifact(path: &std::path::Path) -> LibraryArtifact {
        LibraryArtifact::new(
            ArtifactId::new("artifact:local-state").expect("artifact id"),
            GraphicsTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(
                    PathRef::new(path.to_string_lossy().into_owned()).expect("path"),
                )
                .with_sha256(Sha256Hash::new("f".repeat(64)).expect("digest")),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
    }

    #[test]
    fn distinguishes_missing_from_existing_corrupt_content() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("nvngx_dlss.dll");
        let mut verifier = LocalArtifactVerifier {
            files: HashMap::new(),
            metadata_cache: HashMap::new(),
            updates: Vec::new(),
        };
        assert_eq!(
            verifier.artifact_state(&artifact(&path)),
            LibraryLocalState::Missing
        );

        std::fs::write(&path, b"corrupt").expect("corrupt fixture");
        let mut verifier = LocalArtifactVerifier {
            files: HashMap::new(),
            metadata_cache: HashMap::new(),
            updates: Vec::new(),
        };
        assert_eq!(
            verifier.artifact_state(&artifact(&path)),
            LibraryLocalState::Corrupt
        );
    }
}
