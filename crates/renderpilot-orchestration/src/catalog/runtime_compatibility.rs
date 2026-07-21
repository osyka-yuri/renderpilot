//! Resolves executable facts and enforces Microsoft runtime compatibility.

use std::path::Path;

use renderpilot_application::{
    AppError, AppResult, SwapTargetProfile, ensure_replacement_compatible,
};
use renderpilot_domain::{GameInstallation, GraphicsComponent, LibraryArtifact};

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

pub(super) fn ensure_transition_compatible(
    context: &Context,
    game: &GameInstallation,
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> AppResult<()> {
    let profile = target_profile(context, game);
    ensure_replacement_compatible(component, artifact, &profile).map_err(|error| {
        AppError::invalid_input(format!("runtime artifact is incompatible: {error}"))
    })
}
