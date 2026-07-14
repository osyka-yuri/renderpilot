use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use renderpilot_detection::LibraryPatternSet;
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, GraphicsTechnology, LibraryArtifact, PathRef,
    Sha256Hash, Version,
};

use crate::ServiceError;

use super::{
    library_error,
    manifest::load_local_manifest,
    types::{LibraryManifest, LibraryManifestEntry},
};

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

/// Single authoritative index for catalog listing and artifact download.
///
/// Built in one pass so single-file rows and composed packages always come from
/// the same composition result. Each [`LibraryArtifact`] is owned once in
/// [`Self::artifacts`]; packages point at an index into that vec rather than
/// holding a second copy.
#[derive(Debug)]
pub(super) struct ManifestArtifactIndex {
    artifacts: Vec<LibraryArtifact>,
    /// Single-file artifact id → manifest entry id.
    entry_ids: HashMap<ArtifactId, String>,
    debug_entry_ids: HashSet<String>,
    /// Composed package id → slot in `artifacts` + member entry ids (file order).
    packages: HashMap<ArtifactId, IndexedPackage>,
}

/// Package metadata that does not duplicate the artifact body.
#[derive(Debug)]
struct IndexedPackage {
    artifact_index: usize,
    member_entry_ids: Vec<String>,
}

impl ManifestArtifactIndex {
    /// Catalog listing parts (drops package member metadata).
    pub(super) fn into_catalog_parts(self) -> ManifestArtifactsResult {
        (self.artifacts, self.entry_ids, self.debug_entry_ids)
    }

    /// Manifest entry id for a single-file catalog artifact.
    pub(super) fn single_entry_id(&self, artifact_id: &ArtifactId) -> Option<&str> {
        self.entry_ids.get(artifact_id).map(String::as_str)
    }

    /// Virtual multi-file package: the same artifact the catalog lists, plus
    /// member entry ids in artifact file order.
    pub(super) fn package(
        &self,
        artifact_id: &ArtifactId,
    ) -> Option<(&LibraryArtifact, &[String])> {
        let package = self.packages.get(artifact_id)?;
        Some((
            &self.artifacts[package.artifact_index],
            package.member_entry_ids.as_slice(),
        ))
    }

    /// Ids of composed multi-file packages (tests / diagnostics).
    #[cfg(test)]
    fn package_ids(&self) -> impl Iterator<Item = &ArtifactId> {
        self.packages.keys()
    }
}

/// Reads the local library manifest and converts all entries into [`LibraryArtifact`]s.
pub fn manifest_entries_as_artifacts() -> Result<ManifestArtifactsResult, ServiceError> {
    let Some(manifest) = load_local_manifest()? else {
        return Ok((Vec::new(), HashMap::new(), HashSet::new()));
    };
    Ok(manifest_artifact_index(&manifest)?.into_catalog_parts())
}

/// Builds every single-file artifact and every strict composed package from an
/// already-loaded manifest. Package construction errors are intentionally
/// propagated: a malformed multi-file package must not be hidden as a partial
/// single-file catalog entry.
pub(super) fn manifest_artifact_index(
    manifest: &LibraryManifest,
) -> Result<ManifestArtifactIndex, ServiceError> {
    let patterns = load_library_patterns();
    let packages = super::compose_all_packages(&manifest.entries)?;

    let mut artifacts = Vec::with_capacity(manifest.entries.len() + packages.len());
    let mut entry_ids = HashMap::with_capacity(manifest.entries.len());
    let mut debug_entry_ids = HashSet::new();

    for entry in &manifest.entries {
        let artifact = match build_manifest_index_artifact(entry, patterns) {
            Ok(artifact) => artifact,
            Err(error) => {
                log_manifest_entry_skip(entry, &error);
                continue;
            }
        };

        if entry.build.build_type == "debug" {
            debug_entry_ids.insert(entry.entry_id.clone());
        }
        entry_ids.insert(artifact.id().clone(), entry.entry_id.clone());
        artifacts.push(artifact);
    }

    let mut packages_index = HashMap::with_capacity(packages.len());
    for package in packages {
        let package_id = package.artifact.id().clone();
        let artifact_index = artifacts.len();
        packages_index.insert(
            package_id,
            IndexedPackage {
                artifact_index,
                member_entry_ids: package.member_entry_ids,
            },
        );
        artifacts.push(package.artifact);
    }

    Ok(ManifestArtifactIndex {
        artifacts,
        entry_ids,
        debug_entry_ids,
        packages: packages_index,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libraries::types::{
        BuildInfo, DllFileInfo, FilesInfo, HashesInfo, LibraryInfo, SignatureInfo, VersionInfo,
        ZstFileInfo,
    };

    fn streamline_entry(entry_id: &str, file_name: &str, sha256: &str) -> LibraryManifestEntry {
        LibraryManifestEntry {
            entry_id: entry_id.to_owned(),
            library: LibraryInfo {
                id: entry_id.to_owned(),
                file_name: file_name.to_owned(),
            },
            version: VersionInfo {
                value: "2.9.0.0".to_owned(),
                sort_key: "0002.0009.0000.0000".to_owned(),
            },
            build: BuildInfo {
                build_type: "release".to_owned(),
                label: None,
            },
            files: FilesInfo {
                dll: DllFileInfo {
                    size_bytes: 1,
                    hashes: HashesInfo {
                        sha256: sha256.to_owned(),
                    },
                },
                zst: ZstFileInfo {
                    size_bytes: 1,
                    download_url: "https://example.test/library.dll.zst".to_owned(),
                },
            },
            signature: SignatureInfo::Unsigned,
        }
    }

    fn manifest(entries: Vec<LibraryManifestEntry>) -> LibraryManifest {
        LibraryManifest {
            schema_version: 1,
            generated_at: "2026-07-14T00:00:00Z".to_owned(),
            entries,
        }
    }

    #[test]
    fn index_exposes_the_composed_package_used_by_artifact_download() {
        let index = manifest_artifact_index(&manifest(vec![
            streamline_entry("sl_common", "sl.common.dll", &"a".repeat(64)),
            streamline_entry("sl_interposer", "sl.interposer.dll", &"b".repeat(64)),
        ]))
        .expect("valid composed package");

        let package_ids: Vec<_> = index.package_ids().cloned().collect();
        assert_eq!(package_ids.len(), 1);
        let package_id = &package_ids[0];

        let (package_artifact, member_ids) = index
            .package(package_id)
            .expect("download resolves the package");
        assert_eq!(package_artifact.id(), package_id);
        assert_eq!(member_ids, ["sl_common", "sl_interposer"]);
        assert!(
            index.single_entry_id(package_id).is_none(),
            "package ids are not single-file entry ids"
        );

        let (artifacts, entry_ids, _) = index.into_catalog_parts();
        assert_eq!(entry_ids.len(), 2);
        assert_eq!(artifacts.len(), 3, "two singles + one package");
        assert!(
            artifacts.iter().any(|artifact| artifact.id() == package_id),
            "catalog listing includes the package artifact download materializes"
        );
    }

    #[test]
    fn index_propagates_invalid_composed_package_instead_of_skipping_it() {
        let error = manifest_artifact_index(&manifest(vec![
            streamline_entry("sl_common_lower", "sl.common.dll", &"a".repeat(64)),
            streamline_entry("sl_common_upper", "SL.COMMON.dll", &"b".repeat(64)),
        ]))
        .expect_err("duplicate package target must reject the whole manifest index");

        assert!(
            error
                .to_string()
                .contains("duplicate composed package install target")
        );
    }
}
