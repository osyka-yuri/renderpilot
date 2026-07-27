use renderpilot_domain::{GraphicsTechnology, LibraryArtifact, UpstreamPackageProvider};

use crate::dxc::PACKAGE_FILE_NAMES;

use super::{SwapCompatibilityError, SwapTargetProfile, runtime_file_name};

pub(super) const DXC_PACKAGE_ID: &str = "Microsoft.Direct3D.DXC";
pub(super) const D3D12_PACKAGE_ID: &str = "Microsoft.Direct3D.D3D12";

pub(super) fn validate_dxc_artifact(
    artifact: &LibraryArtifact,
) -> Result<(), SwapCompatibilityError> {
    let target = artifact
        .metadata()
        .runtime_target()
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    let package = artifact
        .metadata()
        .upstream_package()
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    if package.provider() != UpstreamPackageProvider::NuGet
        || !package.id().eq_ignore_ascii_case(DXC_PACKAGE_ID)
        || target.compatibility().is_some()
    {
        return Err(SwapCompatibilityError::InvalidArtifactMetadata);
    }

    if has_complete_dxc_pair(artifact.files()) {
        Ok(())
    } else {
        Err(SwapCompatibilityError::IncompleteDxcPackage)
    }
}

pub(super) fn validate_d3d12_artifact(
    artifact: &LibraryArtifact,
) -> Result<(), SwapCompatibilityError> {
    if artifact.files().len() != 1
        || runtime_file_name(&artifact.files()[0])
            .is_none_or(|name| !name.eq_ignore_ascii_case("D3D12Core.dll"))
    {
        return Err(SwapCompatibilityError::InvalidArtifactMetadata);
    }
    let target = artifact
        .metadata()
        .runtime_target()
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    let version = target
        .compatibility()
        .and_then(renderpilot_domain::RuntimeCompatibility::as_d3d12_sdk_version)
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    if let Some(package) = artifact.metadata().upstream_package() {
        let package_line = package
            .version()
            .numeric_core()
            .segments()
            .get(1)
            .copied()
            .and_then(|line| u32::try_from(line).ok());
        if package.provider() != UpstreamPackageProvider::NuGet
            || !package.id().eq_ignore_ascii_case(D3D12_PACKAGE_ID)
            || package_line != Some(version)
        {
            return Err(SwapCompatibilityError::InvalidArtifactMetadata);
        }
    }
    Ok(())
}

fn has_complete_dxc_pair(files: &[renderpilot_domain::ComponentFile]) -> bool {
    files.len() == PACKAGE_FILE_NAMES.len()
        && PACKAGE_FILE_NAMES.iter().all(|expected| {
            files
                .iter()
                .filter_map(runtime_file_name)
                .any(|actual| actual.eq_ignore_ascii_case(expected))
        })
}

pub(super) fn ensure_executable_compatible(
    artifact: &LibraryArtifact,
    profile: &SwapTargetProfile,
) -> Result<(), SwapCompatibilityError> {
    let Some(target) = artifact.metadata().runtime_target() else {
        return Ok(());
    };
    let executable = profile
        .architecture()
        .ok_or(SwapCompatibilityError::MissingTargetArchitecture)?;
    if target.architecture() != executable {
        return Err(SwapCompatibilityError::ArchitectureMismatch {
            artifact: target.architecture(),
            executable,
        });
    }

    if artifact.technology() != GraphicsTechnology::D3D12Agility {
        return Ok(());
    }

    let version = target
        .compatibility()
        .and_then(renderpilot_domain::RuntimeCompatibility::as_d3d12_sdk_version)
        .ok_or(SwapCompatibilityError::InvalidArtifactMetadata)?;
    let requested = profile
        .d3d12_sdk_version()
        .ok_or(SwapCompatibilityError::MissingD3d12SdkVersion)?;
    if profile.d3d12_executable().is_some() {
        // Managed D3D12 transitions are governed by the original-line policy
        // and explicit executable assessment in the shared compatibility layer.
        return Ok(());
    }
    if version == requested {
        Ok(())
    } else {
        Err(SwapCompatibilityError::D3d12SdkMismatch {
            artifact: version,
            executable: requested,
        })
    }
}
