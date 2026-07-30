//! Post-install hash verification and metadata rebind.
//!
//! Pure plan math lives in [`super::planning`]. The read-only source assessment
//! belongs to the shared catalog preflight; this module closes the later
//! preflight-to-copy race against the bytes actually installed.

use renderpilot_application::{AppError, AppResult};
use renderpilot_detection::{inspect_pe_bytes, sha256_bytes};
use renderpilot_domain::{ComponentFile, LibraryTechnology};

use super::types::PlannedFile;

/// Verifies each installed target still matches the planned artifact bytes,
/// then re-reads PE metadata for persistence.
///
/// Mutates `planned` in place: paths and copy sources stay put; only
/// hash/version on each file are replaced with on-disk truth.
///
/// A source can change after preflight but before `copy`; hashing the target
/// closes that TOCTOU window. Unreadable PE metadata stays unknown instead of
/// inheriting a manifest label that was never observed on the installed file.
pub(super) fn rebind_planned_files_for_technology(
    planned: &mut [PlannedFile],
    technology: LibraryTechnology,
) -> AppResult<()> {
    planned
        .iter_mut()
        .try_for_each(|plan| rebind_planned_file(plan, technology))
}

fn rebind_planned_file(plan: &mut PlannedFile, technology: LibraryTechnology) -> AppResult<()> {
    let target = plan.target();
    let Some(expected) = plan.file.sha256() else {
        return Err(AppError::stale_replacement_source());
    };
    let bytes = std::fs::read(&target).map_err(|error| {
        AppError::provider_failed(format!(
            "failed to read installed file {}: {error}",
            target.display()
        ))
    })?;
    let sha256 = sha256_bytes(&bytes)?;
    if sha256 != *expected {
        return Err(AppError::stale_replacement_source());
    }

    let inspection = inspect_pe_bytes(&bytes);
    if technology == LibraryTechnology::OpenVr {
        let expected_profile = plan
            .file
            .pe_compatibility()
            .ok_or_else(AppError::stale_replacement_source)?;
        let observed_profile = inspection
            .compatibility_profile()
            .ok_or_else(AppError::stale_replacement_source)?;
        if &observed_profile != expected_profile {
            return Err(AppError::stale_replacement_source());
        }
    }

    // Fresh ComponentFile keeps the install path and drops any planned version
    // unless PE metadata can be read from the installed bytes.
    let mut file = ComponentFile::new(plan.file.path().clone()).with_sha256(sha256);
    file = crate::coordinated_files::with_observed_inspection(file, technology, &inspection);
    plan.file = file;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use renderpilot_domain::{
        Architecture, ArtifactId, ArtifactTrustLevel, ComponentFile, LibraryArtifact,
        LibraryTechnology, PathRef, PeCompatibilityProfile, PeExportSet, Sha256Hash, Version,
    };

    use super::rebind_planned_files_for_technology;
    use crate::catalog::execute::types::PlannedFile;
    use crate::catalog::source_assessment::{
        ArtifactSourceAssessment, ArtifactSourceIssue, assess_artifact_runtime_metadata,
        assess_artifact_sources,
    };

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
            LibraryTechnology::DlssSuperResolution,
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
            assess_artifact_sources(&artifact),
            ArtifactSourceAssessment::Usable
        );
    }

    #[test]
    fn reports_content_mismatch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("nvngx_dlss.dll");
        write(&source, b"dll-bytes-3.1.30");

        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:stale").expect("id"),
            LibraryTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(PathRef::new(source.to_string_lossy().as_ref()).expect("path"))
                    .with_sha256(Sha256Hash::new(HEX64).expect("sha"))
                    .with_version(Version::parse("310.7.0.0").expect("version")),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact");

        match assess_artifact_sources(&artifact) {
            ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::ContentMismatch { .. }) => {}
            other => panic!("expected content mismatch, got {other:?}"),
        }
    }

    #[test]
    fn reports_missing_path() {
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:missing").expect("id"),
            LibraryTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(PathRef::new("C:/does/not/exist/nvngx_dlss.dll").expect("path"))
                    .with_sha256(Sha256Hash::new(HEX64).expect("sha")),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact");

        match assess_artifact_sources(&artifact) {
            ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::Missing { .. }) => {}
            other => panic!("expected missing path, got {other:?}"),
        }
    }

    #[test]
    fn reports_non_file_source() {
        let dir = tempfile::tempdir().expect("tempdir");
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:directory").expect("id"),
            LibraryTechnology::DlssSuperResolution,
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
            assess_artifact_sources(&artifact),
            ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::NotRegularFile { .. })
        ));
    }

    #[test]
    fn reports_runtime_pe_mismatch_as_a_typed_source_issue() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("runtime.dll");
        write(&source, b"not-a-pe-image");
        let sha = renderpilot_detection::sha256_file(&source).expect("hash");
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:runtime-metadata-mismatch").expect("id"),
            LibraryTechnology::DlssSuperResolution,
            "runtime.dll",
            vec![
                ComponentFile::new(PathRef::new(source.to_string_lossy().as_ref()).expect("path"))
                    .with_sha256(sha),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(
            renderpilot_domain::ArtifactMetadata::default().with_runtime_target(
                renderpilot_domain::RuntimeTarget::new(renderpilot_domain::Architecture::X64),
            ),
        );

        assert!(matches!(
            assess_artifact_runtime_metadata(&artifact),
            ArtifactSourceAssessment::Unusable(ArtifactSourceIssue::RuntimeMetadataMismatch { .. })
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

        rebind_planned_files_for_technology(&mut planned, LibraryTechnology::DlssSuperResolution)
            .expect("matching target rebinds");
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
        let error = rebind_planned_files_for_technology(
            &mut planned,
            LibraryTechnology::DlssSuperResolution,
        )
        .expect_err("changed target is stale");
        assert_eq!(
            error.kind(),
            &renderpilot_application::AppErrorKind::StaleReplacementSource
        );
    }

    #[test]
    fn openvr_rebind_rejects_matching_non_pe_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("openvr_api.dll");
        write(&target, b"matching-hash-but-not-a-pe");
        let target_ref = PathRef::new(target.to_string_lossy().as_ref()).expect("path");
        let expected = renderpilot_detection::sha256_file(&target).expect("hash");
        let exports =
            PeExportSet::from_canonical_names(vec!["VR_InitInternal".into()]).expect("exports");
        let profile = PeCompatibilityProfile::new(Architecture::X64, exports);
        let mut planned = [PlannedFile {
            source: target,
            file: ComponentFile::new(target_ref)
                .with_sha256(expected)
                .with_pe_compatibility(profile),
        }];

        let error = rebind_planned_files_for_technology(&mut planned, LibraryTechnology::OpenVr)
            .expect_err("OpenVR requires an observed profile");
        assert_eq!(
            error.kind(),
            &renderpilot_application::AppErrorKind::StaleReplacementSource
        );
    }
}
