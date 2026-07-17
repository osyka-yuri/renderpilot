use renderpilot_domain::Architecture;

use crate::Context;
use crate::ServiceError;
use crate::addons::renodx::errors;
use crate::addons::renodx::platform::vulkan::validation::{
    LayerMutationGate, layer_mutation_gate, resolve_digest_verdict,
};
use crate::addons::renodx::vulkan;
use crate::addons::renodx::vulkan::VulkanLayerDetection;
use crate::addons::reshade::fetch::fetch_reshade_from_source;
use crate::addons::reshade::source::require_reshade_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::addons::update::UpdateStatus;
use crate::net::ProgressObserver;

/// Command to update the ReShade Vulkan layer.
pub struct UpdateReShadeCommand<'a> {
    /// The application context.
    pub context: &'a Context,
    /// Independently resolved ReShade sources.
    pub reshade_sources: &'a ReshadeSourceCatalog,
    /// The ReShade release channel to fetch from.
    pub channel: ReshadeChannel,
    /// Optional progress observer for the download.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

impl<'a> UpdateReShadeCommand<'a> {
    /// Executes the update command.
    pub async fn execute(self) -> Result<UpdateStatus, ServiceError> {
        let _guard = crate::addons::vulkan_lock::shared_vulkan_lock().await;

        let report = vulkan::layer_report();
        match layer_mutation_gate(&report) {
            LayerMutationGate::ExternalReadOnly => {
                return Err(errors::invalid(
                    "the visible Vulkan ReShade layer is external; RenderPilot will not modify it"
                        .to_owned(),
                ));
            }
            LayerMutationGate::UnresolvedConflict => {
                return Err(errors::vulkan_layer_conflict("updating"));
            }
            LayerMutationGate::Unsupported => return Err(errors::vulkan_unsupported_platform()),
            LayerMutationGate::Proceed => {}
        }

        let source = require_reshade_source(self.reshade_sources, self.channel, Architecture::X64)?;

        let download = fetch_reshade_from_source(&source, Architecture::X64, self.progress).await?;
        let upstream_digest = download.digest.as_str();

        let actual_digest = vulkan::current_layer_digest();
        let db_digest = vulkan::stored_layer_digest(self.context.storage());
        let verdict = resolve_digest_verdict(
            actual_digest.as_deref(),
            db_digest.as_deref(),
            upstream_digest,
        );
        let needs_layer_write = matches!(
            report.detection(),
            VulkanLayerDetection::NotInstalled
                | VulkanLayerDetection::Conflict
                | VulkanLayerDetection::InstalledDisabled
        );
        let changed = needs_layer_write || verdict.status != UpdateStatus::Current;

        if changed {
            vulkan::install_layer(&download.bytes)?;
        }

        vulkan::record_downloaded_layer(
            self.context.storage(),
            &source,
            &download,
            renderpilot_domain::SharedArtifactOrigin::RenderPilotCreated,
        )?;

        Ok(if changed {
            UpdateStatus::Available
        } else {
            UpdateStatus::Current
        })
    }
}
