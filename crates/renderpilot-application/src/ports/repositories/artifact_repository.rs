use renderpilot_domain::LibraryArtifact;

use crate::AppResult;

/// Repository port for storing downloadable or local replacement artifacts.
pub trait ArtifactRepository: Send + Sync {
    /// Inserts or updates one library artifact.
    fn upsert_artifact(&self, artifact: &LibraryArtifact) -> AppResult<()>;

    /// Lists all known library artifacts.
    fn list_artifacts(&self) -> AppResult<Vec<LibraryArtifact>>;

    /// Deletes an artifact by its ID.
    fn delete_artifact(&self, id: &renderpilot_domain::ArtifactId) -> AppResult<()>;
}
