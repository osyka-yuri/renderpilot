//! Artifact-owned observation queries and atomic scope replacement.

use super::*;

impl SqliteStorage {
    /// Lists only observations owned by one artifact verifier.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub fn list_artifact_observations(
        &self,
        artifact_id: &ArtifactId,
    ) -> AppResult<Vec<StoredFileObservation>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "
                    SELECT normalized_path, identity_kind, object_identity, change_token,
                           size, algorithm_revision, sha256,
                           version_observed, version, runtime_observed, runtime_json,
                           pe_observed, pe_json
                      FROM file_observations
                     WHERE owner_kind = 'artifact' AND artifact_id = ?1
                     ORDER BY normalized_path
                    ",
                )
                .map_err(storage_error)?;
            let rows = statement
                .query_map([artifact_id.as_str()], |row| {
                    Ok(observation_from_row(
                        row,
                        ObservationOwner::Artifact(artifact_id.clone()),
                    ))
                })
                .map_err(storage_error)?;
            let rows = rows.collect::<Result<Vec<_>, _>>().map_err(storage_error)?;
            rows.into_iter().collect()
        })
    }

    /// Lists every artifact-owned observation in one SQLite read, grouped by
    /// its owning artifact. Game-owned facts are deliberately excluded.
    pub fn list_all_artifact_observations(
        &self,
    ) -> AppResult<HashMap<ArtifactId, Vec<StoredFileObservation>>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "
                    SELECT normalized_path, identity_kind, object_identity, change_token,
                           size, algorithm_revision, sha256,
                           version_observed, version, runtime_observed, runtime_json,
                           pe_observed, pe_json, artifact_id
                      FROM file_observations
                     WHERE owner_kind = 'artifact'
                     ORDER BY artifact_id, normalized_path
                    ",
                )
                .map_err(storage_error)?;
            let rows = statement
                .query_map([], |row| {
                    let artifact_id = row.get::<_, String>(13)?;
                    Ok(artifact_observation_from_row(row, artifact_id))
                })
                .map_err(storage_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage_error)?;
            let mut grouped = HashMap::new();
            for row in rows {
                let (artifact_id, observation) = row?;
                grouped
                    .entry(artifact_id)
                    .or_insert_with(Vec::new)
                    .push(observation);
            }
            Ok(grouped)
        })
    }

    /// Replaces only one artifact owner's observations.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub fn replace_artifact_observations(
        &self,
        artifact_id: &ArtifactId,
        observations: &[StoredFileObservation],
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            ensure_only_artifact_owner(artifact_id, observations)?;
            delete_artifact_observations_within_transaction(transaction, artifact_id)?;
            replace_observations_within_transaction(transaction, observations)
        })
    }

    /// Atomically replaces multiple artifact-owned observation scopes.
    ///
    /// Callers provide only owners whose complete verification succeeded. If
    /// any scope is invalid, the transaction leaves every previous scope
    /// untouched.
    pub fn replace_artifact_observation_scopes(
        &self,
        scopes: &HashMap<ArtifactId, Vec<StoredFileObservation>>,
    ) -> AppResult<()> {
        if scopes.is_empty() {
            return Ok(());
        }
        self.with_transaction(|transaction| {
            for (artifact_id, observations) in scopes {
                ensure_only_artifact_owner(artifact_id, observations)?;
            }
            for artifact_id in scopes.keys() {
                delete_artifact_observations_within_transaction(transaction, artifact_id)?;
            }
            replace_observations_within_transaction(transaction, scopes.values().flatten())?;
            Ok(())
        })
    }
}
