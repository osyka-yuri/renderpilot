//! Locked resolution and revalidation for channel switching.

use renderpilot_domain::{AddonKind, GameId, InstalledAddon, InstalledAddonHostKind};

use crate::addons::records;
use crate::addons::renodx::errors;
use crate::addons::renodx::tracking;
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::renodx::use_cases::reshade_update::{
    HostUpdateTarget, recorded_reshade_channel, resolve_host_update_target,
};
use crate::addons::reshade::channel;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::{Context, ServiceError};

pub(super) enum ChannelSwitchPhase1 {
    Healed(InstalledAddon),
    SharedVulkan { record: InstalledAddon },
    Proxy(ProxyChannelSwitchSnapshot),
}

pub(super) struct ProxyChannelSwitchSnapshot {
    pub(super) record: InstalledAddon,
    pub(super) target: HostUpdateTarget,
    pub(super) target_channel: ReshadeChannel,
}

pub(super) fn resolve_channel_switch_phase1(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
    target_channel: ReshadeChannel,
) -> Result<ChannelSwitchPhase1, ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;
    if matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    ) {
        return Ok(ChannelSwitchPhase1::SharedVulkan { record });
    }

    let host_source =
        channel::single_host_source(&record).map_err(|_| errors::duplicate_host_sources())?;
    let current = recorded_reshade_channel(&record);
    if current == Some(target_channel) {
        // Resolve metadata repair without writing it. The caller persists it
        // only after the final guard-bound safety validation.
        let healed = if let Some(host_source) = host_source {
            let healed_source = channel::with_host_channel(host_source, target_channel);
            tracking::replace_host_source(&record, &healed_source)?
        } else {
            record.with_reshade_channel(target_channel.as_str())
        }
        .with_reshade_channel(target_channel.as_str());
        return Ok(ChannelSwitchPhase1::Healed(healed));
    }

    // `resolve_host_update_target` returns `None` for recognized custom builds;
    // RenoDX does not manage their channel.
    let target =
        resolve_host_update_target(context, manifest, reshade_sources, game_id, target_channel)?
            .ok_or_else(|| {
                errors::invalid("cannot resolve the ReShade proxy slot for this game".to_owned())
            })?;
    if target.conflict {
        return Err(errors::invalid(
            "ReShade host conflict must be resolved before switching channel".to_owned(),
        ));
    }
    if !target.target_path.is_file() {
        return Err(errors::invalid(
            "ReShade host binary is missing; repair it before switching channel".to_owned(),
        ));
    }

    Ok(ChannelSwitchPhase1::Proxy(ProxyChannelSwitchSnapshot {
        record,
        target,
        target_channel,
    }))
}

pub(super) fn ensure_proxy_channel_switch_matches(
    snapshot: &ProxyChannelSwitchSnapshot,
    current: &ProxyChannelSwitchSnapshot,
) -> Result<(), ServiceError> {
    if snapshot.record != current.record
        || snapshot.target != current.target
        || snapshot.target_channel != current.target_channel
    {
        return Err(errors::state_changed_retry_update());
    }
    Ok(())
}

pub(super) fn ensure_target_channel(
    reshade_sources: &ReshadeSourceCatalog,
    target_channel: ReshadeChannel,
) -> Result<(), ServiceError> {
    if reshade_sources.supports_channel(target_channel) {
        Ok(())
    } else {
        Err(errors::channel_unavailable(target_channel))
    }
}
