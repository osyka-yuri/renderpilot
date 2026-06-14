//! Best-effort recording of completed swap / rollback operations in the journal.

use renderpilot_domain::{fsr, ArtifactId, ComponentId, GameId, GraphicsComponent, PathRef};
use renderpilot_storage_sqlite::SqliteStorage;

use renderpilot_application::{
    GameRepository, MetadataJson, OperationItemRecord, OperationJournalEntry, OperationKind,
    OperationRecord, OperationRepository, OperationStatus, UnixTimestampMillis,
};

use super::types::OperationMetadata;

const UNKNOWN_GAME_NAME: &str = "Unknown Game";
const UNKNOWN_VERSION: &str = "Unknown";

/// A single file affected by the operation.
pub(super) struct JournalEntryItem<'a> {
    pub(super) path: &'a PathRef,
    pub(super) artifact_id: Option<ArtifactId>,
}

/// Parameters for recording a completed operation in the journal.
///
/// Passed as a single value to [`record_operation_journal_entry`] so that the
/// call sites remain readable without a 7-argument call.
pub(super) struct JournalEntryParams<'a> {
    pub(super) game_id: &'a GameId,
    pub(super) component_id: &'a ComponentId,
    pub(super) kind: OperationKind,
    pub(super) component: &'a GraphicsComponent,
    /// The version the component is being swapped to.
    /// `None` falls back to [`UNKNOWN_VERSION`] in the stored metadata.
    pub(super) to_version: Option<&'a str>,
    /// Files affected by the operation.
    pub(super) items: Vec<JournalEntryItem<'a>>,
}

/// Records a journal entry for the completed operation, best-effort.
///
/// Failures are logged as warnings and do **not** propagate — journal
/// persistence is informational and must never disrupt the main swap / rollback
/// flow.
pub(super) fn record_operation_journal_entry(
    storage: &SqliteStorage,
    params: JournalEntryParams<'_>,
) {
    let JournalEntryParams {
        game_id,
        component_id,
        kind,
        component,
        to_version,
        items,
    } = params;

    let Ok(op_id) = renderpilot_domain::OperationId::new(ulid::Ulid::new().to_string()) else {
        log::warn!("Failed to generate operation id for journal");
        return;
    };
    let timestamp = UnixTimestampMillis::now().unwrap_or(UnixTimestampMillis::EPOCH);

    let metadata_json = build_metadata_json(storage, game_id, component, to_version);

    let operation_record = OperationRecord::new(
        op_id.clone(),
        game_id.clone(),
        kind,
        OperationStatus::Completed,
        timestamp,
    )
    .with_completed_at(timestamp)
    .with_metadata_json(metadata_json);

    let item_records = build_item_records(&op_id, component_id, items);

    if let Ok(entry) = OperationJournalEntry::try_new(operation_record, item_records) {
        if let Err(e) = OperationRepository::save_operation_entry(storage, &entry) {
            log::warn!("Failed to save operation journal entry: {}", e);
        }
    }
}

/// Builds the serialized operation metadata, falling back to placeholders when
/// the game name or versions cannot be resolved.
fn build_metadata_json(
    storage: &SqliteStorage,
    game_id: &GameId,
    component: &GraphicsComponent,
    to_version: Option<&str>,
) -> MetadataJson {
    let game_name = storage
        .find_game(game_id)
        .ok()
        .flatten()
        .map(|g| g.identity().title().to_string())
        .unwrap_or_else(|| UNKNOWN_GAME_NAME.to_owned());

    let metadata = OperationMetadata {
        game_name,
        library: component.technology().as_slug().to_string(),
        from_version: fsr::version_representative(component.files())
            .and_then(|f| f.version())
            .map(|v| v.to_string()),
        to_version: to_version.unwrap_or(UNKNOWN_VERSION).to_owned(),
    };
    let metadata_str = serde_json::to_string(&metadata).unwrap_or_else(|_| "{}".to_owned());
    MetadataJson::new(metadata_str).unwrap_or_default()
}

/// Builds an operation item record per affected file.
fn build_item_records(
    op_id: &renderpilot_domain::OperationId,
    component_id: &ComponentId,
    items: Vec<JournalEntryItem<'_>>,
) -> Vec<OperationItemRecord> {
    items
        .into_iter()
        .map(|item| {
            let mut record = OperationItemRecord::new(
                op_id.clone(),
                component_id.clone(),
                item.path.clone(),
                OperationStatus::Completed,
            );
            if let Some(aid) = item.artifact_id {
                record = record.with_artifact_id(aid);
            }
            record
        })
        .collect()
}
