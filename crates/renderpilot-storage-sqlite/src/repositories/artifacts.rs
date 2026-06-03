use renderpilot_application::{AppResult, ArtifactRepository};
use renderpilot_domain::LibraryArtifact;
use rusqlite::{named_params, Statement, Transaction};

use crate::{error::storage_error, mapping, sqlite_clock};

use super::{
    catalog_select_sql::LIST_ARTIFACTS_SQL, row_mapping::artifact_from_row, SqliteStorage,
};

const UPSERT_ARTIFACT_SQL: &str = "
    INSERT INTO library_artifacts
        (
            id,
            library,
            file_name,
            files_json,
            source,
            source_game_id,
            trust_level,
            created_at,
            updated_at
        )
    VALUES
        (
            :id,
            :technology,
            :file_name,
            :files_json,
            :source,
            :source_game_id,
            :trust_level,
            :created_at_ms,
            :updated_at_ms
        )
    ON CONFLICT(id) DO UPDATE SET
        library        = excluded.library,
        file_name      = excluded.file_name,
        files_json     = excluded.files_json,
        source         = excluded.source,
        source_game_id = excluded.source_game_id,
        trust_level    = excluded.trust_level,
        updated_at     = excluded.updated_at
";

impl ArtifactRepository for SqliteStorage {
    fn upsert_artifact(&self, artifact: &LibraryArtifact) -> AppResult<()> {
        self.with_transaction(|transaction| {
            upsert_artifact_within_transaction(transaction, artifact)
        })
    }

    fn list_artifacts(&self) -> AppResult<Vec<LibraryArtifact>> {
        self.query_list(LIST_ARTIFACTS_SQL, [], artifact_from_row)
    }
}

/// Upserts one artifact row within a transaction.
///
/// This function requires an active `Transaction` object.
pub(super) fn upsert_artifact_within_transaction(
    transaction: &Transaction<'_>,
    artifact: &LibraryArtifact,
) -> AppResult<()> {
    upsert_artifacts_within_transaction(transaction, std::slice::from_ref(artifact))
}

/// Upserts artifact rows within a transaction.
///
/// This function requires an active `Transaction` object, ensuring that the
/// multiple upserts are atomic. If any step fails, the caller's transaction
/// will be rolled back.
pub(super) fn upsert_artifacts_within_transaction(
    transaction: &Transaction<'_>,
    artifacts: &[LibraryArtifact],
) -> AppResult<()> {
    if artifacts.is_empty() {
        return Ok(());
    }

    let now_ms = sqlite_clock::now_ms(transaction)?;

    let mut statement = transaction
        .prepare_cached(UPSERT_ARTIFACT_SQL)
        .map_err(storage_error)?;

    for artifact in artifacts {
        upsert_artifact_with_statement(&mut statement, artifact, now_ms)?;
    }

    Ok(())
}

fn upsert_artifact_with_statement(
    statement: &mut Statement<'_>,
    artifact: &LibraryArtifact,
    stamp_ms: i64,
) -> AppResult<()> {
    let row = ArtifactSqlRow::from_artifact(artifact)?;

    statement
        .execute(named_params! {
            ":id": row.id,
            ":technology": row.technology,
            ":file_name": row.file_name,
            ":files_json": row.files_json,
            ":source": row.source,
            ":source_game_id": row.source_game_id,
            ":trust_level": row.trust_level,
            ":created_at_ms": stamp_ms,
            ":updated_at_ms": stamp_ms,
        })
        .map_err(storage_error)?;

    Ok(())
}

#[derive(Debug)]
struct ArtifactSqlRow<'a> {
    id: &'a str,
    technology: String,
    file_name: &'a str,
    files_json: String,
    source: Option<&'a str>,
    source_game_id: Option<&'a str>,
    trust_level: String,
}

impl<'a> ArtifactSqlRow<'a> {
    fn from_artifact(artifact: &'a LibraryArtifact) -> AppResult<Self> {
        Ok(Self {
            id: artifact.id().as_str(),
            technology: mapping::enum_to_text(&artifact.technology())?,
            file_name: artifact.file_name(),
            files_json: mapping::serialize_json(artifact.files())?,
            source: artifact.source(),
            source_game_id: artifact.source_game_id().map(|game_id| game_id.as_str()),
            trust_level: mapping::enum_to_text(&artifact.trust_level())?,
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
    fn upsert_artifact_updates_existing_artifact_with_same_id() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        // The artifact id is the bundle's content identity, so the same id means
        // the same bundle and the second upsert updates the row in place.
        let first = sample_artifact(
            "artifact:bundle",
            "C:/Games/GameA/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_B,
        );

        let second = sample_artifact(
            "artifact:bundle",
            "C:/Games/GameB/bin/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_A,
        );

        storage
            .upsert_artifact(&first)
            .expect("first artifact should be stored");

        storage
            .upsert_artifact(&second)
            .expect("second artifact should update the existing id row");

        let artifacts = storage.list_artifacts().expect("artifacts should load");

        assert_eq!(
            artifacts.len(),
            1,
            "same artifact id should be stored as one reusable artifact",
        );

        let artifact = &artifacts[0];

        assert_eq!(artifact.sha256().as_str(), HASH_A, "row updated in place");
        assert_eq!(artifact.file_name(), "nvngx_dlss.dll");
        assert_eq!(
            artifact.path().as_str(),
            "C:/Games/GameB/bin/nvngx_dlss.dll"
        );
    }

    #[test]
    fn upsert_artifact_with_distinct_ids_keeps_separate_rows() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let first = sample_artifact(
            "artifact:first",
            "C:/Games/GameA/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_A,
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
            .expect("second artifact should be stored");

        let artifacts = storage.list_artifacts().expect("artifacts should load");

        assert_eq!(
            artifacts.len(),
            2,
            "distinct ids are distinct bundles and keep separate rows",
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
            vec![
                ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
                    .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid")),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact should be valid")
        .with_source("scan-folder")
        .expect("source should be valid")
    }
}
