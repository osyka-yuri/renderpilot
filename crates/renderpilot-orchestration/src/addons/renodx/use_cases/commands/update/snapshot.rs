//! Locked phase-one state resolution for RenoDX updates.

use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, TrackedSource, TrackedSourceRole,
};

use crate::addons::records::{self, source_with_role};
use crate::addons::renodx::errors;
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::renodx::use_cases::reshade_update::{
    HostUpdateTarget, recorded_reshade_channel, resolve_host_update_target,
};
use crate::addons::reshade::channel;
use crate::addons::reshade::types::{RecordedChannelParse, ReshadeChannel, ReshadeSourceCatalog};
use crate::{Context, ServiceError};

/// The exact installation state used to prepare an update outside the lock.
///
/// Tracked sources are owned copies: network preparation must not borrow data
/// that can be changed after the phase-one lock is released.
pub(super) struct UpdateSnapshot {
    pub(super) record: InstalledAddon,
    pub(super) shared_vulkan_channel: Option<ReshadeChannel>,
    pub(super) addon: Option<TrackedSource>,
    pub(super) host: Option<TrackedSource>,
    pub(super) host_target: Option<HostUpdateTarget>,
}

pub(super) fn resolve_update_snapshot(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
) -> Result<UpdateSnapshot, ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;
    if crate::addons::renodx::dlss_fix_binding::resolve(&record).main_payload_collides() {
        return Err(errors::invalid(
            "RenoDX main payload collides with the reserved DLSS-Fix companion target".to_owned(),
        ));
    }

    let shared_vulkan_host = matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    );
    let shared_vulkan_channel = if shared_vulkan_host {
        match recorded_reshade_channel(&record) {
            Some(channel) if reshade_sources.supports_channel(channel) => Some(channel),
            Some(_) => None,
            None => Some(reshade_sources.default_install_channel()),
        }
    } else {
        None
    };

    let addon = source_with_role(&record, TrackedSourceRole::AddonPayload).cloned();
    let host = match channel::single_host_source(&record) {
        Ok(host) => host.cloned(),
        Err(channel::ChannelReadIssue::DuplicateHostSources) => {
            return Err(errors::duplicate_host_sources());
        }
    };
    let host_channel = if shared_vulkan_host {
        None
    } else {
        match channel::installed_channel(&record).map_err(|_| errors::duplicate_host_sources())? {
            Some(channel) => Some(channel),
            None => host.as_ref().and_then(|source| {
                channel::infer_legacy_channel_from_url(source.url())
                    .map(RecordedChannelParse::Parsed)
            }),
        }
    };
    let host_target = match host_channel.and_then(|channel| channel.into_parsed()) {
        Some(channel) => {
            resolve_host_update_target(context, manifest, reshade_sources, game_id, channel)?
        }
        None => None,
    };
    if host_target.as_ref().is_some_and(|target| target.conflict) {
        return Err(errors::invalid(
            "ReShade host conflict must be resolved before updating RenoDX".to_owned(),
        ));
    }

    let host_policy_writes = host.as_ref().is_some_and(|_| {
        host_target
            .as_ref()
            .is_some_and(|target| target.action.writes_host())
    });
    let addon_tracked = addon
        .as_ref()
        .is_some_and(|source| !source.url().is_empty());
    if !addon_tracked && host.is_none() && !host_policy_writes && shared_vulkan_channel.is_none() {
        return Err(errors::invalid(
            "this RenoDX install has no recorded source to update from".to_owned(),
        ));
    }

    Ok(UpdateSnapshot {
        record,
        shared_vulkan_channel,
        addon,
        host,
        host_target,
    })
}

pub(super) fn ensure_update_snapshot_matches(
    snapshot: &UpdateSnapshot,
    current: &UpdateSnapshot,
) -> Result<(), ServiceError> {
    if snapshot.record != current.record
        || snapshot.shared_vulkan_channel != current.shared_vulkan_channel
        || snapshot.host_target != current.host_target
    {
        return Err(errors::state_changed_retry_update());
    }
    Ok(())
}
