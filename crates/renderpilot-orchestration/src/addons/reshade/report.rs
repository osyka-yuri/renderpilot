//! Tool-agnostic core of the host-report: mapping a [`HostAssessment`]'s ReShade
//! host state to the shared observable [`HostFacts`]/[`HostDetection`] DTOs, the
//! update verdict, the channel-switch action, and the recorded channel.
//!
//! Each tool layers its own action set (which actions to offer, how to phrase
//! them) on top; RenoDX and Luma both consume these building blocks.

use renderpilot_domain::{InstalledAddon, TrackedSourceRole};

use super::channel;
use super::dto::{
    ActionDescriptor, ActionDisabledReason, HostActions, HostAddonSupport, HostChannelFacts,
    HostDetection, HostFacts, HostUpdateStatus,
};
use super::scan::{ReshadeAddonSupport, ReshadeHost, ReshadeHostAction, SlotActivity};
use super::types::{ReshadeChannel, ReshadeConfig};

pub(crate) fn host_detection(host: &ReshadeHost, conflict: bool) -> HostDetection {
    if conflict {
        return HostDetection::Conflict;
    }
    if host.as_present().is_some() {
        HostDetection::Present
    } else {
        HostDetection::Absent
    }
}

pub(crate) fn host_facts(
    host: &ReshadeHost,
    action: ReshadeHostAction,
    conflict: bool,
    is_custom_build: bool,
    detected_channel: Option<ReshadeChannel>,
    reshade_config: &ReshadeConfig,
) -> HostFacts {
    let selected = detected_channel
        .unwrap_or_else(|| reshade_config.effective_install_channel(ReshadeChannel::Stable));
    let effective = reshade_config.effective_install_channel(selected);
    let present = host.as_present();
    HostFacts {
        slot: present.map(|host| host.slot.to_owned()),
        active: present.is_some_and(|host| host.active.state == SlotActivity::Active),
        path: present.map(|host| host.path.to_path_buf()),
        version: present.and_then(|host| host.version.map(ToString::to_string)),
        addon_support: present
            .map(|host| match host.addon_support {
                ReshadeAddonSupport::Full => HostAddonSupport::Full,
                ReshadeAddonSupport::None => HostAddonSupport::Limited,
                ReshadeAddonSupport::Unknown => HostAddonSupport::Unknown,
            })
            .unwrap_or(HostAddonSupport::Unknown),
        channel: HostChannelFacts {
            selected,
            effective,
            detected: detected_channel,
        },
        update_status: host_update_status(host, action, conflict, detected_channel, effective),
        is_custom_build,
    }
}

pub(crate) fn host_update_status(
    host: &ReshadeHost,
    action: ReshadeHostAction,
    conflict: bool,
    detected_channel: Option<ReshadeChannel>,
    effective: ReshadeChannel,
) -> HostUpdateStatus {
    if conflict || action == ReshadeHostAction::Conflict {
        return HostUpdateStatus::UnknownNeedsValidation;
    }
    match action {
        ReshadeHostAction::ReinstallWithAddonSupport | ReshadeHostAction::RepairHost => {
            HostUpdateStatus::RepairAvailable
        }
        ReshadeHostAction::UpdateHost => {
            if host.as_present().is_some() {
                HostUpdateStatus::UpdateAvailable
            } else {
                HostUpdateStatus::UnknownNeedsValidation
            }
        }
        ReshadeHostAction::UpToDate => match detected_channel {
            Some(detected) if detected != effective => HostUpdateStatus::ChannelMismatch,
            Some(_) => HostUpdateStatus::Current,
            None => HostUpdateStatus::UnknownNeedsValidation,
        },
        ReshadeHostAction::Conflict => HostUpdateStatus::UnknownNeedsValidation,
    }
}

/// Whether the record's host binary provenance was reconstructed by adopting an
/// on-disk install RenderPilot did not create, rather than recorded from an
/// actual download. Gates whether an "Update" action is offered at all when the
/// host otherwise reads as up to date — a normal, freshly-downloaded host has
/// nothing to normalize, but an adopted one may still silently need to move onto
/// an upstream-verified build. Never gates *confirmation*.
pub(crate) fn has_advisory_host_source(record: Option<&InstalledAddon>) -> bool {
    record.is_some_and(|record| {
        record
            .tracked_sources()
            .iter()
            .any(|source| source.role() == TrackedSourceRole::HostBinary && source.is_advisory())
    })
}

