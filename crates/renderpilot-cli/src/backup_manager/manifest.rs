use std::{fs, path::Path};

use renderpilot_application::{AppError, AppResult, UnixTimestampMillis};
use renderpilot_domain::{GameId, GraphicsTechnology, OperationId, PathRef, Sha256Hash, Version};
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

pub(super) struct BackupManifestInput<'a> {
    pub(super) operation_id: &'a OperationId,
    pub(super) game_id: &'a GameId,
    pub(super) original_path: &'a PathRef,
    pub(super) technology: GraphicsTechnology,
    pub(super) version: Option<&'a Version>,
    pub(super) sha256: &'a Sha256Hash,
    pub(super) created_at: UnixTimestampMillis,
    pub(super) app_version: &'a str,
}

impl BackupManifest {
    pub(super) fn new(input: BackupManifestInput<'_>) -> Self {
        Self {
            operation_id: input.operation_id.as_str().to_owned(),
            game_id: input.game_id.as_str().to_owned(),
            original_path: input.original_path.as_str().to_owned(),
            technology: input.technology.as_slug().to_owned(),
            version: input.version.map(|version| version.as_str().to_owned()),
            sha256: input.sha256.as_str().to_owned(),
            created_at: input.created_at.as_i64(),
            app_version: input.app_version.to_owned(),
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
