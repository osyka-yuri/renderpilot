//! Filesystem integrity for swap sources and post-install metadata rebind.
//!
//! Pure plan math lives in [`super::planning`]. This module intentionally
//! touches disk so apply never trusts a stale catalog path or snapshot.

use std::path::Path;

use renderpilot_application::{AppError, AppResult};
use renderpilot_detection::{read_windows_file_version, sha256_file};
use renderpilot_domain::{ComponentFile, LibraryArtifact};

use super::types::PlannedFile;

/// Why an artifact source failed integrity checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArtifactSourceIssue {
    /// Path is gone (deleted game files, moved install, …).
    Missing { path: String },
    /// Path exists but is not a regular file that can be copied as a DLL.
    NotRegularFile { path: String },
    /// The catalog row has no digest to verify against.
    MissingExpectedHash { path: String },
    /// Path exists but bytes no longer match the catalog snapshot.
    ContentMismatch { path: String },
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
        }
    }
}

/// Outcome of pre-apply source integrity checks (single layer — not nested Result).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArtifactSourceCheck {
    Ok,
    Unusable(ArtifactSourceIssue),
}

/// Ensures every artifact source path exists and still matches its declared
/// content hash.
///
/// Returns [`ArtifactSourceCheck::Unusable`] when the catalog snapshot is
/// unusable so the caller can delete the row and surface
/// [`AppError::stale_replacement_source`]. Transient hash I/O failures stay as
/// [`AppError`] (not treated as "stale entry").
pub(super) fn validate_artifact_sources(
    artifact: &LibraryArtifact,
) -> AppResult<ArtifactSourceCheck> {
    for file in artifact.files() {
        let path = Path::new(file.path().as_str());
        if !path.exists() {
            return Ok(ArtifactSourceCheck::Unusable(
                ArtifactSourceIssue::Missing {
                    path: file.path().as_str().to_owned(),
                },
            ));
        }
        if !path.is_file() {
            return Ok(ArtifactSourceCheck::Unusable(
                ArtifactSourceIssue::NotRegularFile {
                    path: file.path().as_str().to_owned(),
                },
            ));
        }

        let Some(expected) = file.sha256() else {
            return Ok(ArtifactSourceCheck::Unusable(
                ArtifactSourceIssue::MissingExpectedHash {
                    path: file.path().as_str().to_owned(),
                },
            ));
        };

        let actual = sha256_file(path).map_err(|error| {
            AppError::provider_failed(format!(
                "failed to hash artifact source {}: {error}",
                file.path().as_str()
            ))
        })?;

        if actual.as_str() != expected.as_str() {
            return Ok(ArtifactSourceCheck::Unusable(
                ArtifactSourceIssue::ContentMismatch {
                    path: file.path().as_str().to_owned(),
                },
            ));
        }
    }
    Ok(ArtifactSourceCheck::Ok)
}

/// Verifies each installed target still matches the planned artifact bytes,
/// then re-reads PE metadata for persistence.
///
/// Mutates `planned` in place: paths and copy sources stay put; only
/// hash/version on each file are replaced with on-disk truth.
///
/// A source can change after preflight but before `copy`; hashing the target
/// closes that TOCTOU window. Unreadable PE metadata stays unknown instead of
/// inheriting a manifest label that was never observed on the installed file.
pub(super) fn rebind_planned_files_from_disk(planned: &mut [PlannedFile]) -> AppResult<()> {
    planned.iter_mut().try_for_each(rebind_planned_file)
}

