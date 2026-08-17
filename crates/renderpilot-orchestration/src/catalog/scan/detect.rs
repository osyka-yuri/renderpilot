//! Strong per-install library detection.

use std::collections::HashMap;

use renderpilot_detection::{
    DetectedLibraryFile, FILE_OBSERVATION_ALGORITHM_REVISION, FileObservation,
    LibraryPatternComponentDetector, ReusableFileMetadata,
};
use renderpilot_domain::{GameInstallation, PeCompatibilityProfile, RuntimeTarget};
use renderpilot_storage_sqlite::{SqliteStorage, StoredFileObservation};

use crate::ServiceError;

/// Performs one complete traversal and stable-object observation. Reuse is
/// owner-scoped at the storage boundary after this scan; no global cache can
/// skip traversal or become a second authority.
pub(super) fn detect_libraries(
    storage: &SqliteStorage,
    detector: &LibraryPatternComponentDetector,
    game: &GameInstallation,
) -> Result<Vec<DetectedLibraryFile>, ServiceError> {
    let reusable = reusable_game_metadata(storage.list_game_observations(game.id())?)?;
    detector
        .detect_library_files_with_reuse(game, &reusable)
        .map_err(Into::into)
}

fn reusable_game_metadata(
    observations: Vec<StoredFileObservation>,
) -> Result<HashMap<String, ReusableFileMetadata>, ServiceError> {
    observations
        .into_iter()
        .filter(|observation| {
            observation.algorithm_revision == u32::from(FILE_OBSERVATION_ALGORITHM_REVISION)
                && observation.version_observed
                && observation.runtime_observed
                && observation.pe_observed
        })
        .map(|observation| {
            let runtime_target =
                parse_fact::<RuntimeTarget>(observation.runtime_json.as_deref(), "runtime")?;
            let pe_compatibility = parse_fact::<PeCompatibilityProfile>(
                observation.pe_json.as_deref(),
                "PE compatibility",
            )?;
            let path = observation.normalized_path.as_str().to_owned();
            Ok((
                path,
                ReusableFileMetadata {
                    observation: FileObservation {
                        path: observation.normalized_path,
                        identity_kind: observation.identity_kind,
                        object_identity: observation.object_identity,
                        change_token: observation.change_token,
                        size: observation.size,
                        sha256: observation.sha256,
                    },
                    version: observation.version,
                    runtime_target,
                    pe_compatibility,
                },
            ))
        })
        .collect()
}

fn parse_fact<T>(value: Option<&str>, name: &str) -> Result<Option<T>, ServiceError>
where
    T: serde::de::DeserializeOwned,
{
    value
        .map(|value| {
            serde_json::from_str(value).map_err(|error| {
                ServiceError::command_failed(format!(
                    "invalid persisted {name} observation: {error}"
                ))
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use renderpilot_application::AppResult;
    use renderpilot_detection::{
        FILE_OBSERVATION_ALGORITHM_REVISION, FileIdentityProbeResult, FileObservationResult,
        FileObservationSource, LibraryPatternComponentDetector, StrongFileIdentity, sha256_bytes,
    };
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };
    use renderpilot_storage_sqlite::{
        AuthorityCas, CompleteScanWriteUnit, ObservationOwner, StoredFileObservation,
    };

    use super::{detect_libraries, reusable_game_metadata};

    #[derive(Clone)]
    struct ProbeOnlySource {
        identity: StrongFileIdentity,
        full_reads: Arc<AtomicUsize>,
    }

    impl FileObservationSource for ProbeOnlySource {
        fn observe(&self, _path: &Path) -> AppResult<FileObservationResult> {
            self.full_reads.fetch_add(1, Ordering::SeqCst);
            Ok(FileObservationResult::Unavailable)
        }

        fn probe_identity(&self, _path: &Path) -> AppResult<FileIdentityProbeResult> {
            Ok(FileIdentityProbeResult::Available(self.identity.clone()))
        }
    }

    #[test]
    fn stored_game_observation_is_loaded_for_identity_only_reuse() {
        let folder = tempfile::tempdir().expect("game folder");
        let path = folder.path().join("nvngx_dlss.dll");
        std::fs::write(&path, b"stored observation").expect("fixture DLL");
        let normalized_path =
            PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("normalized path");
        let game_id = GameId::new("manual:stored-observation").expect("game id");
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Stored Observation", Launcher::Manual)
                .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(folder.path().to_string_lossy().replace('\\', "/")).expect("root"),
        );
        let identity = StrongFileIdentity {
            kind: "test_identity".to_owned(),
            object_identity: "object-1".to_owned(),
            change_token: "token-1".to_owned(),
            size: 18,
        };
        let observation = StoredFileObservation {
            owner: ObservationOwner::Game(game_id),
            normalized_path,
            identity_kind: identity.kind.clone(),
            object_identity: identity.object_identity.clone(),
            change_token: identity.change_token.clone(),
            size: identity.size,
            algorithm_revision: u32::from(FILE_OBSERVATION_ALGORITHM_REVISION),
            sha256: sha256_bytes(b"stored observation").expect("hash"),
            version_observed: true,
            version: None,
            runtime_observed: true,
            runtime_json: None,
            pe_observed: true,
            pe_json: None,
        };
        let storage = renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("storage");
        storage
            .save_complete_scan_write_unit(CompleteScanWriteUnit {
                game: &game,
                components: &[],
                artifacts: &[],
                observations: &[observation],
                authority: AuthorityCas::new(0),
                prune_empty_operations: false,
            })
            .expect("complete observation scan");

        let full_reads = Arc::new(AtomicUsize::new(0));
        let detector = LibraryPatternComponentDetector::windows_default()
            .expect("patterns")
            .with_file_observation_source(Arc::new(ProbeOnlySource {
                identity,
                full_reads: Arc::clone(&full_reads),
            }));

        let files = detect_libraries(&storage, &detector, &game).expect("identity-only reuse");

        assert_eq!(files.len(), 1);
        assert_eq!(full_reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn reusable_metadata_requires_current_revision_and_complete_fact_masks() {
        let game_id = GameId::new("manual:observation-mask").expect("game id");
        let mut observation = StoredFileObservation {
            owner: ObservationOwner::Game(game_id),
            normalized_path: PathRef::new("C:/Games/Test/nvngx_dlss.dll").expect("path"),
            identity_kind: "test_identity".to_owned(),
            object_identity: "object-1".to_owned(),
            change_token: "token-1".to_owned(),
            size: 1,
            algorithm_revision: u32::from(FILE_OBSERVATION_ALGORITHM_REVISION),
            sha256: sha256_bytes(b"x").expect("hash"),
            version_observed: true,
            version: None,
            runtime_observed: true,
            runtime_json: None,
            pe_observed: true,
            pe_json: None,
        };

        assert_eq!(
            reusable_game_metadata(vec![observation.clone()])
                .expect("observed absence is reusable")
                .len(),
            1
        );

        observation.algorithm_revision = 1;
        assert!(
            reusable_game_metadata(vec![observation.clone()])
                .expect("stale revision is ignored")
                .is_empty()
        );

        observation.algorithm_revision = u32::from(FILE_OBSERVATION_ALGORITHM_REVISION);
        observation.pe_observed = false;
        assert!(
            reusable_game_metadata(vec![observation])
                .expect("partial fact coverage is ignored")
                .is_empty()
        );
    }
}
