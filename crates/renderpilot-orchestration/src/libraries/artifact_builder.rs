use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use renderpilot_detection::LibraryPatternSet;
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, GraphicsTechnology, LibraryArtifact, PathRef,
    Sha256Hash, Version,
};

use crate::ServiceError;

use super::{library_error, manifest::load_local_manifest, types::LibraryManifestEntry};

const MANIFEST_DOWNLOAD_SOURCE: &str = "manifest-download";

static BUNDLED_PATTERNS: LazyLock<LibraryPatternSet> = LazyLock::new(|| {
    crate::util::load_bundled_asset_or_default(
        LibraryPatternSet::bundled_defaults,
        LibraryPatternSet::empty,
        "library pattern set",
    )
});

/// Builds a `LibraryArtifact` instance representing a locally cached library file
/// backed by a downloaded manifest entry.
///
/// This resolves the library pattern technology using the known file name and
/// constructs an artifact definition that can be used for swap operations.
pub(super) fn build_manifest_artifact(
    entry: &LibraryManifestEntry,
    dll_path: &std::path::Path,
    sha256: &str,
) -> Result<LibraryArtifact, ServiceError> {
    let patterns = load_library_patterns();

    build_entry_artifact(entry, &dll_path.to_string_lossy(), sha256, patterns, None)
}

/// Reads the local library manifest and converts all successfully parsed entries
/// into abstract `LibraryArtifact` instances without checking local file presence.
///
/// Returns a tuple containing:
/// - The parsed list of `LibraryArtifact`s.
/// - A mapping between the generated `ArtifactId` and the raw manifest `entry_id`.
/// - A set of `entry_id`s whose build type is `"debug"`.
///
/// Return type of [`manifest_entries_as_artifacts`].
pub type ManifestArtifactsResult = (
    Vec<LibraryArtifact>,
    HashMap<ArtifactId, String>,
    HashSet<String>,
);

/// Reads the local library manifest and converts all entries into [`LibraryArtifact`]s.
pub fn manifest_entries_as_artifacts() -> Result<ManifestArtifactsResult, ServiceError> {
    let manifest = match load_local_manifest()? {
        Some(manifest) => manifest,
        None => return Ok((Vec::new(), HashMap::new(), HashSet::new())),
    };

    let patterns = load_library_patterns();

    let mut artifacts = Vec::new();
    let mut entry_ids = HashMap::new();
    let mut debug_entry_ids = HashSet::new();

    for entry in &manifest.entries {
        let artifact = match build_manifest_index_artifact(entry, patterns) {
            Ok(artifact) => artifact,
            Err(error) => {
                log_manifest_entry_skip(entry, &error);
                continue;
            }
        };
        let artifact_id = artifact.id().clone();

        if entry.build.build_type == "debug" {
            debug_entry_ids.insert(entry.entry_id.clone());
        }
        entry_ids.insert(artifact_id, entry.entry_id.clone());
        artifacts.push(artifact);
    }

    // One composed artifact per cohesive FSR release (loader + upscaler +
    // framegen) is added alongside the individual member artifacts above.
    for package in super::fsr_packages::compose_fsr_packages(&manifest.entries) {
        artifacts.push(package.artifact);
    }

    Ok((artifacts, entry_ids, debug_entry_ids))
}

fn load_library_patterns() -> &'static LibraryPatternSet {
    &BUNDLED_PATTERNS
}

fn build_manifest_index_artifact(
    entry: &LibraryManifestEntry,
    patterns: &LibraryPatternSet,
) -> Result<LibraryArtifact, ServiceError> {
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
) -> Result<LibraryArtifact, ServiceError> {
    let technology = patterns
        .match_file_name(&entry.library.file_name)
        .unwrap_or(GraphicsTechnology::Unknown);
    let path = PathRef::new(artifact_path)
        .map_err(|error| library_error(format!("invalid artifact path: {error}")))?;
    let sha256_hash = Sha256Hash::new(sha256)
        .map_err(|error| library_error(format!("invalid sha256: {error}")))?;
    let version = Version::parse(&entry.version.value)
        .map_err(|error| library_error(format!("invalid version: {error}")))?;

    // Manifest entries are single-file (bundle support for downloads is a
    // follow-up), but the id uses the same bundle scheme as locally-scanned
    // artifacts so the same DLL from a scan and from the manifest dedupes.
    let artifact_id = ArtifactId::for_bundle([&sha256_hash]);
    let file = ComponentFile::new(path)
        .with_sha256(sha256_hash)
        .with_version(version);
    let artifact = LibraryArtifact::new(
        artifact_id,
        technology,
        &entry.library.file_name,
        vec![file],
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

fn log_manifest_entry_skip(entry: &LibraryManifestEntry, error: &ServiceError) {
    log::warn!(
        "manifest_entries_as_artifacts: skipping entry {}: {error}",
        entry.entry_id
    );
}
