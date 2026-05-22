use renderpilot_domain::{GameId, OperationId};

use super::{MetadataJson, OperationKind, OperationStatus, UnixTimestampMillis};

/// Stored operation journal record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRecord {
    /// Stable operation identifier.
    pub id: OperationId,

    /// Game affected by this operation.
    pub game_id: GameId,

    /// Operation kind.
    pub kind: OperationKind,

    /// Current operation status.
    pub status: OperationStatus,

    /// Creation timestamp.
    pub created_at: UnixTimestampMillis,

    /// Completion timestamp, when known.
    pub completed_at: Option<UnixTimestampMillis>,

    /// Optional metadata JSON. Business-level semantics should use typed wrappers.
    pub metadata_json: Option<MetadataJson>,
}

impl OperationRecord {
    /// Creates a new operation journal record.
    #[must_use]
    pub fn new(
        id: OperationId,
        game_id: GameId,
        kind: OperationKind,
        status: OperationStatus,
        created_at: UnixTimestampMillis,
    ) -> Self {
        Self {
            id,
            game_id,
            kind,
            status,
            created_at,
            completed_at: None,
            metadata_json: None,
        }
    }

    /// Sets the completion timestamp.
    #[must_use]
    pub fn with_completed_at(mut self, completed_at: UnixTimestampMillis) -> Self {
        self.completed_at = Some(completed_at);
        self
    }

    /// Sets the record status and completion timestamp together.
    #[must_use]
    pub fn with_state(
        mut self,
        status: OperationStatus,
        completed_at: Option<UnixTimestampMillis>,
    ) -> Self {
        self.status = status;
        self.completed_at = completed_at;
        self
    }

    /// Sets metadata JSON for this record.
    #[must_use]
    pub fn with_metadata_json(mut self, metadata_json: impl Into<MetadataJson>) -> Self {
        self.metadata_json = Some(metadata_json.into());
        self
    }
}
