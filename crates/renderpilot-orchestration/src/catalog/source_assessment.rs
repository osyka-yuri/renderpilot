//! Read-only validation of catalog artifact source files.

use std::path::Path;

use renderpilot_detection::sha256_file;
use renderpilot_domain::{GraphicsTechnology, LibraryArtifact};

/// Why an artifact source cannot be used for a transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArtifactSourceIssue {
    Missing { path: String },
    NotRegularFile { path: String },
    MissingExpectedHash { path: String },
    ContentMismatch { path: String },
    Unreadable { path: String, detail: String },
    RuntimeMetadataMismatch { path: String, detail: String },
}

impl std::fmt::Display for ArtifactSourceIssue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { path } => write!(formatter, "missing path {path}"),
            Self::NotRegularFile { path } => write!(formatter, "not a regular file at {path}"),
            Self::MissingExpectedHash { path } => {
                write!(formatter, "missing expected content hash for {path}")
            }
            Self::ContentMismatch { path } => {
                write!(formatter, "content hash mismatch at {path}")
            }
            Self::Unreadable { path, detail } => {
                write!(formatter, "cannot read source {path}: {detail}")
            }
            Self::RuntimeMetadataMismatch { path, detail } => {
                write!(
                    formatter,
                    "runtime source metadata mismatch at {path}: {detail}"
                )
            }
        }
    }
}

/// Read-only assessment of all source members in one artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArtifactSourceAssessment {
    Usable,
    Unusable(ArtifactSourceIssue),
}

/// Hashes every source member without changing files or catalog state.
pub(super) fn assess_artifact_sources(artifact: &LibraryArtifact) -> ArtifactSourceAssessment {
    for file in artifact.files() {
        let path = Path::new(file.path().as_str());
        let display_path = file.path().as_str().to_owned();
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::Missing {
                    path: display_path,
                });
            }
            Err(error) => {
                return ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::Unreadable {
                    path: display_path,
                    detail: error.to_string(),
                });
            }
        };
        if !metadata.is_file() {
            return ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::NotRegularFile {
                path: display_path,
            });
        }

        let Some(expected) = file.sha256() else {
            return ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::MissingExpectedHash {
                path: display_path,
            });
        };

        let actual = match sha256_file(path) {
            Ok(actual) => actual,
            Err(error) => {
                return ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::Unreadable {
                    path: display_path,
                    detail: error.to_string(),
                });
            }
        };
        if actual != *expected {
            return ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::ContentMismatch {
                path: display_path,
            });
        }
    }

    ArtifactSourceAssessment::Usable
}

/// Re-reads byte-derived runtime facts after transition compatibility passes.
///
/// Keeping this as a typed assessment preserves the historical ordering:
/// content staleness is checked first, executable/component compatibility
/// second, and source PE metadata last.
pub(super) fn assess_artifact_runtime_metadata(
    artifact: &LibraryArtifact,
) -> ArtifactSourceAssessment {
    for file in artifact.files() {
        if let Some(issue) =
            assess_runtime_member_metadata(artifact, file, Path::new(file.path().as_str()))
        {
            return ArtifactSourceAssessment::Unusable(issue);
        }
    }
    ArtifactSourceAssessment::Usable
}

fn assess_runtime_member_metadata(
    artifact: &LibraryArtifact,
    file: &renderpilot_domain::ComponentFile,
    path: &Path,
) -> Option<ArtifactSourceIssue> {
    let target = artifact.metadata().runtime_target()?;
    let issue = |detail: &str| ArtifactSourceIssue::RuntimeMetadataMismatch {
        path: file.path().as_str().to_owned(),
        detail: detail.to_owned(),
    };
    let Some(inspection) = renderpilot_detection::inspect_pe(path) else {
        return Some(issue("PE image cannot be read"));
    };
    if inspection.architecture != Some(target.architecture()) {
        return Some(issue("PE architecture differs from catalog metadata"));
    }

    if artifact.technology() == GraphicsTechnology::OpenVr {
        let Some(observed) = inspection.compatibility_profile() else {
            return Some(issue(
                "complete OpenVR export-surface profile cannot be read",
            ));
        };
        if file.pe_compatibility() != Some(&observed) {
            return Some(issue(
                "OpenVR export-surface profile differs from catalog metadata",
            ));
        }
    }

    if artifact.technology() == GraphicsTechnology::D3D12Agility {
        let declared = target
            .compatibility()
            .and_then(renderpilot_domain::RuntimeCompatibility::as_d3d12_sdk_version);
        let observed = inspection
            .version
            .as_ref()
            .and_then(|version| version.segments().get(1))
            .and_then(|segment| u32::try_from(*segment).ok());
        if observed != declared {
            return Some(issue(
                "D3D12 SDK line differs from the catalog runtime target",
            ));
        }
    }

    None
}
