//! Persistence for advisory shared artifact provenance.
//!
//! Shared artifacts are reconciled from platform/filesystem facts first. This
//! table only accelerates update checks and preserves audit metadata.

use renderpilot_application::{AppResult, SharedArtifactRepository};
use renderpilot_domain::{
    PathRef, SharedArtifactKind, SharedArtifactOrigin, SharedArtifactRecord, SharedArtifactSource,
};
use rusqlite::{OptionalExtension, Row, Transaction, named_params};

use crate::error::{invalid_row, storage_error};
use crate::{mapping, sqlite_clock};

use super::SqliteStorage;

/// Result of a non-owning advisory write at the shared-mutation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalSharedArtifactWrite {
    /// The advisory row was written.
    Applied,
    /// A durable shared mutation owns the singleton, so nothing was written.
    Deferred,
}

const UPSERT_SQL: &str = "
    INSERT INTO shared_artifacts
        (kind, install_dir, manifest_path, dll_path,
         source_url, source_etag, source_digest, source_last_modified,
         channel, origin, created_files_json, created_at, updated_at)
    VALUES
        (:kind, :install_dir, :manifest_path, :dll_path,
         :source_url, :source_etag, :source_digest, :source_last_modified,
         :channel, :origin, :created_files, :now_ms, :now_ms)
    ON CONFLICT(kind) DO UPDATE SET
        install_dir          = excluded.install_dir,
        manifest_path        = excluded.manifest_path,
        dll_path             = excluded.dll_path,
        source_url           = excluded.source_url,
        source_etag          = excluded.source_etag,
        source_digest        = excluded.source_digest,
        source_last_modified = excluded.source_last_modified,
        channel              = excluded.channel,
        origin               = excluded.origin,
        created_files_json   = excluded.created_files_json,
        updated_at           = excluded.updated_at
";

const GET_SQL: &str = "
    SELECT kind, install_dir, manifest_path, dll_path,
           source_url, source_etag, source_digest, source_last_modified,
           channel, origin, created_files_json, created_at, updated_at
    FROM shared_artifacts
    WHERE kind = :kind
";

impl SharedArtifactRepository for SqliteStorage {
    fn upsert_shared_artifact(&self, record: &SharedArtifactRecord) -> AppResult<()> {
        self.with_transaction(|transaction| upsert_within_transaction(transaction, record))
    }

    fn get_shared_artifact(
        &self,
        kind: SharedArtifactKind,
    ) -> AppResult<Option<SharedArtifactRecord>> {
        let kind_text = mapping::enum_to_text(&kind)?;
        self.with_connection(|connection| {
            connection
                .prepare_cached(GET_SQL)
                .map_err(storage_error)?
                .query_row(named_params! { ":kind": kind_text }, |row| {
                    Ok(row_to_shared_artifact(row))
                })
                .optional()
                .map_err(storage_error)?
                .transpose()
        })
    }

    fn delete_shared_artifact(&self, kind: SharedArtifactKind) -> AppResult<()> {
        self.with_transaction(|transaction| delete_within_transaction(transaction, kind))
    }
}

impl SqliteStorage {
    /// Records advisory provenance only when no durable shared mutation owns
    /// the singleton. The pending check and upsert share one immediate SQLite
    /// transaction, so a reservation cannot appear between them.
    pub fn try_upsert_shared_artifact_if_unreserved(
        &self,
        record: &SharedArtifactRecord,
    ) -> AppResult<ConditionalSharedArtifactWrite> {
        self.with_immediate_transaction(|transaction| {
            let reserved = transaction
                .query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM pending_shared_vulkan_mutations
                         WHERE resource_key = ?1
                    )",
                    [super::pending_shared_vulkan_mutations::RESOURCE_KEY],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(storage_error)?;
            if reserved {
                return Ok(ConditionalSharedArtifactWrite::Deferred);
            }
            upsert_within_transaction(transaction, record)?;
            Ok(ConditionalSharedArtifactWrite::Applied)
        })
    }
}

