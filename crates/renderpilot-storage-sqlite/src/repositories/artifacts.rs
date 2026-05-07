use renderpilot_application::{AppResult, ArtifactRepository};
use renderpilot_domain::LibraryArtifact;
use rusqlite::{named_params, Connection};

use crate::{error::storage_error, mapping};

use super::{row_mapping::artifact_from_row, SqliteStorage};

const LIST_ARTIFACTS_SQL: &str = "
    SELECT
        id,
        technology,
        file_name,
        file_path,
        version,
        sha256,
        source,
        source_game_id,
        trust_level
    FROM library_artifacts
    ORDER BY technology, file_name, file_path
";

const UPSERT_ARTIFACT_SQL: &str = "
    INSERT INTO library_artifacts
        (
            id,
            technology,
            file_name,
            file_path,
            version,
            sha256,
            source,
            source_game_id,
            trust_level,
            updated_at
        )
    VALUES
        (
            :id,
            :technology,
            :file_name,
            :file_path,
            :version,
            :sha256,
            :source,
            :source_game_id,
            :trust_level,
            CAST(unixepoch('subsec') * 1000 AS INTEGER)
        )
    ON CONFLICT(sha256) DO UPDATE SET
        technology     = excluded.technology,
        file_name      = excluded.file_name,
        file_path      = excluded.file_path,
        version        = excluded.version,
        source         = excluded.source,
        source_game_id = excluded.source_game_id,
        trust_level    = excluded.trust_level,
        updated_at     = excluded.updated_at
";

impl ArtifactRepository for SqliteStorage {
    fn upsert_artifact(&self, artifact: &LibraryArtifact) -> AppResult<()> {
        let connection = self.connection()?;

        upsert_artifact_in_connection(&connection, artifact)
    }

    fn list_artifacts(&self) -> AppResult<Vec<LibraryArtifact>> {
        self.query_list(LIST_ARTIFACTS_SQL, [], artifact_from_row)
    }
}

/// Upserts one artifact row using an existing connection or outer transaction.
///
/// This function intentionally does not start its own transaction.
/// The caller owns transaction boundaries.
pub(super) fn upsert_artifact_in_connection(
    connection: &Connection,
    artifact: &LibraryArtifact,
) -> AppResult<()> {
    let params = ArtifactSqlParams::from_artifact(artifact)?;

    connection
        .execute(
            UPSERT_ARTIFACT_SQL,
            named_params! {
                ":id": params.id,
                ":technology": params.technology,
                ":file_name": params.file_name,
                ":file_path": params.file_path,
                ":version": params.version,
                ":sha256": params.sha256,
                ":source": params.source,
                ":source_game_id": params.source_game_id,
                ":trust_level": params.trust_level,
            },
        )
        .map_err(storage_error)?;

    Ok(())
}

/// Upserts artifact rows using an existing connection or outer transaction.
///
/// This function intentionally does not start its own transaction.
pub(super) fn upsert_artifacts_in_connection(
    connection: &Connection,
    artifacts: &[LibraryArtifact],
) -> AppResult<()> {
    if artifacts.is_empty() {
        return Ok(());
    }

    let mut statement = connection
        .prepare_cached(UPSERT_ARTIFACT_SQL)
        .map_err(storage_error)?;

    for artifact in artifacts {
        let params = ArtifactSqlParams::from_artifact(artifact)?;

        statement
            .execute(named_params! {
                ":id": params.id,
                ":technology": params.technology,
                ":file_name": params.file_name,
                ":file_path": params.file_path,
                ":version": params.version,
                ":sha256": params.sha256,
                ":source": params.source,
                ":source_game_id": params.source_game_id,
                ":trust_level": params.trust_level,
            })
            .map_err(storage_error)?;
    }

    Ok(())
}

#[derive(Debug)]
struct ArtifactSqlParams<'a> {
    id: &'a str,
    technology: String,
    file_name: &'a str,
    file_path: &'a str,
    version: Option<&'a str>,
    sha256: &'a str,
    source: Option<&'a str>,
    source_game_id: Option<&'a str>,
    trust_level: String,
}

