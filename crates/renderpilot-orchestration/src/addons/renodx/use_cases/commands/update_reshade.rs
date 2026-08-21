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

/// Downloaded, validated ReShade payload. Preparing it performs no shared-layer
/// or storage writes; callers commit it synchronously under the shared guard.
pub struct PreparedReShadeUpdate {
    source: crate::addons::reshade::source::ReshadeSource,
    download: crate::addons::reshade::fetch::Download,
}

impl PreparedReShadeUpdate {
    /// Downloads and validates a ReShade payload without mutating shared state.
    pub async fn prepare(
        reshade_sources: &ReshadeSourceCatalog,
        channel: ReshadeChannel,
        progress: Option<&ProgressObserver<'_>>,
    ) -> Result<Self, ServiceError> {
        let source = require_reshade_source(reshade_sources, channel, Architecture::X64)?;
        let download = fetch_reshade_from_source(&source, Architecture::X64, progress).await?;
        Ok(Self { source, download })
    }

    /// Applies the prepared payload. The caller must invoke this only from an
    /// authority commit closure that holds the shared Vulkan guard.
    pub fn commit(self, context: &Context) -> Result<UpdateStatus, ServiceError> {
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

        let upstream_digest = self.download.digest.as_str();

        let actual_digest = vulkan::current_layer_digest();
        let db_digest = vulkan::stored_layer_digest(context.storage());
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
            vulkan::install_layer(&self.download.bytes)?;
        }

        vulkan::record_downloaded_layer(
            context.storage(),
            &self.source,
            &self.download,
            renderpilot_domain::SharedArtifactOrigin::RenderPilotCreated,
        )?;

        Ok(if changed {
            UpdateStatus::Available
        } else {
            UpdateStatus::Current
        })
    }
}
