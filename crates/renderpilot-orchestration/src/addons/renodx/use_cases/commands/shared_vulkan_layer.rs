//! Commands for the shared ReShade Vulkan layer lifecycle.

use std::path::{Path, PathBuf};

use renderpilot_domain::{Architecture, SharedArtifactKind};

use crate::addons::renodx::dto::vulkan::VulkanLayerManagementReport;
use crate::addons::renodx::errors;
use crate::addons::renodx::matcher::ResolvedInstall;
use crate::addons::renodx::platform::vulkan::validation::{LayerMutationGate, layer_mutation_gate};
use crate::addons::renodx::vulkan::{self, VulkanLayerDetection};
use crate::addons::reshade::fetch::fetch_reshade_from_source;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::source::require_reshade_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

use super::update_reshade::PreparedReShadeUpdate;

pub(crate) struct PrepareInstallRequest<'a> {
    pub plan: &'a ResolvedInstall,
    pub reshade_config: &'a ReshadeSourceCatalog,
    pub channel: ReshadeChannel,
    pub allow_shared_vulkan_layer_install: bool,
    pub exe_path: Option<&'a Path>,
    pub progress: Option<&'a ProgressObserver<'a>>,
}

/// A shared-layer change prepared without mutation locks or forward writes.
pub(crate) enum PreparedInstallChange {
    NotNeeded,
    RegisterApp {
        exe_path: PathBuf,
        layer_dir: PathBuf,
    },
    Install {
        exe_path: PathBuf,
        source: crate::addons::reshade::source::ReshadeSource,
        download: crate::addons::reshade::fetch::Download,
        layer_dir: PathBuf,
    },
}

pub(crate) struct SharedLayerTransactionInput {
    pub(crate) plan: renderpilot_platform_windows::vulkan_layer::SharedVulkanLayerPlan,
    pub(crate) layer_dir: PathBuf,
    pub(crate) source: Option<(
        crate::addons::reshade::source::ReshadeSource,
        crate::addons::reshade::fetch::Download,
    )>,
}

impl PreparedInstallChange {
    #[must_use]
    pub(crate) fn mutates_shared_resource(&self) -> bool {
        !matches!(self, Self::NotNeeded)
    }

    pub(crate) fn resolve_locked_plan(
        &self,
    ) -> Result<
        Option<renderpilot_platform_windows::vulkan_layer::SharedVulkanLayerPlan>,
        ServiceError,
    > {
        let (layer_dir, exe_path, download) = match self {
            Self::NotNeeded => return Ok(None),
            Self::RegisterApp {
                layer_dir,
                exe_path,
                ..
            } => (layer_dir, exe_path, None),
            Self::Install {
                layer_dir,
                exe_path,
                download,
                ..
            } => (layer_dir, exe_path, Some(download.bytes.as_slice())),
        };
        let detection = require_mutable_layer(if download.is_some() {
            "installing"
        } else {
            "registering"
        })?;
        let registry = crate::addons::renodx::platform::vulkan::native_registry()
            .ok_or_else(errors::vulkan_unsupported_platform)?;
        let observation = renderpilot_platform_windows::vulkan_layer::observe_shared_vulkan_layer(
            registry, layer_dir,
        )
        .map_err(|error| {
            errors::failed(format!("failed to inspect shared Vulkan layer: {error}"))
        })?;
        if download.is_none() && detection != VulkanLayerDetection::Installed {
            return Err(errors::invalid(
                "the shared Vulkan layer changed before the final commit; retry the operation"
                    .to_owned(),
            ));
        }
        let plan = match download {
            None => renderpilot_platform_windows::vulkan_layer::plan_register_app_only(
                observation,
                exe_path,
            ),
            Some(bytes) => renderpilot_platform_windows::vulkan_layer::plan_install_and_register(
                observation,
                bytes,
                exe_path,
            ),
        }
        .map_err(|error| errors::failed(error.to_string()))?;
        Ok(Some(plan))
    }

    /// Moves the locked plan and any downloaded source into the combined
    /// transaction input without retaining a pre-lock platform plan.
    pub(crate) fn into_transaction_input(
        self,
        plan: renderpilot_platform_windows::vulkan_layer::SharedVulkanLayerPlan,
    ) -> Option<SharedLayerTransactionInput> {
        match self {
            Self::NotNeeded => None,
            Self::RegisterApp { layer_dir, .. } => Some(SharedLayerTransactionInput {
                plan,
                layer_dir,
                source: None,
            }),
            Self::Install {
                source,
                download,
                layer_dir,
                ..
            } => Some(SharedLayerTransactionInput {
                plan,
                layer_dir,
                source: Some((source, download)),
            }),
        }
    }
}

