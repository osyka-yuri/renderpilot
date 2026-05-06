use std::path::PathBuf;

use renderpilot_application::{AppError, AppResult};
use renderpilot_detection::{DetectedLibraryFile, LibraryPatternComponentDetector};
use renderpilot_domain::{ArtifactId, ArtifactTrustLevel, ComponentFile, GameId, LibraryArtifact};
use renderpilot_platform_windows::ManualFolderGameSource;

use crate::error::CliError;

use super::{storage::open_catalog_storage, ScanFolderCatalogResult};

pub(super) fn scan_folder_impl(path: PathBuf) -> Result<ScanFolderCatalogResult, CliError> {
    let game = ManualFolderGameSource::new(path).discover_game()?;
    let detector = LibraryPatternComponentDetector::windows_default()?;
    let libraries = detector.detect_library_files(&game)?;
    let components = libraries
        .iter()
        .cloned()
        .map(|library| library.into_component(&game))
        .collect::<AppResult<Vec<_>>>()?;
    let artifacts = libraries
        .iter()
        .map(|library| build_scanned_artifact(library, game.id()))
        .collect::<AppResult<Vec<_>>>()?;

    open_catalog_storage()?.save_scan_result(&game, &components, &artifacts)?;

    Ok(ScanFolderCatalogResult { game, libraries })
}

fn build_scanned_artifact(
    library: &DetectedLibraryFile,
    game_id: &GameId,
) -> AppResult<LibraryArtifact> {
    let artifact_id = ArtifactId::new(format!("artifact:{}", library.sha256().as_str()))
        .map_err(domain_to_detection_error)?;
    let file = component_file_from_detected_library(library);

    LibraryArtifact::new(
        artifact_id,
        library.technology(),
        library.file_name(),
        file,
        ArtifactTrustLevel::LocalObserved,
    )
    .map_err(domain_to_detection_error)?
    .with_source("scan-folder")
    .map_err(domain_to_detection_error)
    .map(|artifact| artifact.with_source_game_id(game_id.clone()))
}

fn component_file_from_detected_library(library: &DetectedLibraryFile) -> ComponentFile {
    let mut file =
        ComponentFile::new(library.file_path().clone()).with_sha256(library.sha256().clone());

    if let Some(version) = library.version().cloned() {
        file = file.with_version(version);
    }

    file
}

fn domain_to_detection_error(error: impl std::fmt::Display) -> AppError {
    AppError::detection_failed(error.to_string())
}
