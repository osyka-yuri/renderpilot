//! Shared row decoding and owner-scoped observation replacement primitives.

use super::*;

pub(super) fn artifact_observation_from_row(
    row: &Row<'_>,
    artifact_id: String,
) -> AppResult<(ArtifactId, StoredFileObservation)> {
    let artifact_id = ArtifactId::new(artifact_id)
        .map_err(|error| invalid_row(format!("invalid observation artifact id: {error}")))?;
    let observation = observation_from_row(row, ObservationOwner::Artifact(artifact_id.clone()))?;
    Ok((artifact_id, observation))
}

pub(super) fn observation_from_row(
    row: &Row<'_>,
    owner: ObservationOwner,
) -> AppResult<StoredFileObservation> {
    let normalized_path = row.get::<_, String>(0).map_err(storage_error)?;
    let identity_kind = row.get(1).map_err(storage_error)?;
    let object_identity = row.get(2).map_err(storage_error)?;
    let change_token = row.get(3).map_err(storage_error)?;
    let size = row.get::<_, i64>(4).map_err(storage_error)?;
    let algorithm_revision = row.get::<_, i64>(5).map_err(storage_error)?;
    let sha256 = row.get::<_, String>(6).map_err(storage_error)?;
    let version_observed =
        observation_bool(row.get(7).map_err(storage_error)?, "version_observed")?;
    let version = row.get::<_, Option<String>>(8).map_err(storage_error)?;
    let runtime_observed =
        observation_bool(row.get(9).map_err(storage_error)?, "runtime_observed")?;
    let runtime_json = row.get(10).map_err(storage_error)?;
    let pe_observed = observation_bool(row.get(11).map_err(storage_error)?, "pe_observed")?;
    let pe_json = row.get(12).map_err(storage_error)?;
    let size = u64::try_from(size).map_err(|_| invalid_row("negative observation size"))?;
    let algorithm_revision = u32::try_from(algorithm_revision)
        .map_err(|_| invalid_row("observation revision overflow"))?;
    let normalized_path =
        PathRef::new(normalized_path).map_err(|error| invalid_row(error.to_string()))?;
    let sha256 = Sha256Hash::new(sha256).map_err(invalid_row)?;
    let version = version
        .map(Version::parse)
        .transpose()
        .map_err(|error| invalid_row(format!("invalid observation version: {error}")))?;
    Ok(StoredFileObservation {
        owner,
        normalized_path,
        identity_kind,
        object_identity,
        change_token,
        size,
        algorithm_revision,
        sha256,
        version_observed,
        version,
        runtime_observed,
        runtime_json,
        pe_observed,
        pe_json,
    })
}

fn observation_bool(value: i64, field: &str) -> AppResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid_row(format!("invalid {field} flag"))),
    }
}

pub(super) fn delete_artifact_observations_within_transaction(
    transaction: &Transaction<'_>,
    artifact_id: &ArtifactId,
) -> AppResult<()> {
    transaction
        .execute(
            "DELETE FROM file_observations WHERE owner_kind = 'artifact' AND artifact_id = ?1",
            [artifact_id.as_str()],
        )
        .map_err(storage_error)?;
    Ok(())
}

pub(super) fn replace_observations_within_transaction<'a>(
    transaction: &Transaction<'_>,
    observations: impl IntoIterator<Item = &'a StoredFileObservation>,
) -> AppResult<()> {
    let mut observations = observations.into_iter().peekable();
    if observations.peek().is_none() {
        return Ok(());
    }
    let now_ms = sqlite_clock::now_ms(transaction)?;
    let mut statement = transaction
        .prepare_cached(
            "INSERT INTO file_observations
                (owner_kind, owner_id, game_id, artifact_id, normalized_path, identity_kind,
                 object_identity, change_token, size, algorithm_revision, sha256,
                 version_observed, version, runtime_observed, runtime_json,
                 pe_observed, pe_json, created_at, updated_at)
             VALUES
                (:owner_kind, :owner_id, :game_id, :artifact_id, :normalized_path, :identity_kind,
                 :object_identity, :change_token, :size, :algorithm_revision, :sha256,
                 :version_observed, :version, :runtime_observed, :runtime_json,
                 :pe_observed, :pe_json, :created_at, :updated_at)
             ON CONFLICT(owner_kind, owner_id, normalized_path) DO UPDATE SET
                identity_kind = excluded.identity_kind,
                object_identity = excluded.object_identity,
                change_token = excluded.change_token,
                size = excluded.size,
                algorithm_revision = excluded.algorithm_revision,
                sha256 = excluded.sha256,
                version_observed = excluded.version_observed,
                version = excluded.version,
                runtime_observed = excluded.runtime_observed,
                runtime_json = excluded.runtime_json,
                pe_observed = excluded.pe_observed,
                pe_json = excluded.pe_json,
                updated_at = excluded.updated_at",
        )
        .map_err(storage_error)?;
    for observation in observations {
        observation.validate()?;
        let size = i64::try_from(observation.size)
            .map_err(|_| invalid_row("observation size overflow"))?;
        statement
            .execute(named_params! {
                ":owner_kind": observation.owner.kind(),
                ":owner_id": observation.owner.owner_id(),
                ":game_id": observation.owner.game_id(),
                ":artifact_id": observation.owner.artifact_id(),
                ":normalized_path": observation.normalized_path.as_str(),
                ":identity_kind": observation.identity_kind.as_str(),
                ":object_identity": observation.object_identity.as_str(),
                ":change_token": observation.change_token.as_str(),
                ":size": size,
                ":algorithm_revision": i64::from(observation.algorithm_revision),
                ":sha256": observation.sha256.as_str(),
                ":version_observed": i64::from(observation.version_observed),
                ":version": observation.version.as_ref().map(ToString::to_string),
                ":runtime_observed": i64::from(observation.runtime_observed),
                ":runtime_json": observation.runtime_json.as_deref(),
                ":pe_observed": i64::from(observation.pe_observed),
                ":pe_json": observation.pe_json.as_deref(),
                ":created_at": now_ms,
                ":updated_at": now_ms,
            })
            .map_err(storage_error)?;
    }
    Ok(())
}

pub(super) fn ensure_only_game_owner(
    game_id: &GameId,
    observations: &[StoredFileObservation],
) -> AppResult<()> {
    if observations
        .iter()
        .all(|observation| matches!(&observation.owner, ObservationOwner::Game(owner) if owner == game_id))
    {
        Ok(())
    } else {
        Err(invalid_row(
            "game scan may only replace observations owned by that same game",
        ))
    }
}

pub(super) fn ensure_only_artifact_owner(
    artifact_id: &ArtifactId,
    observations: &[StoredFileObservation],
) -> AppResult<()> {
    if observations
        .iter()
        .all(|observation| matches!(&observation.owner, ObservationOwner::Artifact(owner) if owner == artifact_id))
    {
        Ok(())
    } else {
        Err(invalid_row(
            "artifact verification may only replace observations owned by that artifact",
        ))
    }
}
