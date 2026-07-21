use renderpilot_domain::{GraphicsComponent, LibraryArtifact, UpstreamPackageProvider, openvr};

use super::{SwapCompatibilityError, runtime_file_name};

pub(super) fn validate_artifact(artifact: &LibraryArtifact) -> Result<(), SwapCompatibilityError> {
    if artifact.files().len() != 1
        || !artifact.file_name().eq_ignore_ascii_case(openvr::DLL_NAME)
        || runtime_file_name(&artifact.files()[0])
            .is_none_or(|name| !name.eq_ignore_ascii_case(openvr::DLL_NAME))
    {
        return Err(SwapCompatibilityError::InvalidArtifactMetadata);
    }
    let target = artifact
        .metadata()
        .runtime_target()
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    if target.compatibility().is_some() {
        return Err(SwapCompatibilityError::InvalidArtifactMetadata);
    }
    let package = artifact
        .metadata()
        .upstream_package()
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    let file = &artifact.files()[0];
    let profile = file
        .pe_compatibility()
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    if package.provider() != UpstreamPackageProvider::GitHub
        || package.id() != openvr::UPSTREAM_REPOSITORY
        || profile.architecture() != target.architecture()
    {
        return Err(SwapCompatibilityError::InvalidArtifactMetadata);
    }
    Ok(())
}

pub(super) fn ensure_transition_compatible(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> Result<(), SwapCompatibilityError> {
    if component.files().len() != 1
        || runtime_file_name(&component.files()[0])
            .is_none_or(|name| !name.eq_ignore_ascii_case(openvr::DLL_NAME))
    {
        return Err(SwapCompatibilityError::InvalidArtifactMetadata);
    }
    let installed = component.files()[0]
        .pe_compatibility()
        .ok_or(SwapCompatibilityError::MissingInstalledPeMetadata)?;
    let candidate = artifact.files()[0]
        .pe_compatibility()
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;

    if installed.architecture() != candidate.architecture() {
        return Err(SwapCompatibilityError::InstalledArchitectureMismatch {
            artifact: candidate.architecture(),
            installed: installed.architecture(),
        });
    }
    if !candidate
        .named_exports()
        .is_superset_of(installed.named_exports())
    {
        return Err(SwapCompatibilityError::ExportSurfaceMismatch);
    }
    Ok(())
}
