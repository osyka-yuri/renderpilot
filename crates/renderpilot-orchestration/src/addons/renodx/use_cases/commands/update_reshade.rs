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

pub(crate) struct PreparedSharedVulkanUpdate {
    pub(crate) layer_dir: std::path::PathBuf,
    pub(crate) plan: renderpilot_platform_windows::vulkan_layer::SharedVulkanLayerPlan,
    pub(crate) shared_record: renderpilot_domain::SharedArtifactRecord,
    pub(crate) changed: bool,
}

impl PreparedSharedVulkanUpdate {
    /// Applies this final, lock-derived plan through the durable coordinator.
    pub(crate) fn commit(self, context: &Context) -> Result<UpdateStatus, ServiceError> {
        let Self {
            layer_dir,
            plan,
            shared_record,
            changed,
        } = self;
        let composed = crate::addons::shared_vulkan_mutation::compose(None, Some(plan))?;
        let roots = crate::addons::shared_vulkan_mutation::TrustedRoots::shared_only(&layer_dir)?;
        let registry = crate::addons::renodx::platform::vulkan::native_registry()
            .ok_or_else(errors::vulkan_unsupported_platform)?;
        let mutation_id = ulid::Ulid::generate().to_string();
        let identity = crate::addons::shared_vulkan_mutation::MutationIdentity::new(
            &mutation_id,
            crate::addons::shared_vulkan_mutation::ScopeSpec::shared_only(),
            "renodx_shared_vulkan_refresh",
        );
        let physical = crate::addons::shared_vulkan_mutation::PhysicalParticipants::new(
            roots,
            composed,
            Some(registry),
        );
        let projection = crate::addons::shared_vulkan_mutation::CatalogProjection::new(
            renderpilot_storage_sqlite::SharedArtifactMutation::Upsert(&shared_record),
        );
        crate::addons::shared_vulkan_mutation::execute(
            crate::addons::shared_vulkan_mutation::Request::new(
                context, identity, physical, projection,
            ),
        )?;

        Ok(if changed {
            UpdateStatus::Available
        } else {
            UpdateStatus::Current
        })
    }
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
    pub(crate) fn plan_locked(
        &self,
        context: &Context,
    ) -> Result<PreparedSharedVulkanUpdate, ServiceError> {
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

        let layer_dir = vulkan::layer_dir().ok_or_else(errors::vulkan_unsupported_platform)?;
        let registry = crate::addons::renodx::platform::vulkan::native_registry()
            .ok_or_else(errors::vulkan_unsupported_platform)?;
        let observation = renderpilot_platform_windows::vulkan_layer::observe_shared_vulkan_layer(
            registry, &layer_dir,
        )
        .map_err(|error| {
            errors::failed(format!("failed to inspect shared Vulkan layer: {error}"))
        })?;
        let plan = renderpilot_platform_windows::vulkan_layer::plan_refresh(
            observation,
            &self.download.bytes,
        )
        .map_err(|error| errors::failed(error.to_string()))?;
        let shared_record =
            crate::addons::renodx::platform::vulkan::shared_artifact::downloaded_record(
                &layer_dir,
                &self.source,
                &self.download,
            )?;
        Ok(PreparedSharedVulkanUpdate {
            layer_dir,
            plan,
            shared_record,
            changed,
        })
    }
}
