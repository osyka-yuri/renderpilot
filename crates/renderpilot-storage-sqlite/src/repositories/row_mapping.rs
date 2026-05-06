use renderpilot_application::{
    AppResult, BackupId, BackupRecord, OperationItemRecord, OperationKind, OperationRecord,
    OperationStatus, UnixTimestampMillis,
};
use renderpilot_domain::{
    ComponentFile, GameIdentity, GameInstallation, GraphicsComponent, LibraryArtifact, PathRef,
};

use crate::{
    error::{invalid_row, storage_error},
    mapping,
};

pub(super) fn game_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<GameInstallation>> {
    let id: String = row.get(0)?;
    let title: String = row.get(1)?;
    let launcher: String = row.get(2)?;
    let external_id: Option<String> = row.get(3)?;
    let platform: String = row.get(4)?;
    let runtime: String = row.get(5)?;
    let install_path: String = row.get(6)?;
    let executable_candidates_json: String = row.get(7)?;

    Ok(build_game(GameRow {
        id,
        title,
        launcher,
        external_id,
        platform,
        runtime,
        install_path,
        executable_candidates_json,
    }))
}

pub(super) fn component_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<GraphicsComponent>> {
    let id: String = row.get(0)?;
    let game_id: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let technology: String = row.get(3)?;
    let swappability: String = row.get(4)?;
    let files_json: String = row.get(5)?;

    Ok(build_component(
        id,
        game_id,
        kind,
        technology,
        swappability,
        files_json,
    ))
}

pub(super) fn artifact_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<LibraryArtifact>> {
    Ok(build_artifact(ArtifactRow {
        id: row.get(0)?,
        technology: row.get(1)?,
        file_name: row.get(2)?,
        file_path: row.get(3)?,
        version: row.get(4)?,
        sha256: row.get(5)?,
        source: row.get(6)?,
        source_game_id: row.get(7)?,
        trust_level: row.get(8)?,
    }))
}

pub(super) fn operation_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<OperationRecord>> {
    let id: String = row.get(0)?;
    let game_id: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let status: String = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let completed_at: Option<i64> = row.get(5)?;
    let metadata_json: Option<String> = row.get(6)?;

    Ok(build_operation(
        id,
        game_id,
        kind,
        status,
        created_at,
        completed_at,
        metadata_json,
    ))
}

pub(super) fn operation_item_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<OperationItemRecord>> {
    let operation_id: String = row.get(0)?;
    let component_id: String = row.get(1)?;
    let artifact_id: Option<String> = row.get(2)?;
    let source_path: String = row.get(3)?;
    let target_path: Option<String> = row.get(4)?;
    let status: String = row.get(5)?;
    let metadata_json: Option<String> = row.get(6)?;

    Ok(build_operation_item(
        operation_id,
        component_id,
        artifact_id,
        source_path,
        target_path,
        status,
        metadata_json,
    ))
}

pub(super) fn backup_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AppResult<BackupRecord>> {
    Ok(build_backup(BackupRow {
        id: row.get(0)?,
        operation_id: row.get(1)?,
        game_id: row.get(2)?,
        component_id: row.get(3)?,
        original_path: row.get(4)?,
        backup_path: row.get(5)?,
        sha256: row.get(6)?,
        created_at: row.get(7)?,
        metadata_json: row.get(8)?,
    }))
}

