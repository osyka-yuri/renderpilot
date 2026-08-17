//! Shared verification of locally registered catalog artifacts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use renderpilot_detection::{
    FILE_OBSERVATION_ALGORITHM_REVISION, FileIdentityProbeResult, FileObservationResult,
    FileObservationSource, StrongFileCacheKey, SystemFileObservationSource,
};
use renderpilot_domain::{ArtifactId, LibraryArtifact, PathRef, Sha256Hash};
use renderpilot_storage_sqlite::{ObservationOwner, SqliteStorage, StoredFileObservation};

use super::types::LibraryLocalState;

/// Memoizes verification within one inventory pass and stages observations by
/// artifact owner. A failed verification deliberately leaves the previous
/// owner scope untouched.
pub(super) struct LocalArtifactVerifier {
    files: HashMap<(PathBuf, Sha256Hash), FileState>,
    persisted: HashMap<ArtifactId, HashMap<String, StoredFileObservation>>,
    observations: HashMap<ArtifactId, Vec<StoredFileObservation>>,
    observation_source: Arc<dyn FileObservationSource>,
}

impl Default for LocalArtifactVerifier {
    fn default() -> Self {
        Self {
            files: HashMap::new(),
            persisted: HashMap::new(),
            observations: HashMap::new(),
            observation_source: Arc::new(SystemFileObservationSource),
        }
    }
}

impl LocalArtifactVerifier {
    pub(super) fn load(storage: &SqliteStorage) -> Result<Self, crate::ServiceError> {
        let persisted = storage
            .list_all_artifact_observations()?
            .into_iter()
            .map(|(artifact_id, observations)| {
                let observations = observations
                    .into_iter()
                    .map(|observation| {
                        (observation.normalized_path.as_str().to_owned(), observation)
                    })
                    .collect();
                (artifact_id, observations)
            })
            .collect();
        Ok(Self {
            persisted,
            ..Self::default()
        })
    }

    /// Verified scopes are replaced together only after every complete
    /// verification pass, so owners never overwrite each other and a database
    /// failure cannot publish a partial verifier batch.
    pub(super) fn persist(self, storage: &SqliteStorage) -> Result<(), crate::ServiceError> {
        storage.replace_artifact_observation_scopes(&self.observations)?;
        Ok(())
    }

    pub(super) fn artifact_state(&mut self, artifact: &LibraryArtifact) -> LibraryLocalState {
        let mut result = LibraryLocalState::Verified;
        let mut observations = Vec::with_capacity(artifact.files().len());
        for file in artifact.files() {
            let Some(expected) = file.sha256() else {
                return LibraryLocalState::Corrupt;
            };
            match self.file_state(Path::new(file.path().as_str()), expected, artifact.id()) {
                FileState::Verified(observation) => {
                    if let Some(mut observation) = *observation {
                        observation.owner = ObservationOwner::Artifact(artifact.id().clone());
                        observations.push(observation);
                    }
                }
                FileState::Corrupt => return LibraryLocalState::Corrupt,
                FileState::Missing => result = LibraryLocalState::Missing,
            }
        }
        if result == LibraryLocalState::Verified {
            self.observations
                .insert(artifact.id().clone(), observations);
        }
        result
    }

    fn file_state(
        &mut self,
        path: &Path,
        expected: &Sha256Hash,
        artifact_id: &ArtifactId,
    ) -> FileState {
        let key = (path.to_path_buf(), expected.clone());
        if let Some(state) = self.files.get(&key) {
            return state.clone();
        }
        let normalized_path = match PathRef::new(path.to_string_lossy().replace('\\', "/")) {
            Ok(path) => path,
            Err(_) => return FileState::Corrupt,
        };
        if let Some(observation) = self
            .persisted
            .get(artifact_id)
            .and_then(|observations| observations.get(normalized_path.as_str()))
            .filter(|observation| {
                observation.algorithm_revision == u32::from(FILE_OBSERVATION_ALGORITHM_REVISION)
                    && observation.sha256 == *expected
            })
        {
            match self.observation_source.probe_identity(path) {
                Ok(FileIdentityProbeResult::Available(identity))
                    if same_identity(observation, &identity) =>
                {
                    let verified = FileState::Verified(Box::new(Some(observation.clone())));
                    self.files.insert(key, verified.clone());
                    return verified;
                }
                Ok(FileIdentityProbeResult::Missing) => {
                    self.files.insert(key, FileState::Missing);
                    return FileState::Missing;
                }
                Ok(FileIdentityProbeResult::Unavailable) | Err(_) => {
                    self.files.insert(key, FileState::Corrupt);
                    return FileState::Corrupt;
                }
                Ok(FileIdentityProbeResult::Available(_))
                | Ok(FileIdentityProbeResult::Uncacheable) => {}
            }
        }
        let result = match self.observation_source.observe(path) {
            Ok(FileObservationResult::Missing) => FileState::Missing,
            Ok(FileObservationResult::Available(snapshot)) if snapshot.sha256 == *expected => {
                let observation = snapshot.cache_key.map(|cache_key| StoredFileObservation {
                    owner: ObservationOwner::Artifact(artifact_id.clone()),
                    normalized_path,
                    identity_kind: cache_key.kind,
                    object_identity: cache_key.object_identity,
                    change_token: cache_key.change_token,
                    size: cache_key.size,
                    algorithm_revision: u32::from(FILE_OBSERVATION_ALGORITHM_REVISION),
                    sha256: snapshot.sha256,
                    version_observed: false,
                    version: None,
                    runtime_observed: false,
                    runtime_json: None,
                    pe_observed: false,
                    pe_json: None,
                });
                FileState::Verified(Box::new(observation))
            }
            Ok(FileObservationResult::Available(_))
            | Ok(FileObservationResult::Unavailable)
            | Err(_) => FileState::Corrupt,
        };
        self.files.insert(key, result.clone());
        result
    }
}

