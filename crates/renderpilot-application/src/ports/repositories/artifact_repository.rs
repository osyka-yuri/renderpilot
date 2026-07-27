use renderpilot_domain::LibraryArtifact;

use crate::AppResult;

/// Repository port for storing downloadable or local replacement artifacts.
pub trait ArtifactRepository: Send + Sync {
    /// Inserts or updates one library artifact.
    fn upsert_artifact(&self, artifact: &LibraryArtifact) -> AppResult<()>;

    /// Atomically replaces the catalog registration for one logical package.
    ///
    /// Content-addressed files are not removed. Only persisted artifact rows
    /// carrying a receipt for `package_id` are replaced.
    fn replace_catalog_package_artifact(
        &self,
        package_id: &str,
        artifact: &LibraryArtifact,
    ) -> AppResult<()>;

    /// Deletes every persisted artifact registration for one logical package.
    ///
    /// Content-addressed files are owned by the library cache and are not
    /// removed by this repository operation.
    fn delete_catalog_package_artifacts(&self, package_id: &str) -> AppResult<()>;

    /// Atomically inserts or updates a complete artifact batch.
    ///
    /// Implementations must leave every row unchanged if any item fails.
    fn upsert_artifacts(&self, artifacts: &[LibraryArtifact]) -> AppResult<()>;

    /// Lists all known library artifacts.
    fn list_artifacts(&self) -> AppResult<Vec<LibraryArtifact>>;

    /// Deletes an artifact by its ID.
    fn delete_artifact(&self, id: &renderpilot_domain::ArtifactId) -> AppResult<()>;
}
