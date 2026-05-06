use renderpilot_application::{AppResult, ArtifactRepository};
use renderpilot_domain::LibraryArtifact;
use rusqlite::params;

use crate::{error::storage_error, mapping};

use super::{row_mapping::artifact_from_row, SqliteStorage};

impl ArtifactRepository for SqliteStorage {
    fn upsert_artifact(&self, artifact: &LibraryArtifact) -> AppResult<()> {
        let connection = self.connection()?;
        let technology = mapping::enum_to_text(artifact.technology())?;
        let version = artifact.version().map(|version| version.as_str());
        let trust_level = mapping::enum_to_text(artifact.trust_level())?;

        connection
            .execute(
                "INSERT INTO library_artifacts (
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
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, unixepoch('subsec') * 1000)
                 ON CONFLICT(sha256) DO UPDATE SET
                    technology = excluded.technology,
                    file_name = excluded.file_name,
                    file_path = excluded.file_path,
                    version = excluded.version,
                    source = excluded.source,
                    source_game_id = excluded.source_game_id,
                    trust_level = excluded.trust_level,
                    updated_at = excluded.updated_at",
                params![
                    artifact.id().as_str(),
                    technology,
                    artifact.file_name(),
                    artifact.path().as_str(),
                    version,
                    artifact.sha256().as_str(),
                    artifact.source(),
                    artifact.source_game_id().map(|game_id| game_id.as_str()),
                    trust_level
                ],
            )
            .map_err(storage_error)?;

        Ok(())
    }

    fn list_artifacts(&self) -> AppResult<Vec<LibraryArtifact>> {
        self.query_list(
            "SELECT id, technology, file_name, file_path, version, sha256, source, source_game_id, trust_level
             FROM library_artifacts
             ORDER BY technology, file_name, file_path",
            [],
            artifact_from_row,
        )
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

    #[test]
    fn list_artifacts_round_trips_all_required_fields() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let game = sample_game("manual:C:/Games/GameA", "Game A");
        let artifact = sample_artifact(
            "artifact:hash-a",
            "C:/Games/GameA/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
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
    fn identical_sha256_is_not_duplicated() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let first = sample_artifact(
            "artifact:first",
            "C:/Games/GameA/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        let second = sample_artifact(
            "artifact:second",
            "C:/Games/GameB/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );

        storage
            .upsert_artifact(&first)
            .expect("first artifact should be stored");
        storage
            .upsert_artifact(&second)
            .expect("second artifact should merge");

        let artifacts = storage.list_artifacts().expect("artifacts should load");

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].sha256(), first.sha256());
        assert_eq!(artifacts[0].file_name(), "nvngx_dlss.dll");
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