fn same_identity(observation: &StoredFileObservation, identity: &StrongFileCacheKey) -> bool {
    observation.identity_kind == identity.kind
        && observation.object_identity == identity.object_identity
        && observation.change_token == identity.change_token
        && observation.size == identity.size
}

#[derive(Clone)]
enum FileState {
    Verified(Box<Option<StoredFileObservation>>),
    Missing,
    Corrupt,
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use renderpilot_application::{AppResult, ArtifactRepository};
    use renderpilot_detection::{StableFileSnapshot, sha256_bytes};
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, LibraryArtifact, LibraryTechnology,
    };

    use super::*;

    #[derive(Clone)]
    struct UncacheableSource {
        bytes: Vec<u8>,
        full_reads: Arc<AtomicUsize>,
    }

    impl FileObservationSource for UncacheableSource {
        fn observe(&self, _path: &Path) -> AppResult<FileObservationResult> {
            self.full_reads.fetch_add(1, Ordering::SeqCst);
            Ok(FileObservationResult::Available(StableFileSnapshot {
                cache_key: None,
                sha256: sha256_bytes(&self.bytes)?,
                bytes: self.bytes.clone(),
            }))
        }

        fn probe_identity(&self, _path: &Path) -> AppResult<FileIdentityProbeResult> {
            Ok(FileIdentityProbeResult::Uncacheable)
        }
    }

    fn artifact_with_id(id: &str, path: &Path, bytes: &[u8]) -> LibraryArtifact {
        LibraryArtifact::new(
            ArtifactId::new(id).expect("artifact id"),
            LibraryTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(
                    PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path"),
                )
                .with_sha256(sha256_bytes(bytes).expect("hash")),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact")
    }

    fn artifact(path: &Path, bytes: &[u8]) -> LibraryArtifact {
        artifact_with_id("artifact:uncacheable-verifier", path, bytes)
    }

    #[test]
    fn verifier_load_uses_one_select_for_any_artifact_count() {
        let directory = tempfile::tempdir().expect("temp dir");
        let storage = SqliteStorage::in_memory().expect("storage");
        let artifacts = (0..8)
            .map(|index| {
                artifact_with_id(
                    &format!("artifact:batch-{index}"),
                    &directory.path().join(format!("nvngx_dlss_{index}.dll")),
                    format!("artifact bytes {index}").as_bytes(),
                )
            })
            .collect::<Vec<_>>();
        for artifact in &artifacts {
            storage.upsert_artifact(artifact).expect("artifact row");
        }

        let (_, select_count) = storage
            .with_select_statement_count(LocalArtifactVerifier::load)
            .expect("batch verifier load");

        assert_eq!(select_count, 1, "artifact count must not grow SQLite reads");
    }

    #[test]
    fn verified_without_cache_key_clears_the_old_artifact_observation() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("nvngx_dlss.dll");
        let bytes = b"verified uncacheable bytes";
        let artifact = artifact(&path, bytes);
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_artifact(&artifact).expect("artifact row");
        let old = StoredFileObservation {
            owner: ObservationOwner::Artifact(artifact.id().clone()),
            normalized_path: PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path"),
            identity_kind: "obsolete-weak-key".to_owned(),
            object_identity: "old-object".to_owned(),
            change_token: "old-token".to_owned(),
            size: 1,
            algorithm_revision: 1,
            sha256: sha256_bytes(b"old").expect("old hash"),
            version_observed: false,
            version: None,
            runtime_observed: false,
            runtime_json: None,
            pe_observed: false,
            pe_json: None,
        };
        storage
            .replace_artifact_observations(artifact.id(), std::slice::from_ref(&old))
            .expect("old observation");

        let full_reads = Arc::new(AtomicUsize::new(0));
        let mut verifier = LocalArtifactVerifier {
            observation_source: Arc::new(UncacheableSource {
                bytes: bytes.to_vec(),
                full_reads: Arc::clone(&full_reads),
            }),
            persisted: HashMap::from([(
                artifact.id().clone(),
                HashMap::from([(old.normalized_path.as_str().to_owned(), old)]),
            )]),
            ..LocalArtifactVerifier::default()
        };
        assert_eq!(
            verifier.artifact_state(&artifact),
            LibraryLocalState::Verified
        );
        assert_eq!(full_reads.load(Ordering::SeqCst), 1);
        verifier
            .persist(&storage)
            .expect("persist verified empty key set");
        assert!(
            storage
                .list_artifact_observations(artifact.id())
                .expect("observations")
                .is_empty(),
            "Verified(None) must replace the owner scope with no reusable observation"
        );
    }
}
