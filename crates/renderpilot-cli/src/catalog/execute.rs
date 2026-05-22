use std::{fs, path::Path, path::PathBuf};

use renderpilot_application::{AppError, AppResult, ComponentRepository};
use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, GameId, GraphicsComponent, LibraryArtifact,
};
use renderpilot_storage_sqlite::SqliteStorage;
use serde::Serialize;

use crate::{
    catalog::{
        self,
        swap::{require_artifact, require_component_for_game, require_game},
    },
    error::CliError,
};

#[derive(Debug, Serialize)]
pub(crate) struct SwapResult {
    pub(crate) game_id: String,
    pub(crate) component_id: String,
    pub(crate) applied_path: String,
    pub(crate) replacement_path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RollbackResult {
    pub(crate) game_id: String,
    pub(crate) component_id: String,
    pub(crate) restored_path: String,
}

pub(crate) fn build_swap_plan(
    game_id: GameId,
    component_id: ComponentId,
    artifact_id: ArtifactId,
) -> Result<renderpilot_application::OperationPlan, CliError> {
    Ok(catalog::build_swap_plan(game_id, component_id, artifact_id)?.plan)
}

pub(crate) fn apply_swap(
    game_id: GameId,
    component_id: ComponentId,
    artifact_id: ArtifactId,
) -> Result<SwapResult, CliError> {
    catalog::with_catalog_storage(|storage| {
        require_game(storage, &game_id)?;
        let component = require_component_for_game(storage, &game_id, &component_id)?;
        let artifact = require_artifact(storage, &artifact_id)?;

        let paths = SwapPaths::from_component(&component)?;

        validate_files_exist(&paths, &artifact)?;
        ensure_backup_sidecar(&paths)?;
        copy_artifact_over_target(&paths, &artifact)?;

        let updated_file = build_updated_file(&paths, &artifact);
        upsert_component_file(storage, &game_id, &component, &updated_file)?;

        Ok(SwapResult {
            game_id: game_id.as_str().to_owned(),
            component_id: component_id.as_str().to_owned(),
            applied_path: paths.target_path_string(),
            replacement_path: artifact.path().as_str().to_owned(),
        })
    })
}

pub(crate) fn rollback_component(
    game_id: GameId,
    component_id: ComponentId,
) -> Result<RollbackResult, CliError> {
    catalog::with_catalog_storage(|storage| {
        require_game(storage, &game_id)?;
        let component = require_component_for_game(storage, &game_id, &component_id)?;

        let paths = SwapPaths::from_component(&component)?;

        restore_backup_sidecar(&paths)?;
        let restored_file = read_restored_file_metadata(&paths)?;
        upsert_component_file(storage, &game_id, &component, &restored_file)?;

        Ok(RollbackResult {
            game_id: game_id.as_str().to_owned(),
            component_id: component_id.as_str().to_owned(),
            restored_path: paths.target_path_string(),
        })
    })
}

// --- private helpers ---

struct SwapPaths {
    target_file: ComponentFile,
    sidecar_path: PathBuf,
}

impl SwapPaths {
    fn from_component(component: &GraphicsComponent) -> AppResult<Self> {
        let target_file = component
            .files()
            .first()
            .cloned()
            .ok_or_else(|| AppError::invalid_input("component has no files"))?;
        let sidecar_path = PathBuf::from(format!("{}.bak", target_file.path().as_str()));

        Ok(Self {
            target_file,
            sidecar_path,
        })
    }

    fn target_path(&self) -> &Path {
        Path::new(self.target_file.path().as_str())
    }

    fn target_path_string(&self) -> String {
        self.target_file.path().as_str().to_owned()
    }
}

fn validate_files_exist(paths: &SwapPaths, artifact: &LibraryArtifact) -> AppResult<()> {
    if !paths.target_path().exists() {
        return Err(AppError::invalid_input(format!(
            "target file does not exist: {}",
            paths.target_path_string()
        )));
    }
    let artifact_path = Path::new(artifact.path().as_str());
    if !artifact_path.exists() {
        return Err(AppError::invalid_input(format!(
            "artifact file does not exist: {}",
            artifact.path().as_str()
        )));
    }
    Ok(())
}

fn ensure_backup_sidecar(paths: &SwapPaths) -> AppResult<()> {
    if paths.sidecar_path.exists() {
        return Ok(());
    }
    fs::rename(paths.target_path(), &paths.sidecar_path).map_err(|error| {
        AppError::provider_failed(format!(
            "failed to create backup for {}: {error}",
            paths.target_path_string()
        ))
    })
}

fn copy_artifact_over_target(paths: &SwapPaths, artifact: &LibraryArtifact) -> AppResult<()> {
    fs::copy(Path::new(artifact.path().as_str()), paths.target_path())
        .map(|_| ())
        .map_err(|error| {
            AppError::provider_failed(format!(
                "failed to copy artifact to {}: {error}",
                paths.target_path_string()
            ))
        })
}

fn build_updated_file(paths: &SwapPaths, artifact: &LibraryArtifact) -> ComponentFile {
    let mut file =
        ComponentFile::new(paths.target_file.path().clone()).with_sha256(artifact.sha256().clone());
    if let Some(version) = artifact.version() {
        file = file.with_version(version.clone());
    }
    file
}

fn restore_backup_sidecar(paths: &SwapPaths) -> AppResult<()> {
    if !paths.sidecar_path.exists() {
        return Err(AppError::invalid_input(format!(
            "backup file does not exist: {}",
            paths.sidecar_path.display()
        )));
    }

    fs::rename(&paths.sidecar_path, paths.target_path()).map_err(|error| {
        AppError::provider_failed(format!(
            "failed to restore backup to {}: {error}",
            paths.target_path_string()
        ))
    })
}

fn read_restored_file_metadata(paths: &SwapPaths) -> AppResult<ComponentFile> {
    let sha256 = renderpilot_detection::sha256_file(paths.target_path())?;
    let version = renderpilot_detection::read_windows_file_version(paths.target_path());

    let mut file = ComponentFile::new(paths.target_file.path().clone()).with_sha256(sha256);
    if let Some(version) = version {
        file = file.with_version(version);
    }
    Ok(file)
}

fn upsert_component_file(
    storage: &SqliteStorage,
    game_id: &GameId,
    component: &GraphicsComponent,
    file: &ComponentFile,
) -> AppResult<()> {
    let mut rebuilt = GraphicsComponent::new(
        component.id().clone(),
        component.game_id().clone(),
        component.kind(),
        component.technology(),
        component.swappability(),
    );
    for existing in component.files() {
        if existing.path() == file.path() {
            rebuilt = rebuilt.with_file(file.clone());
        } else {
            rebuilt = rebuilt.with_file(existing.clone());
        }
    }

    storage.replace_components_for_game(game_id, &[rebuilt])
}
