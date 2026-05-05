use renderpilot_domain::{ComponentId, GameId, OperationId, PathRef, Sha256Hash};

use super::{BackupId, MetadataJson, UnixTimestampMillis};

/// Stored backup record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupRecord {
    /// Stable backup identifier.
    pub id: BackupId,

    /// Operation that created this backup.
    pub operation_id: OperationId,

    /// Game affected by this backup.
    pub game_id: GameId,

    /// Component backed up by this row, when known.
    pub component_id: Option<ComponentId>,

    /// Original file path.
    pub original_path: PathRef,

    /// Backup file path.
    pub backup_path: PathRef,

    /// SHA-256 of the backed-up bytes, when available.
    pub sha256: Option<Sha256Hash>,

    /// Creation timestamp.
    pub created_at: UnixTimestampMillis,

    /// Optional adapter-owned metadata JSON.
    pub metadata_json: Option<MetadataJson>,
}

impl BackupRecord {
    /// Creates a new backup record.
    #[must_use]
    pub fn new(
        id: BackupId,
        operation_id: OperationId,
        game_id: GameId,
        original_path: PathRef,
        backup_path: PathRef,
        created_at: UnixTimestampMillis,
    ) -> Self {
        Self {
            id,
            operation_id,
            game_id,
            component_id: None,
            original_path,
            backup_path,
            sha256: None,
            created_at,
            metadata_json: None,
        }
    }

    /// Sets the component backed up by this row.
    #[must_use]
    pub fn with_component_id(mut self, component_id: ComponentId) -> Self {
        self.component_id = Some(component_id);
        self
    }

    /// Sets the SHA-256 hash of the backed-up bytes.
    #[must_use]
    pub fn with_sha256(mut self, sha256: Sha256Hash) -> Self {
        self.sha256 = Some(sha256);
        self
    }

    /// Sets adapter-owned metadata JSON.
    #[must_use]
    pub fn with_metadata_json(mut self, metadata_json: impl Into<MetadataJson>) -> Self {
        self.metadata_json = Some(metadata_json.into());
        self
    }
}
