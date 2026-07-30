use renderpilot_orchestration::application::{
    OperationItemRecord, OperationJournalEntry, OperationKind, OperationRecord,
    OperationRepository, OperationStatus, UnixTimestampMillis,
};
use renderpilot_orchestration::domain::{
    ComponentId, LibraryTechnology, OperationId, PathRef, Swappability,
};

use super::super::{CatalogFixture, args, sample_component, sample_game};

#[test]
fn list_operations_renders_item_counts_from_aggregate_entries() {
    let fixture = CatalogFixture::new("list-operations");
    let game = sample_game("manual:C:/Games/GameA", "Game A", "C:/Games/GameA");

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[
            sample_component(
                "component:game-a:dlss",
                game.id().as_str(),
                LibraryTechnology::DlssSuperResolution,
                Swappability::Swappable,
                "C:/Games/GameA/nvngx_dlss.dll",
                Some("3.5.0"),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            sample_component(
                "component:game-a:fg",
                game.id().as_str(),
                LibraryTechnology::DlssFrameGeneration,
                Swappability::Swappable,
                "C:/Games/GameA/nvngx_dlssg.dll",
                Some("3.5.0"),
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
        ],
    );

    let operation_id =
        OperationId::new("operation:replace_component:list").expect("operation id should be valid");
    let entry = OperationJournalEntry::try_new(
        OperationRecord::new(
            operation_id.clone(),
            game.id().clone(),
            OperationKind::ReplaceComponent,
            OperationStatus::Completed,
            UnixTimestampMillis::new(10).expect("timestamp should be valid"),
        ),
        vec![
            OperationItemRecord::new(
                operation_id.clone(),
                ComponentId::new("component:game-a:dlss").expect("component id should be valid"),
                PathRef::new("C:/Games/GameA/nvngx_dlss.dll").expect("path should be valid"),
                OperationStatus::Completed,
            ),
            OperationItemRecord::new(
                operation_id,
                ComponentId::new("component:game-a:fg").expect("component id should be valid"),
                PathRef::new("C:/Games/GameA/nvngx_dlssg.dll").expect("path should be valid"),
                OperationStatus::Completed,
            ),
        ],
    )
    .expect("journal entry should be valid");
    fixture
        .storage()
        .save_operation_entry(&entry)
        .expect("journal entry should be stored");

    let output = fixture
        .run(args(&["list-operations", "--game", game.id().as_str()]))
        .expect("list operations should succeed");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
    let operations = json["operations"]
        .as_array()
        .expect("operations array should be present");

    assert_eq!(json["game_id"], game.id().as_str());
    assert_eq!(operations.len(), 1);
    assert_eq!(
        operations[0]["operation_id"],
        "operation:replace_component:list"
    );
    assert_eq!(operations[0]["item_count"], 2);
    assert_eq!(operations[0]["component_id"], "component:game-a:dlss");
}
