//! Package metadata, trust normalization, and removal of obsolete manifest rows.

use renderpilot_application::AppResult;
use rusqlite::Connection;

use super::super::version;
use super::util::{ensure_column, table_has_column};

pub(super) fn apply(connection: &Connection) -> AppResult<()> {
    ensure_column(
        connection,
        "library_artifacts",
        "metadata_json",
        "ALTER TABLE library_artifacts ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}' \
         CHECK (json_valid(metadata_json)) CHECK (json_type(metadata_json) = 'object')",
    )?;
    normalize_artifact_trust_levels(connection)?;
    remove_legacy_manifest_artifacts(connection)?;
    version::write(connection, 11)
}

fn normalize_artifact_trust_levels(connection: &Connection) -> AppResult<()> {
    if !table_has_column(connection, "library_artifacts", "trust_level")? {
        return Ok(());
    }

    connection
        .execute_batch(
            "UPDATE library_artifacts
             SET trust_level = CASE trust_level
                 WHEN 'LocalObserved' THEN 'local_observed'
                 WHEN 'UserImported' THEN 'user_imported'
                 WHEN 'ManifestDownloaded' THEN 'catalog_downloaded'
                 WHEN 'CatalogDownloaded' THEN 'catalog_downloaded'
                 WHEN 'Unknown' THEN 'unknown'
                 ELSE trust_level
             END
             WHERE trust_level IN (
                 'LocalObserved',
                 'UserImported',
                 'ManifestDownloaded',
                 'CatalogDownloaded',
                 'Unknown'
             )",
        )
        .map_err(|error| {
            crate::error::storage_context("could not normalize artifact trust levels", error)
        })
}

fn remove_legacy_manifest_artifacts(connection: &Connection) -> AppResult<()> {
    if !table_has_column(connection, "library_artifacts", "trust_level")?
        || !table_has_column(connection, "library_artifacts", "source")?
    {
        return Ok(());
    }

    connection
        .execute(
            "DELETE FROM library_artifacts
             WHERE trust_level = 'catalog_downloaded'
               AND (source IS NULL OR source != 'catalog-v1')",
            [],
        )
        .map(|_| ())
        .map_err(|error| {
            crate::error::storage_context(
                "could not remove legacy manifest artifact registrations",
                error,
            )
        })
}
