use renderpilot_domain::{ArtifactId, ComponentId, OperationId, PathRef};

use super::{MetadataJson, OperationStatus};

/// Stored operation item row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationItemRecord {
    /// Operation that owns this item.
    pub operation_id: OperationId,

    /// Component affected by this item.
    pub component_id: ComponentId,

    /// Artifact selected for this item, when applicable.
    pub artifact_id: Option<ArtifactId>,

    /// Source file path.
    pub source_path: PathRef,

    /// Target file path, when applicable.
    pub target_path: Option<PathRef>,

    /// Current item status.
    pub status: OperationStatus,

    /// Optional adapter-owned metadata JSON.
    pub metadata_json: Option<MetadataJson>,
}

impl OperationItemRecord {
    /// Creates a new operation item row.
    #[must_use]
    pub fn new(
        operation_id: OperationId,
        component_id: ComponentId,
        source_path: PathRef,
        status: OperationStatus,
    ) -> Self {
        Self {
            operation_id,
            component_id,
            artifact_id: None,
            source_path,
            target_path: None,
            status,
            metadata_json: None,
        }
    }

    /// Sets the selected artifact.
    #[must_use]
    pub fn with_artifact_id(mut self, artifact_id: ArtifactId) -> Self {
        self.artifact_id = Some(artifact_id);
        self
    }

    /// Sets the target file path.
    #[must_use]
    pub fn with_target_path(mut self, target_path: PathRef) -> Self {
        self.target_path = Some(target_path);
        self
    }

    /// Sets adapter-owned metadata JSON.
    #[must_use]
    pub fn with_metadata_json(mut self, metadata_json: impl Into<MetadataJson>) -> Self {
        self.metadata_json = Some(metadata_json.into());
        self
    }
}
