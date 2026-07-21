//! Resolves executable facts and enforces Microsoft runtime compatibility.

use std::path::{Path, PathBuf};

use renderpilot_application::{AppError, AppResult, SwapTargetProfile, ensure_swap_compatible};
use renderpilot_domain::{GameInstallation, GraphicsTechnology, LibraryArtifact};

use crate::Context;

const D3D12_SDK_VERSION_EXPORT: &str = "D3D12SDKVersion";

pub(super) fn target_profile(context: &Context, game: &GameInstallation) -> SwapTargetProfile {
    let override_path = crate::addons::game_context::executable_override(context, game.id());
    let resolved = crate::game_executable::resolve_primary_executable(
        Path::new(game.install_path().as_str()),
        override_path.as_deref(),
        true,
    );
    let architecture = resolved
        .as_ref()
        .and_then(|executable| executable.graphics.architecture());
    let d3d12_sdk_version = resolved.as_ref().and_then(|executable| {
        renderpilot_detection::read_pe_exported_u32(
            Path::new(executable.path.as_str()),
            D3D12_SDK_VERSION_EXPORT,
        )
    });
    SwapTargetProfile::new(architecture, d3d12_sdk_version)
}

pub(super) fn ensure_artifact_compatible(
    context: &Context,
    game: &GameInstallation,
    artifact: &LibraryArtifact,
    inspect_sources: bool,
) -> AppResult<()> {
    let profile = target_profile(context, game);
    ensure_swap_compatible(artifact, &profile).map_err(|error| {
        AppError::invalid_input(format!("runtime artifact is incompatible: {error}"))
    })?;

    if inspect_sources && artifact.metadata().runtime_target().is_some() {
        inspect_artifact_sources(artifact)?;
    }
    Ok(())
}

fn inspect_artifact_sources(artifact: &LibraryArtifact) -> AppResult<()> {
    let target = artifact
        .metadata()
        .runtime_target()
        .ok_or_else(|| AppError::invalid_input("runtime artifact lacks target metadata"))?;

    for file in artifact.files() {
        let source = PathBuf::from(file.path().as_str());
        let inspection = renderpilot_detection::inspect_pe(&source).ok_or_else(|| {
            AppError::invalid_input(format!(
                "cannot inspect runtime source {}",
                file.path().as_str()
            ))
        })?;
        if inspection.architecture != Some(target.architecture()) {
            return Err(AppError::invalid_input(format!(
                "runtime source architecture mismatch at {}",
                file.path().as_str()
            )));
        }

        if artifact.technology() == GraphicsTechnology::D3D12Agility {
            let declared = target
                .compatibility()
                .and_then(renderpilot_domain::RuntimeCompatibility::as_d3d12_sdk_version)
                .ok_or_else(|| AppError::invalid_input("D3D12 artifact lacks SDK line"))?;
            let observed = inspection
                .version
                .as_ref()
                .and_then(|version| version.segments().get(1))
                .and_then(|segment| u32::try_from(*segment).ok());
            if observed != Some(declared) {
                return Err(AppError::invalid_input(
                    "D3D12 artifact SDK line does not match its PE version",
                ));
            }
        }
    }
    Ok(())
}
