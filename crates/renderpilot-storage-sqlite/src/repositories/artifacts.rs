use renderpilot_application::{AppResult, ArtifactRepository};
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

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
            technology,
            file_name,
            files_json,
            metadata_json,
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
            :metadata_json,
            :source,
            :source_game_id,
            :trust_level,
            :created_at_ms,
            :updated_at_ms
        )
    ON CONFLICT(id) DO UPDATE SET
        technology     = excluded.technology,
        file_name      = excluded.file_name,
        files_json     = excluded.files_json,
        metadata_json  = excluded.metadata_json,
        source         = excluded.source,
        source_game_id = excluded.source_game_id,
        trust_level    = excluded.trust_level,
        updated_at     = excluded.updated_at
    WHERE library_artifacts.trust_level != 'catalog_downloaded'
       OR excluded.trust_level = 'catalog_downloaded'
";

impl SqliteStorage {
    /// Process-local revision of only the durable replacement inventory.
    ///
    /// Unlike SQLite's connection-wide `total_changes()`, this fingerprint is
    /// unaffected by favorites, covers, operations, or other catalog writes.
    /// It therefore keeps the expensive replacement universe hot until an
    /// artifact row actually changes.
    pub fn library_artifact_revision(&self) -> AppResult<u64> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached("SELECT id, updated_at FROM library_artifacts ORDER BY id")
                .map_err(storage_error)?;
            let mut rows = statement.query([]).map_err(storage_error)?;
            let mut hasher = DefaultHasher::new();
            while let Some(row) = rows.next().map_err(storage_error)? {
                row.get::<_, String>(0)
                    .map_err(storage_error)?
                    .hash(&mut hasher);
                row.get::<_, i64>(1)
                    .map_err(storage_error)?
                    .hash(&mut hasher);
            }
            Ok(hasher.finish())
        })
    }
}

impl ArtifactRepository for SqliteStorage {
    fn upsert_artifact(&self, artifact: &LibraryArtifact) -> AppResult<()> {
        self.upsert_artifacts(std::slice::from_ref(artifact))
    }

    fn upsert_artifacts(&self, artifacts: &[LibraryArtifact]) -> AppResult<()> {
        self.with_transaction(|transaction| {
            upsert_artifacts_within_transaction(transaction, artifacts)
        })
    }

    fn replace_catalog_package_artifact(
        &self,
        package_id: &str,
        artifact: &LibraryArtifact,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            validate_catalog_package_replacement(package_id, artifact)?;
            transaction
                .execute(
                    "DELETE FROM library_artifacts
                     WHERE id != ?1
                       AND json_extract(
                           metadata_json,
                           '$.catalog_package_receipt.package_id'
                       ) = ?2",
                    rusqlite::params![artifact.id().as_str(), package_id],
                )
                .map_err(storage_error)?;
            upsert_artifacts_within_transaction(transaction, std::slice::from_ref(artifact))
        })
    }

    fn delete_catalog_package_artifacts(&self, package_id: &str) -> AppResult<()> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "DELETE FROM library_artifacts
                     WHERE json_extract(
                         metadata_json,
                         '$.catalog_package_receipt.package_id'
                     ) = ?1",
                    [package_id],
                )
                .map_err(storage_error)?;
            Ok(())
        })
    }

    fn list_artifacts(&self) -> AppResult<Vec<LibraryArtifact>> {
        self.query_list(LIST_ARTIFACTS_SQL, [], artifact_from_row)
    }

    fn delete_artifact(&self, id: &renderpilot_domain::ArtifactId) -> AppResult<()> {
        self.with_connection(|conn| {
            let mut statement = conn
                .prepare_cached("DELETE FROM library_artifacts WHERE id = ?")
                .map_err(storage_error)?;

            statement.execute([id.as_str()]).map_err(storage_error)?;

            Ok(())
        })
    }
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
    validate_catalog_receipt_identity(artifact)?;
    let row = ArtifactSqlRow::from_artifact(artifact)?;

    statement
        .execute(named_params! {
            ":id": row.id,
            ":technology": row.technology,
            ":file_name": row.file_name,
            ":files_json": row.files_json,
            ":metadata_json": row.metadata_json,
            ":source": row.source,
            ":source_game_id": row.source_game_id,
            ":trust_level": row.trust_level,
            ":created_at_ms": stamp_ms,
            ":updated_at_ms": stamp_ms,
        })
        .map_err(storage_error)?;

    Ok(())
}

