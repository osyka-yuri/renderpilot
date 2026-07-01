use renderpilot_domain::{SharedArtifactKind, SharedArtifactRecord};

use crate::AppResult;

/// Repository port for advisory shared artifact provenance.
///
/// Shared artifacts are global resources (for example the ReShade Vulkan layer)
/// rather than per-game installs. Records are useful for update checks and audit,
/// but callers must reconcile against filesystem/platform facts when a row is
/// missing or stale.
pub trait SharedArtifactRepository: Send + Sync {
    /// Inserts or replaces the shared artifact record.
    fn upsert_shared_artifact(&self, record: &SharedArtifactRecord) -> AppResult<()>;

    /// Returns the shared artifact record for `kind`, if one is recorded.
    fn get_shared_artifact(
        &self,
        kind: SharedArtifactKind,
    ) -> AppResult<Option<SharedArtifactRecord>>;

    /// Deletes the shared artifact record for `kind`. Missing rows are a no-op.
    fn delete_shared_artifact(&self, kind: SharedArtifactKind) -> AppResult<()>;
}
