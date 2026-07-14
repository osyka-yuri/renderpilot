use renderpilot_application::{AppResult, ArtifactRepository};
use std::collections::HashSet;

use renderpilot_domain::{ArtifactTrustLevel, GameId, LibraryArtifact};
use rusqlite::{Statement, Transaction, named_params};

use crate::{error::storage_error, mapping, sqlite_clock};

use super::{
    SqliteStorage, catalog_select_sql::LIST_ARTIFACTS_SQL, row_mapping::artifact_from_row,
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
    WHERE library_artifacts.trust_level != 'ManifestDownloaded'
       OR excluded.trust_level = 'ManifestDownloaded'
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

    fn delete_artifact(&self, id: &renderpilot_domain::ArtifactId) -> AppResult<()> {
        let conn = self.connection()?;
        let mut statement = conn
            .prepare_cached("DELETE FROM library_artifacts WHERE id = ?")
            .map_err(storage_error)?;

        statement.execute([id.as_str()]).map_err(storage_error)?;

        Ok(())
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

/// Drops LocalObserved artifacts previously sourced from `game_id` that are no
/// longer present in the latest scan set for that game.
///
/// Manual restores outside RenderPilot leave stale rows pointing at the same
/// path with outdated version/hash snapshots; pruning on rescan keeps the
/// replacement pool honest. The retained-id check runs in Rust rather than a
/// variable-length `NOT IN` clause, so a large scan cannot hit SQLite's bind
/// parameter limit. ManifestDownloaded / UserImported rows are never removed.
pub(super) fn prune_stale_local_observed_for_game_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    retain: &[LibraryArtifact],
) -> AppResult<()> {
    let local_observed = mapping::enum_to_text(&ArtifactTrustLevel::LocalObserved)?;
    let retained_ids: HashSet<&str> = retain
        .iter()
        .map(|artifact| artifact.id().as_str())
        .collect();
    let stale_ids = {
        let mut statement = transaction
            .prepare(
                "SELECT id FROM library_artifacts
                 WHERE source_game_id = ?1 AND trust_level = ?2",
            )
            .map_err(storage_error)?;
        statement
            .query_map(
                rusqlite::params![game_id.as_str(), local_observed.as_str()],
                |row| row.get::<_, String>(0),
            )
            .map_err(storage_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage_error)?
    };

    for artifact_id in stale_ids {
        if !retained_ids.contains(artifact_id.as_str()) {
            transaction
                .execute(
                    "DELETE FROM library_artifacts WHERE id = ?1",
                    rusqlite::params![artifact_id],
                )
                .map_err(storage_error)?;
        }
    }

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
        ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
        GameIdentity, GameInstallation, GameRuntime, GraphicsComponent, GraphicsTechnology,
        Launcher, LibraryArtifact, PathRef, Platform, Sha256Hash, Swappability,
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
        sample_artifact_with_trust(
            id,
            path,
            file_name,
            sha256,
            ArtifactTrustLevel::LocalObserved,
        )
    }

    fn sample_artifact_with_trust(
        id: &str,
        path: &str,
        file_name: &str,
        sha256: &str,
        trust_level: ArtifactTrustLevel,
    ) -> LibraryArtifact {
        LibraryArtifact::new(
            ArtifactId::new(id).expect("artifact id should be valid"),
            GraphicsTechnology::DlssSuperResolution,
            file_name,
            vec![
                ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
                    .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid")),
            ],
            trust_level,
        )
        .expect("artifact should be valid")
        .with_source(match trust_level {
            ArtifactTrustLevel::ManifestDownloaded => "manifest-download",
            _ => "scan-folder",
        })
        .expect("source should be valid")
    }

    #[test]
    fn upsert_local_observed_does_not_overwrite_manifest_downloaded() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        // Manifest-download artifact pointing to cache path.
        let cache = sample_artifact_with_trust(
            "artifact:bundle",
            "C:/AppData/RenderPilot/libraries/fsr_upscaler_dx12/v1/amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_upscaler_dx12.dll",
            HASH_A,
            ArtifactTrustLevel::ManifestDownloaded,
        );

        storage
            .upsert_artifact(&cache)
            .expect("manifest-download artifact should be stored");

        // Scan finds the same bytes in the game folder after a swap and tries to
        // register a local-observed artifact with the same content-based id.
        let game_folder_scan = sample_artifact_with_trust(
            "artifact:bundle",
            "C:/Games/Game1/amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_upscaler_dx12.dll",
            HASH_A,
            ArtifactTrustLevel::LocalObserved,
        );

        storage
            .upsert_artifact(&game_folder_scan)
            .expect("scan upsert should not error");

        let artifacts = storage.list_artifacts().expect("artifacts should load");
        assert_eq!(artifacts.len(), 1);
        // The manifest-download record must be preserved — its path points to the
        // managed cache and must not be replaced with the game-folder copy.
        assert_eq!(
            artifacts[0].path().as_str(),
            "C:/AppData/RenderPilot/libraries/fsr_upscaler_dx12/v1/amd_fidelityfx_upscaler_dx12.dll",
            "local-observed scan must not overwrite a manifest-downloaded artifact's cache path"
        );
        assert_eq!(
            artifacts[0].trust_level(),
            ArtifactTrustLevel::ManifestDownloaded
        );
    }

    #[test]
    fn prune_removes_stale_local_observed_for_game_but_keeps_others() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let game_a = sample_game("manual:C:/Games/A", "Game A");
        let game_b = sample_game("manual:C:/Games/B", "Game B");
        storage.upsert_game(&game_a).expect("game a");
        storage.upsert_game(&game_b).expect("game b");

        let stale_a = sample_artifact(
            "artifact:stale-a",
            "C:/Games/A/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_A,
        )
        .with_source_game_id(game_a.id().clone());
        let current_a = sample_artifact(
            "artifact:current-a",
            "C:/Games/A/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_B,
        )
        .with_source_game_id(game_a.id().clone());
        let other_b = sample_artifact(
            "artifact:other-b",
            "C:/Games/B/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        )
        .with_source_game_id(game_b.id().clone());
        let cached = sample_artifact_with_trust(
            "artifact:cached",
            "C:/AppData/cache/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            ArtifactTrustLevel::ManifestDownloaded,
        );

        for artifact in [&stale_a, &current_a, &other_b, &cached] {
            storage
                .upsert_artifact(artifact)
                .expect("artifact should store");
        }

        // Rescan game A with only current_a in the write unit (plus empty
        // components): prune runs inside save_scan_write_unit.
        let component = GraphicsComponent::new(
            ComponentId::new("component:a:dlss").expect("component id"),
            game_a.id().clone(),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(PathRef::new("C:/Games/A/nvngx_dlss.dll").expect("path"))
                .with_sha256(Sha256Hash::new(HASH_B).expect("sha")),
        );

        storage
            .save_scan_result(&game_a, &[component], std::slice::from_ref(&current_a))
            .expect("rescan should prune stale LocalObserved");

        let ids: Vec<String> = storage
            .list_artifacts()
            .expect("list")
            .into_iter()
            .map(|artifact| artifact.id().as_str().to_owned())
            .collect();

        assert!(
            !ids.iter().any(|id| id == "artifact:stale-a"),
            "stale LocalObserved for game A must be pruned"
        );
        assert!(ids.iter().any(|id| id == "artifact:current-a"));
        assert!(
            ids.iter().any(|id| id == "artifact:other-b"),
            "LocalObserved from other games must remain"
        );
        assert!(
            ids.iter().any(|id| id == "artifact:cached"),
            "ManifestDownloaded must never be pruned by game scan"
        );
    }

    #[test]
    fn empty_scan_prunes_only_current_games_local_observed_artifacts() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let game_a = sample_game("manual:C:/Games/A", "Game A");
        let game_b = sample_game("manual:C:/Games/B", "Game B");
        storage.upsert_game(&game_a).expect("game a");
        storage.upsert_game(&game_b).expect("game b");

        let stale_a = sample_artifact(
            "artifact:stale-a",
            "C:/Games/A/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_A,
        )
        .with_source_game_id(game_a.id().clone());
        let other_b = sample_artifact(
            "artifact:other-b",
            "C:/Games/B/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_B,
        )
        .with_source_game_id(game_b.id().clone());
        let imported_a = sample_artifact_with_trust(
            "artifact:imported-a",
            "C:/Imported/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            ArtifactTrustLevel::UserImported,
        )
        .with_source_game_id(game_a.id().clone());

        for artifact in [&stale_a, &other_b, &imported_a] {
            storage
                .upsert_artifact(artifact)
                .expect("artifact should store");
        }

        storage
            .save_scan_result(&game_a, &[], &[])
            .expect("empty scan should prune only game a's observations");

        let ids: Vec<String> = storage
            .list_artifacts()
            .expect("list")
            .into_iter()
            .map(|artifact| artifact.id().as_str().to_owned())
            .collect();
        assert!(!ids.iter().any(|id| id == "artifact:stale-a"));
        assert!(ids.iter().any(|id| id == "artifact:other-b"));
        assert!(ids.iter().any(|id| id == "artifact:imported-a"));
    }

    #[test]
    fn prune_handles_a_retain_set_larger_than_sqlite_parameter_limit() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let game = sample_game("manual:C:/Games/Large", "Large Scan");
        storage.upsert_game(&game).expect("game should store");

        // SQLite's common 999-variable limit would reject the old NOT IN
        // implementation. These retained LocalObserved rows are written and
        // pruned in one transaction without a variable-length SQL statement.
        let retain: Vec<LibraryArtifact> = (0..1_100)
            .map(|index| {
                sample_artifact(
                    &format!("artifact:retain-{index}"),
                    &format!("C:/Games/Large/lib-{index}.dll"),
                    &format!("lib-{index}.dll"),
                    &format!("{index:064x}"),
                )
                .with_source_game_id(game.id().clone())
            })
            .collect();

        storage
            .save_scan_result(&game, &[], &retain)
            .expect("large retain set must not exceed SQLite parameter limits");
        assert_eq!(
            storage.list_artifacts().expect("list").len(),
            retain.len(),
            "every retained LocalObserved artifact must survive"
        );
    }

    #[test]
    fn upsert_manifest_downloaded_overwrites_manifest_downloaded() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let first = sample_artifact_with_trust(
            "artifact:bundle",
            "C:/AppData/old/amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_upscaler_dx12.dll",
            HASH_A,
            ArtifactTrustLevel::ManifestDownloaded,
        );

        storage
            .upsert_artifact(&first)
            .expect("first artifact should be stored");

        let second = sample_artifact_with_trust(
            "artifact:bundle",
            "C:/AppData/new/amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_upscaler_dx12.dll",
            HASH_A,
            ArtifactTrustLevel::ManifestDownloaded,
        );

        storage
            .upsert_artifact(&second)
            .expect("second manifest-download artifact should be stored");

        let artifacts = storage.list_artifacts().expect("artifacts should load");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts[0].path().as_str(),
            "C:/AppData/new/amd_fidelityfx_upscaler_dx12.dll",
            "a manifest-downloaded artifact can be updated by another manifest-downloaded one"
        );
    }
}