/// Reusable transaction-local shared artifact upsert.
pub(super) fn upsert_within_transaction(
    transaction: &Transaction<'_>,
    record: &SharedArtifactRecord,
) -> AppResult<()> {
    let now_ms = sqlite_clock::now_ms(transaction)?;
    transaction
        .prepare_cached(UPSERT_SQL)
        .map_err(storage_error)?
        .execute(named_params! {
            ":kind": mapping::enum_to_text(&record.kind())?,
            ":install_dir": record.install_dir().as_str(),
            ":manifest_path": record.manifest_path().as_str(),
            ":dll_path": record.dll_path().as_str(),
            ":source_url": record.source_url(),
            ":source_etag": record.source_etag(),
            ":source_digest": record.source_digest(),
            ":source_last_modified": record.source_last_modified(),
            ":channel": record.channel(),
            ":origin": mapping::enum_to_text(&record.origin())?,
            ":created_files": mapping::serialize_json(record.created_files())?,
            ":now_ms": now_ms,
        })
        .map_err(storage_error)?;
    Ok(())
}

/// Reusable transaction-local shared artifact delete.
pub(super) fn delete_within_transaction(
    transaction: &Transaction<'_>,
    kind: SharedArtifactKind,
) -> AppResult<()> {
    let kind_text = mapping::enum_to_text(&kind)?;
    transaction
        .prepare_cached("DELETE FROM shared_artifacts WHERE kind = :kind")
        .map_err(storage_error)?
        .execute(named_params! { ":kind": kind_text })
        .map_err(storage_error)?;
    Ok(())
}

