//! Commands for the shared ReShade Vulkan layer lifecycle.

use std::path::{Path, PathBuf};

use renderpilot_domain::{Architecture, SharedArtifactOrigin};

use crate::addons::renodx::dto::vulkan::VulkanLayerManagementReport;
use crate::addons::renodx::errors;
use crate::addons::renodx::matcher::ResolvedInstall;
use crate::addons::renodx::platform::vulkan::validation::{LayerMutationGate, layer_mutation_gate};
use crate::addons::renodx::vulkan::{self, VulkanLayerDetection};
use crate::addons::reshade::fetch::fetch_reshade_from_source;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::source::require_reshade_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::addons::vulkan_lock;
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
        channel: ReshadeChannel,
    },
    Install {
        exe_path: PathBuf,
        source: crate::addons::reshade::source::ReshadeSource,
        download: crate::addons::reshade::fetch::Download,
        installing_from_scratch: bool,
    },
}

impl PreparedInstallChange {
    #[must_use]
    pub(crate) const fn mutates_shared_resource(&self) -> bool {
        !matches!(self, Self::NotNeeded)
    }

    /// Applies the prepared change. The caller must invoke this from the
    /// combined game/shared authority commit closure.
    pub(crate) fn commit(self, context: &Context) -> Result<bool, ServiceError> {
        match self {
            Self::NotNeeded => Ok(false),
            Self::RegisterApp { exe_path, channel } => {
                vulkan::register_app(&exe_path)?;
                record_shared_layer_best_effort(vulkan::record_detected_layer(
                    context.storage(),
                    SharedArtifactOrigin::AdoptedOfficial,
                    Some(channel),
                ));
                Ok(false)
            }
            Self::Install {
                exe_path,
                source,
                download,
                installing_from_scratch,
            } => {
                if let Err(error) = vulkan::install_layer(&download.bytes) {
                    if installing_from_scratch {
                        cleanup_after_failed_install();
                    }
                    return Err(error);
                }
                if let Err(error) = vulkan::register_app(&exe_path) {
                    if installing_from_scratch {
                        cleanup_after_failed_install();
                    }
                    return Err(error);
                }
                record_shared_layer_best_effort(vulkan::record_downloaded_layer(
                    context.storage(),
                    &source,
                    &download,
                    SharedArtifactOrigin::RenderPilotCreated,
                ));
                Ok(true)
            }
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
    let report = vulkan::layer_report();
    match layer_mutation_gate(&report) {
        LayerMutationGate::ExternalReadOnly => {
            return Err(errors::invalid(
                "a non-standard ReShade Vulkan layer is already registered; RenderPilot will not modify it"
                    .to_owned(),
            ));
        }
        LayerMutationGate::UnresolvedConflict => {
            return Err(errors::vulkan_layer_conflict("installing"));
        }
        LayerMutationGate::Unsupported => return Err(errors::vulkan_unsupported_platform()),
        LayerMutationGate::Proceed => {}
    }

    if matches!(report.detection(), VulkanLayerDetection::Installed) {
        return Ok(PreparedInstallChange::RegisterApp { exe_path, channel });
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
    // Nothing existed before this attempt, so a failure partway through should
    // try to leave nothing behind rather than stranding a partial install that
    // the next detection pass would report as a Conflict.
    let installing_from_scratch = matches!(
        report.detection(),
        VulkanLayerDetection::NotInstalled | VulkanLayerDetection::InstalledDisabled
    );
    Ok(PreparedInstallChange::Install {
        exe_path,
        source,
        download,
        installing_from_scratch,
    })
}

/// Best-effort cleanup after a from-scratch layer install fails partway
/// through. The original error is what the caller returns; a cleanup failure
/// is logged rather than silently dropped, since it means the layer is left
/// in a state the next detection pass will report as a Conflict.
fn cleanup_after_failed_install() {
    if let Err(error) = vulkan::remove_layer() {
        log::warn!("failed to clean up a partially installed Vulkan layer: {error}");
    }
}

fn record_shared_layer_best_effort(result: Result<(), ServiceError>) {
    if let Err(error) = result {
        log::warn!("failed to persist advisory Vulkan layer record: {error}");
    }
}

/// Removes RenderPilot's shared ReShade Vulkan layer (a user maintenance action).
/// External layers are never touched. Per-game installs are left in place; they simply
/// stop loading until a layer is present again.
pub async fn remove_vulkan_layer(context: &Context) -> Result<(), ServiceError> {
    let _shared_guard = vulkan_lock::shared_vulkan_lock().await;
    vulkan::remove_layer()?;
    vulkan::forget_layer_record(context.storage());
    Ok(())
}

/// Applies the shared ReShade Vulkan layer for the requested channel and returns
/// a fresh settings-facing management report.
pub async fn apply_vulkan_layer(
    context: &Context,
    reshade_sources: &ReshadeSourceCatalog,
    channel: ReshadeChannel,
    safety: crate::SharedVulkanSafetyPermit,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<VulkanLayerManagementReport, ServiceError> {
    if !reshade_sources.supports_channel(channel) {
        return Err(errors::channel_unavailable(channel));
    }

    let prepared = PreparedReShadeUpdate::prepare(reshade_sources, channel, progress).await?;
    let guard = vulkan_lock::shared_vulkan_lock().await;
    crate::FileSafetyAuthority::new().authorize_shared_vulkan_commit(
        renderpilot_domain::mutation_features::SHARED_VULKAN_APPLY,
        &guard,
        &safety,
        || prepared.commit(context).map(|_| ()),
    )?;

    Ok(
        crate::addons::renodx::use_cases::queries::vulkan_layer::management_status(
            context,
            reshade_sources,
        )
        .await,
    )
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
        let authority = crate::FileSafetyAuthority::new();
        let assessment = authority
            .issue_shared_vulkan_assessment()
            .expect("assessment");
        let safety = authority
            .shared_vulkan_permit(Some(&assessment.context_token))
            .expect("permit");

        let error = poll_ready(apply_vulkan_layer(
            &context,
            &reshade_sources,
            ReshadeChannel::Stable,
            safety,
            None,
        ))
        .expect_err("unsupported channel should be rejected");

        match error {
            ServiceError::InvalidInput(message) => assert!(message.contains("stable")),
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }
}
