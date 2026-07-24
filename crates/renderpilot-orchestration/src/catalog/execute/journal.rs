//! Best-effort recording of completed swap / rollback operations in the journal.

use renderpilot_domain::{
    ArtifactId, ComponentId, GameId, GraphicsComponent, PathRef, component_version_report,
};
use renderpilot_storage_sqlite::SqliteStorage;

use renderpilot_application::{
    AppResult, D3d12ExecutableAction, GameRepository, MetadataJson, OperationItemRecord,
    OperationJournalEntry, OperationKind, OperationRecord, OperationRepository, OperationStatus,
    UnixTimestampMillis,
};
use serde::{Deserialize, Serialize};

use super::types::{D3d12ExecutableActionResult, OperationMetadata};

const UNKNOWN_GAME_NAME: &str = "Unknown Game";
const UNKNOWN_VERSION: &str = "Unknown";

/// A single file affected by the operation.
pub(crate) struct JournalEntryItem<'a> {
    path: &'a PathRef,
    artifact_id: Option<ArtifactId>,
    metadata: Option<JournalItemMetadata>,
}

impl<'a> JournalEntryItem<'a> {
    /// Records one DLL/package member. These items retain the historical public count.
    pub(crate) fn component_file(path: &'a PathRef, artifact_id: Option<ArtifactId>) -> Self {
        Self {
            path,
            artifact_id,
            metadata: None,
        }
    }