impl<'a> ArtifactSqlParams<'a> {
    fn from_artifact(artifact: &'a LibraryArtifact) -> AppResult<Self> {
        Ok(Self {
            id: artifact.id().as_str(),
            technology: mapping::enum_to_text(artifact.technology())?,
            file_name: artifact.file_name(),
            file_path: artifact.path().as_str(),
            version: artifact.version().map(|version| version.as_str()),
            sha256: artifact.sha256().as_str(),
            source: artifact.source(),
            source_game_id: artifact.source_game_id().map(|game_id| game_id.as_str()),
            trust_level: mapping::enum_to_text(artifact.trust_level())?,
        })
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{ArtifactRepository, GameRepository};
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, GameId, GameIdentity, GameInstallation,
        GameRuntime, GraphicsTechnology, Launcher, LibraryArtifact, PathRef, Platform, Sha256Hash,
    };

    use super::SqliteStorage;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn list_artifacts_round_trips_all_required_fields() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let game = sample_game("manual:C:/Games/GameA", "Game A");

        let artifact = sample_artifact(
            "artifact:hash-a",
            "C:/Games/GameA/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_A,
        )
        .with_source_game_id(game.id().clone());

        storage.upsert_game(&game).expect("game should be stored");
        storage
            .upsert_artifact(&artifact)
            .expect("artifact should be stored");

        let artifacts = storage.list_artifacts().expect("artifacts should load");

        assert_eq!(artifacts, vec![artifact]);
    }

    #[test]
    fn upsert_artifact_updates_existing_artifact_with_same_sha256() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let first = sample_artifact(
            "artifact:first",
            "C:/Games/GameA/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_B,
        );

        let second = sample_artifact(
            "artifact:second",
            "C:/Games/GameB/bin/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_B,
        );

        storage
            .upsert_artifact(&first)
            .expect("first artifact should be stored");

        storage
            .upsert_artifact(&second)
            .expect("second artifact should update the existing sha256 row");

        let artifacts = storage.list_artifacts().expect("artifacts should load");

        assert_eq!(
            artifacts.len(),
            1,
            "same sha256 should be stored as one reusable artifact",
        );

        let artifact = &artifacts[0];

        assert_eq!(artifact.sha256(), first.sha256());
        assert_eq!(artifact.file_name(), "nvngx_dlss.dll");
        assert_eq!(
            artifact.path().as_str(),
            "C:/Games/GameB/bin/nvngx_dlss.dll"
        );
    }

    #[test]
    fn list_artifacts_returns_artifacts_sorted_by_technology_file_name_and_path() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let later = sample_artifact(
            "artifact:z",
            "C:/Games/Z/nvngx_dlss.dll",
            "z.dll",
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        );

        let earlier_b = sample_artifact(
            "artifact:b",
            "C:/Games/B/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        );

        let earlier_a = sample_artifact(
            "artifact:a",
            "C:/Games/A/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        );

        storage
            .upsert_artifact(&later)
            .expect("later artifact should store");
        storage
            .upsert_artifact(&earlier_b)
            .expect("earlier_b artifact should store");
        storage
            .upsert_artifact(&earlier_a)
            .expect("earlier_a artifact should store");

        let artifacts = storage.list_artifacts().expect("artifacts should load");

        assert_eq!(artifacts, vec![earlier_a, earlier_b, later]);
    }

    fn sample_game(id: &str, title: &str) -> GameInstallation {
        let identity = GameIdentity::new(
            GameId::new(id).expect("game id should be valid"),
            title,
            Launcher::Manual,
        )
        .expect("game identity should be valid");

        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new("C:/Games/Test").expect("install path should be valid"),
        )
    }

    fn sample_artifact(id: &str, path: &str, file_name: &str, sha256: &str) -> LibraryArtifact {
        LibraryArtifact::new(
            ArtifactId::new(id).expect("artifact id should be valid"),
            GraphicsTechnology::DlssSuperResolution,
            file_name,
            ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
                .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid")),
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact should be valid")
        .with_source("scan-folder")
        .expect("source should be valid")
    }
}