/// Resolves and downloads any shared Vulkan host change without writing the
/// layer, application registration, or advisory storage record.
pub(crate) async fn prepare_for_install(
    request: PrepareInstallRequest<'_>,
) -> Result<PreparedInstallChange, ServiceError> {
    let PrepareInstallRequest {
        plan,
        reshade_config,
        channel,
        allow_shared_vulkan_layer_install,
        exe_path,
        progress,
    } = request;
    if !matches!(plan.host_kind, HostKind::Vulkan) {
        return Ok(PreparedInstallChange::NotNeeded);
    }
    let exe_path = exe_path.map(Path::to_path_buf).ok_or_else(|| {
        errors::invalid(
            "cannot install RenoDX for this Vulkan game without a resolved executable".to_owned(),
        )
    })?;
    let detection = require_mutable_layer("installing")?;

    let layer_dir = vulkan::layer_dir().ok_or_else(errors::vulkan_unsupported_platform)?;
    if detection == VulkanLayerDetection::Installed {
        return Ok(PreparedInstallChange::RegisterApp {
            exe_path,
            layer_dir,
        });
    }

    // Remaining states after the gate above: NotInstalled, InstalledDisabled, or a
    // standard/mutable Conflict — all of which call for a fresh layer install.
    if !allow_shared_vulkan_layer_install {
        return Err(errors::invalid(
            "installing RenoDX for this Vulkan game adds a global ReShade Vulkan layer; \
             this caller did not permit installing it"
                .to_owned(),
        ));
    }
    if plan.arch != Architecture::X64 {
        return Err(errors::invalid(
            "RenderPilot can only install the shared Vulkan layer for x64 games in this version"
                .to_owned(),
        ));
    }
    let source = require_reshade_source(reshade_config, channel, plan.arch)?;
    let download = fetch_reshade_from_source(&source, plan.arch, progress).await?;
    Ok(PreparedInstallChange::Install {
        exe_path,
        source,
        download,
        layer_dir,
    })
}

fn require_mutable_layer(operation: &str) -> Result<VulkanLayerDetection, ServiceError> {
    let report = vulkan::layer_report();
    match layer_mutation_gate(&report) {
        LayerMutationGate::ExternalReadOnly => Err(errors::invalid(
            "a non-standard ReShade Vulkan layer is already registered; RenderPilot will not modify it"
                .to_owned(),
        )),
        LayerMutationGate::UnresolvedConflict => Err(errors::vulkan_layer_conflict(operation)),
        LayerMutationGate::Unsupported => Err(errors::vulkan_unsupported_platform()),
        LayerMutationGate::Proceed => Ok(report.detection()),
    }
}

/// Removes RenderPilot's shared ReShade Vulkan layer (a user maintenance action).
/// External layers are never touched. Per-game installs are left in place; they simply
/// stop loading until a layer is present again.
pub async fn remove_vulkan_layer(context: &Context) -> Result<(), ServiceError> {
    let _shared_guard =
        crate::mutation_boundary::enter_shared_only_mutation_boundary_async(context).await?;
    match layer_mutation_gate(&vulkan::layer_report()) {
        LayerMutationGate::ExternalReadOnly => {
            return Err(errors::invalid(
                "a non-standard ReShade Vulkan layer is already registered; RenderPilot will not modify it"
                    .to_owned(),
            ));
        }
        LayerMutationGate::UnresolvedConflict => {
            return Err(errors::vulkan_layer_conflict("removing"));
        }
        LayerMutationGate::Unsupported => return Err(errors::vulkan_unsupported_platform()),
        LayerMutationGate::Proceed => {}
    }
    let Some(layer_dir) = crate::addons::renodx::platform::vulkan::program_data::layer_dir() else {
        return Ok(());
    };
    let registry = crate::addons::renodx::platform::vulkan::native_registry()
        .ok_or_else(errors::vulkan_unsupported_platform)?;
    let observation = renderpilot_platform_windows::vulkan_layer::observe_shared_vulkan_layer(
        registry, &layer_dir,
    )
    .map_err(|error| errors::failed(format!("failed to inspect shared Vulkan layer: {error}")))?;
    let plan = renderpilot_platform_windows::vulkan_layer::plan_settings_remove(observation);
    let remove_empty_dir = plan.directory.remove_if_empty;
    let composed = crate::addons::shared_vulkan_mutation::compose(None, Some(plan))?;
    let id = ulid::Ulid::generate().to_string();
    let identity = crate::addons::shared_vulkan_mutation::MutationIdentity::new(
        &id,
        crate::addons::shared_vulkan_mutation::ScopeSpec::shared_only(),
        "shared_vulkan_remove",
    );
    let physical = crate::addons::shared_vulkan_mutation::PhysicalParticipants::new(
        crate::addons::shared_vulkan_mutation::TrustedRoots::shared_only(&layer_dir)?,
        composed,
        Some(registry),
    );
    let projection = crate::addons::shared_vulkan_mutation::CatalogProjection::new(
        renderpilot_storage_sqlite::SharedArtifactMutation::Delete(
            SharedArtifactKind::RenoDxVulkanLayer,
        ),
    );
    crate::addons::shared_vulkan_mutation::execute(
        crate::addons::shared_vulkan_mutation::Request::new(
            context, identity, physical, projection,
        ),
    )?;
    if remove_empty_dir {
        remove_empty_layer_dir_best_effort(&layer_dir);
    }
    Ok(())
}