fn validate_catalog_receipt_identity(artifact: &LibraryArtifact) -> AppResult<()> {
    let Some(receipt) = artifact.metadata().catalog_package_receipt() else {
        return Ok(());
    };
    let expected = receipt.artifact_id();
    if artifact.id() != &expected {
        return Err(storage_error(format!(
            "catalog receipt revision requires artifact id `{expected}`, got `{}`",
            artifact.id()
        )));
    }
    Ok(())
}

fn validate_catalog_package_replacement(
    package_id: &str,
    artifact: &LibraryArtifact,
) -> AppResult<()> {
    let receipt = artifact
        .metadata()
        .catalog_package_receipt()
        .ok_or_else(|| storage_error("catalog package replacement requires a receipt"))?;
    if receipt.package_id != package_id {
        return Err(storage_error(format!(
            "catalog package replacement expected package `{package_id}`, got `{}`",
            receipt.package_id
        )));
    }
    validate_catalog_receipt_identity(artifact)
}

/// Drops LocalObserved artifacts previously sourced from `game_id` that are no
/// longer present in the latest scan set for that game.
///
/// Manual restores outside RenderPilot leave stale rows pointing at the same
/// path with outdated version/hash snapshots; pruning on rescan keeps the
/// replacement pool honest. The retained-id check runs in Rust rather than a
/// variable-length `NOT IN` clause, so a large scan cannot hit SQLite's bind
/// parameter limit. Catalog-downloaded and user-imported rows are never removed.
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
    metadata_json: String,
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
            metadata_json: mapping::serialize_json(artifact.metadata())?,
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
        Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, CatalogPackageReceiptV1,
        CatalogReceiptSchemaV1, CatalogSignatureReceipt, CatalogTargetReceipt, ComponentFile,
        ComponentId, ComponentKind, GameId, GameIdentity, GameInstallation, GameRuntime, Launcher,
        LibraryArtifact, LibraryComponent, LibraryTechnology, PackageRelease, PackageVersion,
        PathRef, Platform, ReleaseChannel, RuntimeCompatibility, RuntimeTarget, Sha256Hash,
        Swappability, UpstreamPackage, UpstreamPackageProvider,
    };

    use super::SqliteStorage;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn artifact_revision_changes_only_with_artifact_inventory() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let game = sample_game("manual:revision", "Revision Game");
        storage.upsert_game(&game).expect("game should be stored");
        let empty_revision = storage
            .library_artifact_revision()
            .expect("empty inventory revision");

        storage
            .save_game_ui_state(game.id().as_str(), true, false)
            .expect("favorite state");
        assert_eq!(
            storage
                .library_artifact_revision()
                .expect("revision after unrelated write"),
            empty_revision,
            "UI-only writes must not invalidate the replacement universe"
        );

        let artifact = sample_artifact(
            "artifact:revision",
            "C:/Games/Revision/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_A,
        );
        storage
            .upsert_artifact(&artifact)
            .expect("artifact should be stored");
        assert_ne!(
            storage
                .library_artifact_revision()
                .expect("revision after artifact write"),
            empty_revision
        );
    }

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
        .with_metadata(
            ArtifactMetadata::default()
                .with_upstream_package(
                    UpstreamPackage::new(
                        UpstreamPackageProvider::NuGet,
                        "Microsoft.Direct3D.D3D12",
                        "1.618.5",
                    )
                    .expect("package metadata"),
                )
                .with_runtime_target(
                    RuntimeTarget::new(Architecture::X64)
                        .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: 618 }),
                ),
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
    fn catalog_receipt_round_trips_with_revision_derived_artifact_id() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let artifact = catalog_artifact(HASH_A);

        storage
            .upsert_artifact(&artifact)
            .expect("catalog receipt should store");

        assert_eq!(
            storage.list_artifacts().expect("artifact should load"),
            vec![artifact]
        );
    }

    #[test]
    fn catalog_receipt_rejects_an_artifact_id_not_derived_from_its_revision() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let artifact = catalog_artifact(HASH_A);
        let mismatched = LibraryArtifact::new(
            ArtifactId::for_package_revision(&Sha256Hash::new(HASH_B).expect("different revision")),
            artifact.technology(),
            artifact.file_name(),
            artifact.files().to_vec(),
            artifact.trust_level(),
        )
        .expect("artifact")
        .with_metadata(artifact.metadata().clone());

        let error = storage
            .upsert_artifact(&mismatched)
            .expect_err("identity mismatch must fail closed");
        assert!(
            error
                .to_string()
                .contains("catalog receipt revision requires artifact id")
        );
    }

    #[test]
    fn catalog_package_replacement_keeps_only_the_latest_registration() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let first = catalog_artifact(HASH_A);
        let second = catalog_artifact(HASH_B);

        storage
            .replace_catalog_package_artifact("nvidia.dlss", &first)
            .expect("first package registration");
        storage
            .replace_catalog_package_artifact("nvidia.dlss", &second)
            .expect("replacement package registration");

        assert_eq!(
            storage.list_artifacts().expect("artifact list"),
            vec![second],
            "one logical package must have one current registration"
        );
    }

    #[test]
    fn invalid_catalog_package_replacement_leaves_previous_registration_untouched() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let first = catalog_artifact(HASH_A);
        let second = catalog_artifact(HASH_B);

        storage
            .replace_catalog_package_artifact("nvidia.dlss", &first)
            .expect("first package registration");
        storage
            .replace_catalog_package_artifact("different.package", &second)
            .expect_err("package mismatch must fail");

        assert_eq!(
            storage.list_artifacts().expect("artifact list"),
            vec![first],
            "failed replacement must roll back its delete"
        );
    }

    #[test]
    fn deleting_a_catalog_package_removes_only_its_registrations() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let first = catalog_artifact_for("nvidia.dlss", HASH_A);
        let stale = catalog_artifact_for("nvidia.dlss", HASH_B);
        let unrelated = catalog_artifact_for(
            "intel.xess",
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        );

        storage
            .upsert_artifacts(&[first, stale, unrelated.clone()])
            .expect("catalog registrations");
        storage
            .delete_catalog_package_artifacts("nvidia.dlss")
            .expect("delete logical package");

        assert_eq!(
            storage.list_artifacts().expect("artifact list"),
            vec![unrelated]
        );
    }

    #[test]
    fn artifact_batch_rolls_back_every_row_when_one_receipt_is_invalid() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let first = sample_artifact("artifact:first", "C:/cache/first.dll", "first.dll", HASH_A);
        let valid = catalog_artifact(HASH_B);
        let invalid = LibraryArtifact::new(
            ArtifactId::new("package:not-the-revision").expect("artifact id"),
            valid.technology(),
            valid.file_name(),
            valid.files().to_vec(),
            valid.trust_level(),
        )
        .expect("artifact")
        .with_metadata(valid.metadata().clone());

        storage
            .upsert_artifacts(&[first, invalid])
            .expect_err("whole batch must fail");
        assert!(
            storage.list_artifacts().expect("artifact list").is_empty(),
            "the first row must be rolled back with the invalid second row"
        );
    }

    #[test]
    fn list_artifacts_rejects_malformed_metadata_json() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let artifact = sample_artifact(
            "artifact:malformed-metadata",
            "C:/Games/GameA/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_A,
        );
        storage
            .upsert_artifact(&artifact)
            .expect("artifact should be stored");

        {
            let connection = storage.connection().expect("sqlite connection should lock");
            connection
                .execute_batch(
                    "PRAGMA ignore_check_constraints = ON;
                     UPDATE library_artifacts
                     SET metadata_json = '{broken'
                     WHERE id = 'artifact:malformed-metadata';
                     PRAGMA ignore_check_constraints = OFF;",
                )
                .expect("corrupt metadata fixture");
        }

        let error = storage
            .list_artifacts()
            .expect_err("malformed metadata must fail closed");
        assert!(error.to_string().contains("invalid sqlite row"));
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
            PathRef::new(format!("C:/Games/{}", id.replace([':', '/', '\\'], "_")))
                .expect("install path should be valid"),
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
            LibraryTechnology::DlssSuperResolution,
            file_name,
            vec![
                ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
                    .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid")),
            ],
            trust_level,
        )
        .expect("artifact should be valid")
        .with_source(match trust_level {
            ArtifactTrustLevel::CatalogDownloaded => "catalog-v1",
            _ => "scan-folder",
        })
        .expect("source should be valid")
    }

    fn catalog_artifact(revision: &str) -> LibraryArtifact {
        catalog_artifact_for("nvidia.dlss", revision)
    }

    fn catalog_artifact_for(package_id: &str, revision: &str) -> LibraryArtifact {
        let revision = Sha256Hash::new(revision).expect("revision");
        let receipt = CatalogPackageReceiptV1 {
            schema_version: CatalogReceiptSchemaV1,
            package_id: package_id.to_owned(),
            vendor: "nvidia".to_owned(),
            technology: "dlss_super_resolution".to_owned(),
            variant: "runtime".to_owned(),
            display_name: "NVIDIA DLSS".to_owned(),
            release: PackageRelease {
                version: PackageVersion::parse("3.10.0").expect("package version"),
                channel: ReleaseChannel::Stable,
                label: None,
            },
            target: CatalogTargetReceipt {
                os: "windows".to_owned(),
                architecture: Architecture::X64,
                compatibility: None,
            },
            provenance: None,
            revision_sha256: revision.clone(),
            primary_file_name: "nvngx_dlss.dll".to_owned(),
            primary_sha256: Sha256Hash::new(HASH_A).expect("member digest"),
            primary_signature: CatalogSignatureReceipt::Unsigned,
            legal_documents: Vec::new(),
            size_bytes: 1,
        };
        sample_artifact_with_trust(
            ArtifactId::for_package_revision(&revision).as_str(),
            "C:/cache/nvngx_dlss.dll",
            "nvngx_dlss.dll",
            HASH_A,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .with_metadata(ArtifactMetadata::default().with_catalog_package_receipt(receipt))
    }

    #[test]
    fn upsert_local_observed_does_not_overwrite_catalog_downloaded() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        // Catalog artifact pointing to the managed content store.
        let cache = sample_artifact_with_trust(
            "artifact:bundle",
            "C:/AppData/RenderPilot/libraries/fsr_upscaler_dx12/v1/amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_upscaler_dx12.dll",
            HASH_A,
            ArtifactTrustLevel::CatalogDownloaded,
        );

        storage
            .upsert_artifact(&cache)
            .expect("catalog artifact should be stored");

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
        // The catalog record must be preserved — its path points to the managed
        // content store and must not be replaced with the game-folder copy.
        assert_eq!(
            artifacts[0].path().as_str(),
            "C:/AppData/RenderPilot/libraries/fsr_upscaler_dx12/v1/amd_fidelityfx_upscaler_dx12.dll",
            "local-observed scan must not overwrite a catalog artifact's managed path"
        );
        assert_eq!(
            artifacts[0].trust_level(),
            ArtifactTrustLevel::CatalogDownloaded
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
            ArtifactTrustLevel::CatalogDownloaded,
        );

        for artifact in [&stale_a, &current_a, &other_b, &cached] {
            storage
                .upsert_artifact(artifact)
                .expect("artifact should store");
        }

        // Rescan game A with only current_a in the write unit (plus empty
        // components): prune runs inside save_scan_write_unit.
        let component = LibraryComponent::new(
            ComponentId::new("component:a:dlss").expect("component id"),
            game_a.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::DlssSuperResolution,
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
            "CatalogDownloaded must never be pruned by game scan"
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
    fn upsert_catalog_downloaded_overwrites_catalog_downloaded() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let first = sample_artifact_with_trust(
            "artifact:bundle",
            "C:/AppData/old/amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_upscaler_dx12.dll",
            HASH_A,
            ArtifactTrustLevel::CatalogDownloaded,
        );

        storage
            .upsert_artifact(&first)
            .expect("first artifact should be stored");

        let second = sample_artifact_with_trust(
            "artifact:bundle",
            "C:/AppData/new/amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_upscaler_dx12.dll",
            HASH_A,
            ArtifactTrustLevel::CatalogDownloaded,
        );

        storage
            .upsert_artifact(&second)
            .expect("second catalog artifact should be stored");

        let artifacts = storage.list_artifacts().expect("artifacts should load");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts[0].path().as_str(),
            "C:/AppData/new/amd_fidelityfx_upscaler_dx12.dll",
            "a catalog artifact can be updated by another catalog artifact"
        );
    }
}
