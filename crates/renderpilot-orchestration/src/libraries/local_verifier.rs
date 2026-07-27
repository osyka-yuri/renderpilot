//! Shared verification of locally registered catalog artifacts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use renderpilot_domain::{LibraryArtifact, Sha256Hash};

use super::types::LibraryLocalState;

/// Memoizes file-integrity checks across packages that share content blobs.
#[derive(Default)]
pub(super) struct LocalArtifactVerifier {
    files: HashMap<(PathBuf, Sha256Hash), LibraryLocalState>,
}

impl LocalArtifactVerifier {
    /// Classifies the complete local registration without collapsing absent and corrupt files.
    pub(super) fn artifact_state(&mut self, artifact: &LibraryArtifact) -> LibraryLocalState {
        let mut result = LibraryLocalState::Verified;
        for file in artifact.files() {
            let expected = file
                .sha256()
                .expect("library artifact invariant violated: sha256 must be present");
            match self.file_state(Path::new(file.path().as_str()), expected) {
                LibraryLocalState::Corrupt => return LibraryLocalState::Corrupt,
                LibraryLocalState::Missing => result = LibraryLocalState::Missing,
                LibraryLocalState::Verified => {}
                LibraryLocalState::Absent => {
                    unreachable!("individual registered files are never absent registrations")
                }
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
            Ok(_) => match crate::fs::sha256_of_non_empty_file(path) {
                Ok(actual) if actual == *expected => LibraryLocalState::Verified,
                Ok(_) | Err(_) => LibraryLocalState::Corrupt,
            },
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
        let mut verifier = LocalArtifactVerifier::default();
        assert_eq!(
            verifier.artifact_state(&artifact(&path)),
            LibraryLocalState::Missing
        );

        std::fs::write(&path, b"corrupt").expect("corrupt fixture");
        let mut verifier = LocalArtifactVerifier::default();
        assert_eq!(
            verifier.artifact_state(&artifact(&path)),
            LibraryLocalState::Corrupt
        );
    }
}