fn row_to_shared_artifact(row: &Row<'_>) -> AppResult<SharedArtifactRecord> {
    let kind: SharedArtifactKind =
        mapping::enum_from_text(&row.get::<_, String>("kind").map_err(storage_error)?)?;
    let install_dir = PathRef::new(row.get::<_, String>("install_dir").map_err(storage_error)?)
        .map_err(invalid_row)?;
    let manifest_path = PathRef::new(
        row.get::<_, String>("manifest_path")
            .map_err(storage_error)?,
    )
    .map_err(invalid_row)?;
    let dll_path = PathRef::new(row.get::<_, String>("dll_path").map_err(storage_error)?)
        .map_err(invalid_row)?;
    let source = SharedArtifactSource {
        url: row.get("source_url").map_err(storage_error)?,
        etag: row.get("source_etag").map_err(storage_error)?,
        digest: row.get("source_digest").map_err(storage_error)?,
        last_modified: row.get("source_last_modified").map_err(storage_error)?,
        channel: row.get("channel").map_err(storage_error)?,
    };
    let origin: SharedArtifactOrigin =
        mapping::enum_from_text(&row.get::<_, String>("origin").map_err(storage_error)?)?;
    let created_files: Vec<PathRef> = mapping::deserialize_json(
        &row.get::<_, String>("created_files_json")
            .map_err(storage_error)?,
    )?;
    let created_at: i64 = row.get("created_at").map_err(storage_error)?;
    let updated_at: i64 = row.get("updated_at").map_err(storage_error)?;

    Ok(SharedArtifactRecord::from_parts(
        kind,
        install_dir,
        manifest_path,
        dll_path,
        source,
        origin,
        created_files,
    )
    .with_timestamps(Some(created_at), Some(updated_at)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> PathRef {
        PathRef::new(value).expect("path")
    }

    fn record() -> SharedArtifactRecord {
        SharedArtifactRecord::new(
            SharedArtifactKind::RenoDxVulkanLayer,
            path("C:/ProgramData/ReShade"),
            path("C:/ProgramData/ReShade/ReShade64.json"),
            path("C:/ProgramData/ReShade/ReShade64.dll"),
            SharedArtifactOrigin::RenderPilotCreated,
        )
        .with_source(SharedArtifactSource::known(
            "https://nightly.link/reshade/x64.zip",
            Some("\"etag\"".to_owned()),
            "a".repeat(64),
            Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned()),
            "nightly",
        ))
        .with_created_files(vec![
            path("C:/ProgramData/ReShade/ReShade64.dll"),
            path("C:/ProgramData/ReShade/ReShade64.json"),
        ])
    }

    #[test]
    fn upsert_then_get_round_trips_shared_artifact() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let record = record();

        storage
            .upsert_shared_artifact(&record)
            .expect("upsert shared artifact");
        let loaded = storage
            .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
            .expect("get shared artifact")
            .expect("record exists");

        assert_eq!(loaded.kind(), SharedArtifactKind::RenoDxVulkanLayer);
        assert_eq!(loaded.install_dir(), record.install_dir());
        assert_eq!(loaded.manifest_path(), record.manifest_path());
        assert_eq!(loaded.dll_path(), record.dll_path());
        assert_eq!(loaded.source_url(), record.source_url());
        assert_eq!(loaded.source_digest(), record.source_digest());
        assert_eq!(loaded.channel(), record.channel());
        assert_eq!(loaded.origin(), record.origin());
        assert_eq!(loaded.created_files(), record.created_files());
        assert!(loaded.installed_at().is_some());
        assert!(loaded.updated_at().is_some());
    }

    #[test]
    fn get_returns_none_when_absent() {
        let storage = SqliteStorage::in_memory().expect("storage");

        assert!(
            storage
                .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
                .expect("get shared artifact")
                .is_none()
        );
    }

    #[test]
    fn repeated_upsert_preserves_created_at_and_advances_updated_at() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let first = record();
        storage
            .upsert_shared_artifact(&first)
            .expect("initial upsert");
        let loaded_first = storage
            .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
            .expect("get first")
            .expect("first record exists");

        let second = SharedArtifactRecord::new(
            SharedArtifactKind::RenoDxVulkanLayer,
            path("C:/ProgramData/ReShade"),
            path("C:/ProgramData/ReShade/ReShade64.json"),
            path("C:/ProgramData/ReShade/ReShade64.dll"),
            SharedArtifactOrigin::AdoptedOfficial,
        )
        .with_source(SharedArtifactSource {
            url: None,
            etag: None,
            digest: None,
            last_modified: None,
            channel: Some("stable".to_owned()),
        });
        storage
            .upsert_shared_artifact(&second)
            .expect("second upsert");
        let loaded_second = storage
            .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
            .expect("get second")
            .expect("second record exists");

        assert_eq!(loaded_second.installed_at(), loaded_first.installed_at());
        assert!(
            loaded_second.updated_at() > loaded_first.updated_at(),
            "updated_at should advance on upsert"
        );
        assert_eq!(loaded_second.source_url(), None);
        assert_eq!(loaded_second.channel(), Some("stable"));
        assert_eq!(
            loaded_second.origin(),
            SharedArtifactOrigin::AdoptedOfficial
        );
    }

    #[test]
    fn delete_removes_shared_artifact() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .upsert_shared_artifact(&record())
            .expect("upsert shared artifact");

        storage
            .delete_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
            .expect("delete shared artifact");

        assert!(
            storage
                .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
                .expect("get shared artifact")
                .is_none()
        );
    }

    #[test]
    fn conditional_upsert_applies_without_a_shared_reservation() {
        let storage = SqliteStorage::in_memory().expect("storage");
        assert_eq!(
            storage
                .try_upsert_shared_artifact_if_unreserved(&record())
                .expect("conditional upsert"),
            ConditionalSharedArtifactWrite::Applied
        );
        assert!(
            storage
                .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
                .expect("artifact")
                .is_some()
        );
    }

    #[test]
    fn conditional_upsert_defers_while_shared_mutation_is_reserved() {
        let storage = SqliteStorage::in_memory().expect("storage");
        storage
            .try_begin_shared_vulkan_mutation(
                &super::super::pending_shared_vulkan_mutations::BeginSharedVulkanMutation {
                    id: "conditional-adoption-fence".to_owned(),
                    scope: super::super::pending_shared_vulkan_mutations::SharedVulkanMutationScope::SharedOnly,
                    game_id: None,
                    feature: "test".to_owned(),
                    initial_manifest_json: "{}".to_owned(),
                    root_capabilities_json: "{}".to_owned(),
                },
            )
            .expect("shared reservation");

        assert_eq!(
            storage
                .try_upsert_shared_artifact_if_unreserved(&record())
                .expect("conditional upsert"),
            ConditionalSharedArtifactWrite::Deferred
        );
        assert!(
            storage
                .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
                .expect("artifact")
                .is_none()
        );
    }
}
