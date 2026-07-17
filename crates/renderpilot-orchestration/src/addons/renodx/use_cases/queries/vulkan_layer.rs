/// Queries shared ReShade Vulkan layer status.
use crate::Context;
use crate::addons::renodx::dto::vulkan::VulkanLayerManagementReport;
use crate::addons::renodx::vulkan::{self, VulkanLayerReport};
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};

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
    reshade_sources: &ReshadeSourceCatalog,
) -> VulkanLayerManagementReport {
    let recorded_channel = vulkan::stored_layer_channel(context.storage());
    let default_channel = reshade_sources.default_install_channel();
    let selected_channel = recorded_channel.unwrap_or(default_channel);

    let update_verdict = if reshade_sources.supports_channel(selected_channel) {
        check_layer_update(context.storage(), reshade_sources, selected_channel).await
    } else {
        Some(
            crate::addons::renodx::platform::vulkan::validation::LayerUpdateVerdict {
                status: crate::addons::update::UpdateStatus::Unknown,
                diagnostics: Vec::new(),
            },
        )
    };

    VulkanLayerManagementReport {
        layer: vulkan::layer_report(),
        reshade_stable_supported: reshade_sources.supports_channel(ReshadeChannel::Stable),
        recorded_channel,
        default_channel,
        update_status: update_verdict.map(|v| v.status),
    }
}
