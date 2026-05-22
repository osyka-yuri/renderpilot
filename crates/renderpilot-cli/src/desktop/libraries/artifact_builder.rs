use std::collections::HashMap;

use renderpilot_detection::LibraryPatternSet;
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, GraphicsTechnology, LibraryArtifact, PathRef,
    Sha256Hash, Version,
};

use crate::error::CliError;

use super::{library_error, manifest::load_local_manifest, types::LibraryManifestEntry};

const MANIFEST_DOWNLOAD_SOURCE: &str = "manifest-download";

pub(super) fn build_manifest_artifact(
    entry: &LibraryManifestEntry,
    dll_path: &std::path::Path,
    sha256: &str,
) -> Result<LibraryArtifact, CliError> {
    let patterns = load_library_patterns()?;

    build_entry_artifact(entry, &dll_path.to_string_lossy(), sha256, &patterns, None)
}

pub(crate) fn manifest_entries_as_artifacts(
) -> Result<(Vec<LibraryArtifact>, HashMap<ArtifactId, String>), CliError> {
    let manifest = match load_local_manifest()? {
        Some(manifest) => manifest,
        None => return Ok((Vec::new(), HashMap::new())),
    };

    let patterns = load_library_patterns()?;

    let mut artifacts = Vec::new();
    let mut entry_ids = HashMap::new();

    for entry in &manifest.entries {
        let artifact = match build_manifest_index_artifact(entry, &patterns) {
            Ok(artifact) => artifact,
            Err(error) => {
                log_manifest_entry_skip(entry, &error);
                continue;
            }
        };
        let artifact_id = artifact.id().clone();

        entry_ids.insert(artifact_id, entry.entry_id.clone());
        artifacts.push(artifact);
    }

    Ok((artifacts, entry_ids))
}

fn load_library_patterns() -> Result<LibraryPatternSet, CliError> {
    LibraryPatternSet::bundled_defaults()
        .map_err(|error| library_error(format!("failed to load library patterns: {error}")))
}

fn build_manifest_index_artifact(
    entry: &LibraryManifestEntry,
    patterns: &LibraryPatternSet,
) -> Result<LibraryArtifact, CliError> {
    let artifact = build_entry_artifact(
        entry,
        &format!("manifest://{}", entry.entry_id),
        &entry.files.dll.hashes.sha256,
        patterns,
        Some(MANIFEST_DOWNLOAD_SOURCE),
    )?;

    Ok(artifact)
}

fn build_entry_artifact(
    entry: &LibraryManifestEntry,
    artifact_path: &str,
    sha256: &str,
    patterns: &LibraryPatternSet,
    source: Option<&str>,
) -> Result<LibraryArtifact, CliError> {
    let technology = patterns
        .match_file_name(&entry.library.file_name)
        .unwrap_or(GraphicsTechnology::Unknown);
    let artifact_id = ArtifactId::new(format!("artifact:{sha256}"))
        .map_err(|error| library_error(format!("invalid artifact id: {error}")))?;
    let path = PathRef::new(artifact_path)
        .map_err(|error| library_error(format!("invalid artifact path: {error}")))?;
    let sha256_hash = Sha256Hash::new(sha256)
        .map_err(|error| library_error(format!("invalid sha256: {error}")))?;
    let version = Version::parse(&entry.version.value)
        .map_err(|error| library_error(format!("invalid version: {error}")))?;

    let file = ComponentFile::new(path)
        .with_sha256(sha256_hash)
        .with_version(version);
    let artifact = LibraryArtifact::new(
        artifact_id,
        technology,
        &entry.library.file_name,
        file,
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .map_err(|error| library_error(format!("failed to build artifact: {error}")))?;

    match source {
        Some(source) => artifact
            .with_source(source)
            .map_err(|error| library_error(format!("failed to attach artifact source: {error}"))),
        None => Ok(artifact),
    }
}

fn log_manifest_entry_skip(entry: &LibraryManifestEntry, error: &CliError) {
    log::warn!(
        "manifest_entries_as_artifacts: skipping entry {}: {error}",
        entry.entry_id
    );
}