/// Builds the channel-switch action toggling between the current and the other
/// channel, disabled with `StableUnavailable` when the manifest can't serve the
/// target.
pub(crate) fn switch_channel_action(
    current: ReshadeChannel,
    reshade_config: &ReshadeConfig,
) -> ActionDescriptor {
    let target = match current {
        ReshadeChannel::Stable => ReshadeChannel::Nightly,
        ReshadeChannel::Nightly => ReshadeChannel::Stable,
    };
    if reshade_config.supports_channel(target) {
        ActionDescriptor::enabled().with_target_channel(target)
    } else {
        ActionDescriptor::disabled(ActionDisabledReason::StableUnavailable)
            .with_target_channel(target)
    }
}

/// Maps a host assessment to the shared [`HostActions`] wire DTO.
pub(crate) fn host_action_core(
    host: &ReshadeHost,
    action: ReshadeHostAction,
    conflict: bool,
    is_custom_build: bool,
    record: Option<&InstalledAddon>,
    switch_channel: Option<ActionDescriptor>,
) -> HostActions {
    if is_custom_build {
        return HostActions::default();
    }
    if conflict || action == ReshadeHostAction::Conflict {
        return HostActions {
            resolve_conflict: Some(ActionDescriptor::disabled(
                ActionDisabledReason::BlockedByConflict,
            )),
            ..HostActions::default()
        };
    }
    match action {
        ReshadeHostAction::UpdateHost if host.as_present().is_none() => HostActions {
            install: Some(ActionDescriptor::enabled()),
            switch_channel,
            ..HostActions::default()
        },
        ReshadeHostAction::UpdateHost => HostActions {
            update: Some(ActionDescriptor::enabled()),
            switch_channel,
            ..HostActions::default()
        },
        ReshadeHostAction::ReinstallWithAddonSupport | ReshadeHostAction::RepairHost => {
            HostActions {
                repair: Some(ActionDescriptor::enabled()),
                switch_channel,
                ..HostActions::default()
            }
        }
        ReshadeHostAction::UpToDate => {
            let update = has_advisory_host_source(record).then(ActionDescriptor::enabled);
            HostActions {
                use_existing: host.as_present().map(|_| ActionDescriptor::enabled()),
                switch_channel,
                update,
                ..HostActions::default()
            }
        }
        ReshadeHostAction::Conflict => HostActions {
            resolve_conflict: Some(ActionDescriptor::disabled(
                ActionDisabledReason::BlockedByConflict,
            )),
            ..HostActions::default()
        },
    }
}

/// Builds detection, facts, and actions from an already-assessed host.
/// Tools only supply assessment inputs and optional channel-switch descriptor.
pub(crate) fn build_host_report_core(
    host: &ReshadeHost,
    action: ReshadeHostAction,
    conflict: bool,
    is_custom_build: bool,
    record: Option<&InstalledAddon>,
    reshade_config: &ReshadeConfig,
    switch_channel: Option<ActionDescriptor>,
) -> (HostDetection, HostFacts, HostActions) {
    let detected_channel = recorded_channel(record);
    (
        host_detection(host, conflict),
        host_facts(
            host,
            action,
            conflict,
            is_custom_build,
            detected_channel,
            reshade_config,
        ),
        host_action_core(
            host,
            action,
            conflict,
            is_custom_build,
            record,
            switch_channel,
        ),
    )
}

/// The ReShade channel recorded on the install's host binary artifact, if any. A
/// record with duplicate host sources or an unreadable channel degrades to `None`.
pub(crate) fn recorded_channel(record: Option<&InstalledAddon>) -> Option<ReshadeChannel> {
    record.and_then(|record| {
        if record.reshade_channel().is_some() {
            ReshadeChannel::parse_recorded(record.reshade_channel()).into_parsed()
        } else {
            channel::installed_channel(record)
                .ok()
                .flatten()
                .and_then(|c| c.into_parsed())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_mismatch_when_detected_channel_differs_from_effective() {
        let status = host_update_status(
            &ReshadeHost::Absent,
            ReshadeHostAction::UpToDate,
            false,
            Some(ReshadeChannel::Stable),
            ReshadeChannel::Nightly,
        );
        assert_eq!(status, HostUpdateStatus::ChannelMismatch);
    }

    #[test]
    fn channel_match_when_detected_equals_effective() {
        let status = host_update_status(
            &ReshadeHost::Absent,
            ReshadeHostAction::UpToDate,
            false,
            Some(ReshadeChannel::Stable),
            ReshadeChannel::Stable,
        );
        assert_eq!(status, HostUpdateStatus::Current);
    }
}