pub(super) fn collect_rows<T, F>(rows: rusqlite::MappedRows<'_, F>) -> AppResult<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<AppResult<T>>,
{
    let mut values = Vec::new();

    for row in rows {
        values.push(row.map_err(storage_error)??);
    }

    Ok(values)
}

struct GameRow {
    id: String,
    title: String,
    launcher: String,
    external_id: Option<String>,
    platform: String,
    runtime: String,
    install_path: String,
    executable_candidates_json: String,
}

fn build_game(row: GameRow) -> AppResult<GameInstallation> {
    let id = mapping::game_id(row.id)?;
    let launcher = mapping::launcher(row.launcher)?;
    let platform = mapping::platform(row.platform)?;
    let runtime = mapping::runtime(row.runtime)?;
    let install_path = mapping::path_ref(row.install_path)?;

    let executable_candidates: Vec<PathRef> =
        mapping::deserialize_json(&row.executable_candidates_json)?;

    let mut identity = GameIdentity::new(id, row.title, launcher).map_err(invalid_row)?;

    if let Some(external_id) = row.external_id {
        identity = identity
            .with_external_id(external_id)
            .map_err(invalid_row)?;
    }

    let mut game = GameInstallation::new(identity, platform, runtime, install_path);

    for candidate in executable_candidates {
        game = game.with_executable_candidate(candidate);
    }

    Ok(game)
}

fn build_component(
    id: String,
    game_id: String,
    kind: String,
    technology: String,
    swappability: String,
    files_json: String,
) -> AppResult<GraphicsComponent> {
    let id = mapping::component_id(id)?;
    let game_id = mapping::game_id(game_id)?;
    let kind = mapping::component_kind(kind)?;
    let technology = mapping::graphics_technology(technology)?;
    let swappability = mapping::swappability(swappability)?;
    let files = mapping::component_files(files_json)?;

    let mut component = GraphicsComponent::new(id, game_id, kind, technology, swappability);

    for file in files {
        component = component.with_file(file);
    }

    Ok(component)
}

struct ArtifactRow {
    id: String,
    technology: String,
    file_name: String,
    file_path: String,
    version: Option<String>,
    sha256: String,
    source: Option<String>,
    source_game_id: Option<String>,
    trust_level: String,
}

fn build_artifact(row: ArtifactRow) -> AppResult<LibraryArtifact> {
    let id = mapping::artifact_id(row.id)?;
    let technology = mapping::graphics_technology(row.technology)?;
    let file_path = mapping::path_ref(row.file_path)?;
    let sha256 = mapping::sha256(row.sha256)?;
    let trust_level = mapping::artifact_trust_level(row.trust_level)?;
    let mut file = ComponentFile::new(file_path).with_sha256(sha256);

    if let Some(version) = row.version {
        file = file.with_version(mapping::version(version)?);
    }

    let mut artifact = LibraryArtifact::new(id, technology, row.file_name, file, trust_level)
        .map_err(invalid_row)?;

    if let Some(source) = row.source {
        artifact = artifact.with_source(source).map_err(invalid_row)?;
    }

    if let Some(source_game_id) = row.source_game_id {
        artifact = artifact.with_source_game_id(mapping::game_id(source_game_id)?);
    }

    Ok(artifact)
}

fn build_operation(
    id: String,
    game_id: String,
    kind: String,
    status: String,
    created_at: i64,
    completed_at: Option<i64>,
    metadata_json: Option<String>,
) -> AppResult<OperationRecord> {
    let mut operation = OperationRecord::new(
        mapping::operation_id(id)?,
        mapping::game_id(game_id)?,
        OperationKind::from_storage(kind).map_err(invalid_row)?,
        OperationStatus::from_storage(status).map_err(invalid_row)?,
        UnixTimestampMillis::new(created_at).map_err(invalid_row)?,
    );

    if let Some(completed_at) = completed_at {
        operation = operation
            .with_completed_at(UnixTimestampMillis::new(completed_at).map_err(invalid_row)?);
    }

    if let Some(metadata_json) = metadata_json {
        operation = operation.with_metadata_json(mapping::metadata_json(metadata_json)?);
    }

    Ok(operation)
}

fn build_operation_item(
    operation_id: String,
    component_id: String,
    artifact_id: Option<String>,
    source_path: String,
    target_path: Option<String>,
    status: String,
    metadata_json: Option<String>,
) -> AppResult<OperationItemRecord> {
    let mut item = OperationItemRecord::new(
        mapping::operation_id(operation_id)?,
        mapping::component_id(component_id)?,
        mapping::path_ref(source_path)?,
        OperationStatus::from_storage(status).map_err(invalid_row)?,
    );

    if let Some(artifact_id) = artifact_id {
        item = item.with_artifact_id(mapping::artifact_id(artifact_id)?);
    }

    if let Some(target_path) = target_path {
        item = item.with_target_path(mapping::path_ref(target_path)?);
    }

    if let Some(metadata_json) = metadata_json {
        item = item.with_metadata_json(mapping::metadata_json(metadata_json)?);
    }

    Ok(item)
}

struct BackupRow {
    id: String,
    operation_id: String,
    game_id: String,
    component_id: Option<String>,
    original_path: String,
    backup_path: String,
    sha256: Option<String>,
    created_at: i64,
    metadata_json: Option<String>,
}

fn build_backup(row: BackupRow) -> AppResult<BackupRecord> {
    let mut backup = BackupRecord::new(
        BackupId::new(row.id).map_err(invalid_row)?,
        mapping::operation_id(row.operation_id)?,
        mapping::game_id(row.game_id)?,
        mapping::path_ref(row.original_path)?,
        mapping::path_ref(row.backup_path)?,
        UnixTimestampMillis::new(row.created_at).map_err(invalid_row)?,
    );

    if let Some(component_id) = row.component_id {
        backup = backup.with_component_id(mapping::component_id(component_id)?);
    }

    if let Some(sha256) = row.sha256 {
        backup = backup.with_sha256(mapping::sha256(sha256)?);
    }

    if let Some(metadata_json) = row.metadata_json {
        backup = backup.with_metadata_json(mapping::metadata_json(metadata_json)?);
    }

    Ok(backup)
}

#[cfg(test)]
mod tests {
    use renderpilot_application::AppErrorKind;

    use super::build_operation;

    #[test]
    fn build_operation_rejects_invalid_metadata_json() {
        let error = build_operation(
            "operation-1".to_owned(),
            "game-1".to_owned(),
            "scan".to_owned(),
            "planned".to_owned(),
            1,
            None,
            Some("{".to_owned()),
        )
        .unwrap_err();

        assert_eq!(error.kind(), AppErrorKind::StorageFailed);
        assert!(error.message().contains("invalid sqlite row"));
        assert!(error.message().contains("metadata json must be valid JSON"));
    }
}