/// Removes only the exact, planner-authorized layer directory after the
/// durable participant transaction. `remove_dir` is intentionally
/// non-recursive: a concurrent or previously unknown child makes cleanup a
/// harmless best-effort miss instead of granting deletion authority.
fn remove_empty_layer_dir_best_effort(path: &Path) {
    if let Err(error) = std::fs::remove_dir(path)
        && !matches!(
            error.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
        )
    {
        log::debug!(
            "shared Vulkan layer directory cleanup skipped for `{}`: {error}",
            path.display()
        );
    }
}

/// Applies the shared ReShade Vulkan layer for the requested channel and returns
/// a fresh settings-facing management report.
pub async fn apply_vulkan_layer(
    context: &Context,
    reshade_sources: &ReshadeSourceCatalog,
    channel: ReshadeChannel,
    safety: Option<crate::SharedVulkanSafetyPermit>,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<VulkanLayerManagementReport, ServiceError> {
    if !reshade_sources.supports_channel(channel) {
        return Err(errors::channel_unavailable(channel));
    }

    let prepared = PreparedReShadeUpdate::prepare(reshade_sources, channel, progress).await?;
    let guard =
        crate::mutation_boundary::enter_shared_only_mutation_boundary_async(context).await?;
    let prepared = prepared.plan_locked(context)?;
    match shared_permit_for_plan(&prepared.plan, safety.as_ref())? {
        None => {
            prepared.commit(context)?;
        }
        Some(safety) => {
            crate::FileSafetyAuthority::new().authorize_shared_vulkan_commit(
                renderpilot_domain::mutation_features::SHARED_VULKAN_APPLY,
                &guard,
                safety,
                || prepared.commit(context).map(|_| ()),
            )?;
        }
    }

    Ok(
        crate::addons::renodx::use_cases::queries::vulkan_layer::management_status(
            context,
            reshade_sources,
        )
        .await,
    )
}

fn shared_permit_for_plan<'a>(
    plan: &renderpilot_platform_windows::vulkan_layer::SharedVulkanLayerPlan,
    safety: Option<&'a crate::SharedVulkanSafetyPermit>,
) -> Result<Option<&'a crate::SharedVulkanSafetyPermit>, ServiceError> {
    if plan.is_noop() {
        return Ok(None);
    }
    safety
        .map(Some)
        .ok_or_else(|| ServiceError::safety_context_missing(crate::SafetyScope::SharedVulkan))
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::task::{Context as TaskContext, Poll, Waker};

    use crate::addons::renodx::test_support;
    use crate::{Context, ServiceError};

    use super::*;

    fn poll_ready<F: Future>(future: F) -> F::Output {
        let mut task_context = TaskContext::from_waker(Waker::noop());
        let mut future = Box::pin(future);
        match Future::poll(future.as_mut(), &mut task_context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("future unexpectedly reached an async boundary"),
        }
    }

    #[test]
    fn apply_vulkan_layer_rejects_unsupported_channel_before_update() {
        let mut reshade_sources = test_support::reshade_sources();
        reshade_sources.stable = None;
        let dir = tempfile::tempdir().expect("tempdir");
        let context = Context::open_at(dir.path().join("catalog.sqlite")).expect("context");
        let error = poll_ready(apply_vulkan_layer(
            &context,
            &reshade_sources,
            ReshadeChannel::Stable,
            None,
            None,
        ))
        .expect_err("unsupported channel should be rejected");

        match error {
            ServiceError::InvalidInput(message) => assert!(message.contains("stable")),
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    fn shared_plan(
        create_if_absent: bool,
    ) -> renderpilot_platform_windows::vulkan_layer::SharedVulkanLayerPlan {
        use renderpilot_platform_windows::vulkan_layer::{
            DirectoryMutation, DirectoryObservation, LayerPlanOperation, SharedVulkanLayerPlan,
        };

        SharedVulkanLayerPlan {
            operation: LayerPlanOperation::Refresh,
            files: Vec::new(),
            registry: None,
            directory: DirectoryMutation {
                path: std::path::PathBuf::from("shared"),
                before: DirectoryObservation {
                    exists: !create_if_absent,
                    entries: Vec::new(),
                },
                create_if_absent,
                remove_if_empty: false,
            },
            unregister_outcome: None,
        }
    }

    #[test]
    fn shared_physical_noop_does_not_require_a_safety_permit() {
        assert!(
            shared_permit_for_plan(&shared_plan(false), None)
                .expect("no-op must not require shared mutation authority")
                .is_none()
        );
    }

    #[test]
    fn shared_physical_mutation_still_requires_a_safety_permit() {
        let error = shared_permit_for_plan(&shared_plan(true), None)
            .expect_err("a physical shared mutation must require authority");
        assert!(matches!(
            error,
            ServiceError::SafetyContextMissing {
                scope: crate::SafetyScope::SharedVulkan
            }
        ));
    }
}
