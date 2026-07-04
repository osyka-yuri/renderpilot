/// Queries shared ReShade Vulkan layer status.
use crate::Context;
use crate::addons::renodx::dto::vulkan::VulkanLayerManagementReport;
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::renodx::vulkan::{self, VulkanLayerReport};
use crate::addons::reshade::types::ReshadeChannel;

/// Returns the current global ReShade Vulkan layer status.
#[must_use]
pub fn status() -> VulkanLayerReport {
    vulkan::layer_report()
}

use crate::addons::renodx::use_cases::queries::updates::check_layer_update;

/// Returns the settings-facing shared Vulkan layer management report.
#[must_use]
pub async fn management_status(
    context: &Context,
    manifest: &RenoDxManifest,
) -> VulkanLayerManagementReport {
    let recorded_channel = vulkan::stored_layer_channel(context.storage());
    let default_channel = manifest
        .reshade
        .effective_install_channel(ReshadeChannel::Stable);

    let effective_channel = recorded_channel.unwrap_or(default_channel);

    let update_verdict =
        check_layer_update(context.storage(), &manifest.reshade, effective_channel).await;

    VulkanLayerManagementReport {
        layer: vulkan::layer_report(),
        reshade_stable_supported: manifest.reshade.supports_channel(ReshadeChannel::Stable),
        recorded_channel,
        default_channel,
        update_status: update_verdict.map(|v| v.status),
    }
}
