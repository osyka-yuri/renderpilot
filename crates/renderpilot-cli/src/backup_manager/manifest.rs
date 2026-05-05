use std::{fs, path::Path};

use renderpilot_application::{AppError, AppResult, UnixTimestampMillis};
use renderpilot_domain::{GameId, GraphicsTechnology, OperationId, PathRef, Sha256Hash};
use serde::Serialize;

use super::filesystem::file_system_error;

#[derive(Debug, Serialize)]
pub(super) struct BackupManifest {
    operation_id: String,
    game_id: String,
    original_path: String,
    technology: String,
    version: Option<String>,
    sha256: String,
    created_at: i64,
    app_version: String,
}

impl BackupManifest {
    pub(super) fn new(
        operation_id: OperationId,
        game_id: GameId,
        original_path: PathRef,
        technology: GraphicsTechnology,
        version: Option<renderpilot_domain::Version>,
        sha256: Sha256Hash,
        created_at: UnixTimestampMillis,
        app_version: &str,
    ) -> Self {
        Self {
            operation_id: operation_id.as_str().to_owned(),
            game_id: game_id.as_str().to_owned(),
            original_path: original_path.as_str().to_owned(),
            technology: technology.as_slug().to_owned(),
            version: version.map(|version| version.as_str().to_owned()),
            sha256: sha256.as_str().to_owned(),
            created_at: created_at.as_i64(),
            app_version: app_version.to_owned(),
        }
    }
}

pub(super) fn write_backup_manifest(path: &Path, manifest: &BackupManifest) -> AppResult<()> {
    let contents = serde_json::to_vec_pretty(manifest).map_err(|error| {
        AppError::provider_failed(format!("failed to serialize backup manifest: {error}"))
    })?;

    fs::write(path, contents).map_err(|error| {
        file_system_error(
            format!("failed to write backup manifest {}", path.display()),
            error,
        )
    })
}