    /// Records an EXE transition without changing the historical DLL item count.
    pub(crate) fn d3d12_executable(action: &'a D3d12ExecutableAction) -> Self {
        Self {
            path: action.executable_path(),
            artifact_id: None,
            metadata: Some(JournalItemMetadata::D3d12Executable {
                from_sdk_version: action.current_sdk_version(),
                to_sdk_version: action.target_sdk_version(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JournalItemMetadata {
    D3d12Executable {
        from_sdk_version: u32,
        to_sdk_version: u32,
    },
}

/// Parameters for recording a completed operation in the journal.
///
/// Passed as a single value to [`record_operation_journal_entry`] so that the
/// call sites remain readable without a 7-argument call.
pub(crate) struct JournalEntryParams<'a> {
    pub(crate) game_id: &'a GameId,
    pub(crate) component_id: &'a ComponentId,
    pub(crate) kind: OperationKind,
    pub(crate) component: &'a GraphicsComponent,
    /// The version the component is being swapped to.
    /// `None` falls back to [`UNKNOWN_VERSION`] in the stored metadata.
    pub(crate) to_version: Option<&'a str>,
    /// Files affected by the operation.
    pub(crate) items: Vec<JournalEntryItem<'a>>,
    pub(crate) d3d12_executable_action: Option<D3d12ExecutableActionResult>,
}

/// Records a journal entry for the completed operation, best-effort.
///
/// Failures are logged as warnings and do **not** propagate — journal
/// persistence is informational and must never disrupt the main swap / rollback
/// flow.
pub(crate) fn record_operation_journal_entry(
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
        d3d12_executable_action,
    } = params;

    let Ok(op_id) = renderpilot_domain::OperationId::new(ulid::Ulid::generate().to_string()) else {
        log::warn!("Failed to generate operation id for journal");
        return;
    };
    let timestamp = UnixTimestampMillis::now().unwrap_or(UnixTimestampMillis::EPOCH);

    let metadata_json = match build_metadata_json(
        storage,
        game_id,
        component,
        to_version,
        d3d12_executable_action,
    ) {
        Ok(metadata) => metadata,
        Err(error) => {
            log::warn!("Failed to build operation journal metadata: {error}");
            return;
        }
    };

    let operation_record = OperationRecord::new(
        op_id.clone(),
        game_id.clone(),
        kind,
        OperationStatus::Completed,
        timestamp,
    )
    .with_completed_at(timestamp)
    .with_metadata_json(metadata_json);

    let item_records = match build_item_records(&op_id, component_id, items) {
        Ok(items) => items,
        Err(error) => {
            log::warn!("Failed to build operation journal items: {error}");
            return;
        }
    };

    if let Ok(entry) = OperationJournalEntry::try_new(operation_record, item_records)
        && let Err(e) = OperationRepository::save_operation_entry(storage, &entry)
    {
        log::warn!("Failed to save operation journal entry: {}", e);
    }
}

/// Builds the serialized operation metadata, falling back to placeholders when
/// the game name or versions cannot be resolved.
fn build_metadata_json(
    storage: &SqliteStorage,
    game_id: &GameId,
    component: &GraphicsComponent,
    to_version: Option<&str>,
    d3d12_executable_action: Option<D3d12ExecutableActionResult>,
) -> AppResult<MetadataJson> {
    let game_name = storage
        .find_game(game_id)
        .ok()
        .flatten()
        .map(|g| g.identity().title().to_string())
        .unwrap_or_else(|| UNKNOWN_GAME_NAME.to_owned());

    let metadata = OperationMetadata {
        game_name,
        library: component.technology().as_slug().to_string(),
        from_version: component_version_report(component.files(), component.technology())
            .known_version()
            .map(ToString::to_string),
        to_version: to_version.unwrap_or(UNKNOWN_VERSION).to_owned(),
        d3d12_executable_action,
    };
    let metadata_str = serde_json::to_string(&metadata).map_err(|error| {
        renderpilot_application::AppError::invalid_input(format!(
            "cannot serialize operation journal metadata: {error}"
        ))
    })?;
    MetadataJson::new(metadata_str)
}

/// Builds an operation item record per affected file.
fn build_item_records(
    op_id: &renderpilot_domain::OperationId,
    component_id: &ComponentId,
    items: Vec<JournalEntryItem<'_>>,
) -> AppResult<Vec<OperationItemRecord>> {
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
            if let Some(metadata) = item.metadata {
                record = record.with_metadata_json(MetadataJson::new(
                    serde_json::to_string(&metadata).map_err(|error| {
                        renderpilot_application::AppError::invalid_input(format!(
                            "cannot serialize journal item metadata: {error}"
                        ))
                    })?,
                )?);
            }
            Ok(record)
        })
        .collect()
}

/// Counts only component DLL/package members, preserving the pre-EXE public contract.
pub(crate) fn component_file_item_count(items: &[OperationItemRecord]) -> usize {
    items
        .iter()
        .filter(|item| journal_item_is_component_file(item))
        .count()
}

/// Unknown/legacy metadata remains a component file; only our typed EXE marker is excluded.
pub(crate) fn journal_item_is_component_file(item: &OperationItemRecord) -> bool {
    !item.metadata_json.as_ref().is_some_and(|metadata| {
        matches!(
            serde_json::from_str::<JournalItemMetadata>(metadata.as_str()),
            Ok(JournalItemMetadata::D3d12Executable { .. })
        )
    })
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{D3d12ExecutableAction, D3d12ExecutableProfile};
    use renderpilot_domain::{ComponentId, OperationId, PathRef};

    use super::{
        JournalEntryItem, build_item_records, component_file_item_count,
        journal_item_is_component_file,
    };

    #[test]
    fn executable_item_is_typed_without_changing_the_component_file_count() {
        let operation_id = OperationId::new("operation:journal-exe").expect("operation");
        let component_id = ComponentId::new("component:d3d12").expect("component");
        let dll_path = PathRef::new("C:/Game/D3D12Core.dll").expect("DLL path");
        let context = D3d12ExecutableProfile::new(
            PathRef::new("C:/Game/game.exe").expect("EXE path"),
            PathRef::new("C:/Game/game.exe.bak").expect("backup path"),
            606,
            606,
            false,
            false,
        );
        let action = D3d12ExecutableAction::for_swap(&context, 619).expect("patch assessment");

        let records = build_item_records(
            &operation_id,
            &component_id,
            vec![
                JournalEntryItem::component_file(&dll_path, None),
                JournalEntryItem::d3d12_executable(&action),
            ],
        )
        .expect("journal items");

        assert_eq!(records.len(), 2, "the EXE remains independently auditable");
        assert_eq!(
            component_file_item_count(&records),
            1,
            "the public count remains the historical DLL/package-member count"
        );
        assert!(journal_item_is_component_file(&records[0]));
        assert!(!journal_item_is_component_file(&records[1]));
        assert_eq!(
            records[1]
                .metadata_json
                .as_ref()
                .map(|metadata| metadata.as_str()),
            Some(r#"{"kind":"d3d12_executable","from_sdk_version":606,"to_sdk_version":619}"#)
        );
    }

    #[test]
    fn unknown_metadata_remains_backward_compatible_as_a_component_file() {
        let record = renderpilot_application::OperationItemRecord::new(
            OperationId::new("operation:legacy-item").expect("operation"),
            ComponentId::new("component:legacy").expect("component"),
            PathRef::new("C:/Game/legacy.dll").expect("path"),
            renderpilot_application::OperationStatus::Completed,
        )
        .with_metadata_json(
            renderpilot_application::MetadataJson::new(r#"{"legacy":true}"#).expect("metadata"),
        );

        assert!(journal_item_is_component_file(&record));
        assert_eq!(component_file_item_count(&[record]), 1);
    }
}