fn rebind_planned_file(plan: &mut PlannedFile) -> AppResult<()> {
    let target = plan.target();
    let Some(expected) = plan.file.sha256() else {
        return Err(AppError::stale_replacement_source());
    };
    let sha256 = sha256_file(&target).map_err(|error| {
        AppError::provider_failed(format!(
            "failed to hash installed file {}: {error}",
            target.display()
        ))
    })?;
    if sha256 != *expected {
        return Err(AppError::stale_replacement_source());
    }

    // Fresh ComponentFile keeps the install path and drops any planned version
    // unless PE metadata can be read from the installed bytes.
    let mut file = ComponentFile::new(plan.file.path().clone()).with_sha256(sha256);
    if let Some(version) = read_windows_file_version(&target) {
        file = file.with_version(version);
    }
    plan.file = file;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, GraphicsTechnology, LibraryArtifact,
        PathRef, Sha256Hash, Version,
    };

    use super::{
        ArtifactSourceCheck, ArtifactSourceIssue, rebind_planned_files_from_disk,
        validate_artifact_sources,
    };
    use crate::catalog::execute::types::PlannedFile;

    const HEX64: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn write(path: &std::path::Path, bytes: &[u8]) {
        fs::write(path, bytes).expect("write fixture");
    }

    #[test]
    fn accepts_matching_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("nvngx_dlss.dll");
        write(&source, b"dll-bytes-310.7");

        let sha = renderpilot_detection::sha256_file(&source).expect("hash");
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:ok").expect("id"),
            GraphicsTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(PathRef::new(source.to_string_lossy().as_ref()).expect("path"))
                    .with_sha256(sha)
                    .with_version(Version::parse("310.7.0.0").expect("version")),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact");

        assert_eq!(
            validate_artifact_sources(&artifact).expect("hash ok"),
            ArtifactSourceCheck::Ok
        );
    }

    #[test]
    fn reports_content_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("nvngx_dlss.dll");
        write(&source, b"dll-bytes-3.1.30");

        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:stale").expect("id"),
            GraphicsTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(PathRef::new(source.to_string_lossy().as_ref()).expect("path"))
                    .with_sha256(Sha256Hash::new(HEX64).expect("sha"))
                    .with_version(Version::parse("310.7.0.0").expect("version")),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact");

        match validate_artifact_sources(&artifact).expect("validation ran") {
            ArtifactSourceCheck::Unusable(ArtifactSourceIssue::ContentMismatch { .. }) => {}
            other => panic!("expected content mismatch, got {other:?}"),
        }
    }

    #[test]
    fn reports_missing_path() {
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:missing").expect("id"),
            GraphicsTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(PathRef::new("C:/does/not/exist/nvngx_dlss.dll").expect("path"))
                    .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact");

        match validate_artifact_sources(&artifact).expect("validation ran") {
            ArtifactSourceCheck::Unusable(ArtifactSourceIssue::Missing { .. }) => {}
            other => panic!("expected missing path, got {other:?}"),
        }
    }

    #[test]
    fn reports_non_file_source() {
        let dir = tempfile::tempdir().expect("tempdir");
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:directory").expect("id"),
            GraphicsTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(
                    PathRef::new(dir.path().to_string_lossy().as_ref()).expect("path"),
                )
                .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact");

        assert!(matches!(
            validate_artifact_sources(&artifact).expect("validation ran"),
            ArtifactSourceCheck::Unusable(ArtifactSourceIssue::NotRegularFile { .. })
        ));
    }

    #[test]
    fn rebind_keeps_observed_hash_and_drops_unreadable_pe_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("nvngx_dlss.dll");
        write(&target, b"observed-non-pe-bytes");
        let target_ref = PathRef::new(target.to_string_lossy().as_ref()).expect("path");
        let expected = renderpilot_detection::sha256_file(&target).expect("hash");
        let mut planned = [PlannedFile {
            source: target,
            file: ComponentFile::new(target_ref)
                .with_sha256(expected.clone())
                // Catalog plan may carry a manifest label; rebind must not keep it
                // when PE metadata cannot be read from the installed file.
                .with_version(Version::parse("310.7.0.0").expect("version")),
        }];

        rebind_planned_files_from_disk(&mut planned).expect("matching target rebinds");
        assert_eq!(planned[0].file.sha256(), Some(&expected));
        assert_eq!(
            planned[0].file.version(),
            None,
            "unreadable PE metadata must not inherit the manifest version"
        );
    }

    #[test]
    fn rebind_rejects_target_whose_hash_diverged_after_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("nvngx_dlss.dll");
        write(&target, b"bytes-as-planned");
        let target_ref = PathRef::new(target.to_string_lossy().as_ref()).expect("path");
        let expected = renderpilot_detection::sha256_file(&target).expect("hash");

        write(&target, b"mutated-after-copy");
        let mut planned = [PlannedFile {
            source: target,
            file: ComponentFile::new(target_ref).with_sha256(expected),
        }];
        let error =
            rebind_planned_files_from_disk(&mut planned).expect_err("changed target is stale");
        assert_eq!(
            error.kind(),
            &renderpilot_application::AppErrorKind::StaleReplacementSource
        );
    }
}
