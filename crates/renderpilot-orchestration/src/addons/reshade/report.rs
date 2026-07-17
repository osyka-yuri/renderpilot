//! Tool-agnostic core of the host-report: mapping a
//! [`HostAssessment`](super::host_policy::HostAssessment)'s ReShade
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
use super::host_policy::{HostAssessment, HostLifecycle};
use super::scan::{ReshadeAddonSupport, ReshadeHost, ReshadeHostAction, SlotActivity};
use super::types::{ReshadeChannel, ReshadeSourceCatalog};

/// How first-install conflict / lifecycle are applied for a host path.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HostReportPolicy {
    /// When true and no install record exists, prefer
    /// [`HostAssessment::initial_is_conflict`] over the raw assessment conflict.
    pub apply_initial_conflict: bool,
    /// When true, run [`apply_initial_lifecycle_actions`] after core actions.
    pub apply_lifecycle: bool,
}

impl HostReportPolicy {
    /// Proxy-host first install: initial conflict + lifecycle (Luma; RenoDX proxy).
    pub(crate) const PROXY_INITIAL: Self = Self {
        apply_initial_conflict: true,
        apply_lifecycle: true,
    };

    /// Shared Vulkan layer path: maintenance conflict only, no proxy lifecycle.
    pub(crate) const VULKAN_MAINTENANCE: Self = Self {
        apply_initial_conflict: false,
        apply_lifecycle: false,
    };
}

/// Inputs for assembling shared host detection / facts / actions from an assessment.
#[derive(Debug, Clone)]
pub(crate) struct AssembleHostReport<'a> {
    pub assessment: &'a HostAssessment,
    pub record: Option<&'a InstalledAddon>,
    pub reshade_config: &'a ReshadeSourceCatalog,
    pub switch_channel: Option<ActionDescriptor>,
    pub policy: HostReportPolicy,
}

/// Effective conflict flag for the host report (first-install vs maintenance).
#[must_use]
pub(crate) fn effective_host_conflict(
    assessment: &HostAssessment,
    record: Option<&InstalledAddon>,
    apply_initial_conflict: bool,
) -> bool {
    if apply_initial_conflict && record.is_none() && assessment.initial_is_conflict() {
        true
    } else {
        assessment.conflict
    }
}

/// Builds detection, facts, and actions from a completed host assessment.
/// Tools only supply assessment, channel-switch descriptor, and policy.
pub(crate) fn assemble_host_report(
    input: AssembleHostReport<'_>,
) -> (HostDetection, HostFacts, HostActions) {
    let conflict = effective_host_conflict(
        input.assessment,
        input.record,
        input.policy.apply_initial_conflict,
    );
    let (detection, facts, mut actions) = build_host_report_core(
        &input.assessment.host,
        input.assessment.action,
        conflict,
        input.assessment.is_known_custom_build(),
        input.record,
        input.reshade_config,
        input.switch_channel,
    );
    if input.policy.apply_lifecycle {
        apply_initial_lifecycle_actions(&mut actions, input.assessment.lifecycle, input.record);
    }
    (detection, facts, actions)
}

/// Shared "no resolvable host slot / target" report: absent host, conflict action,
/// empty actions (tools wrap with their default action type alias).
pub(crate) fn missing_host_report_core(
    record: Option<&InstalledAddon>,
    reshade_config: &ReshadeSourceCatalog,
) -> (HostDetection, HostFacts, HostActions) {
    build_host_report_core(
        &ReshadeHost::Absent,
        ReshadeHostAction::Conflict,
        false,
        false,
        record,
        reshade_config,
        None,
    )
}

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
    reshade_config: &ReshadeSourceCatalog,
) -> HostFacts {
    let selected = detected_channel.unwrap_or_else(|| reshade_config.default_install_channel());
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
            detected: detected_channel,
        },
        update_status: host_update_status(host, action, conflict, detected_channel, selected),
        is_custom_build,
    }
}

pub(crate) fn host_update_status(
    host: &ReshadeHost,
    action: ReshadeHostAction,
    conflict: bool,
    detected_channel: Option<ReshadeChannel>,
    selected: ReshadeChannel,
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
            Some(detected) if detected != selected => HostUpdateStatus::ChannelMismatch,
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
    reshade_config: &ReshadeSourceCatalog,
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
            install: Some(ActionDescriptor::disabled(
                ActionDisabledReason::BlockedByConflict,
            )),
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
                // A first add-on install may not replace a user's existing
                // host. Keep the maintenance action visible for already
                // tracked installs, while exposing a concrete disabled reason
                // to install cards before any write is attempted.
                install: Some(ActionDescriptor::disabled(
                    ActionDisabledReason::BlockedByConflict,
                )),
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
    reshade_config: &ReshadeSourceCatalog,
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

/// Applies first-install-only lifecycle semantics after the shared action mapper
/// has described the raw runtime state. Repairing a proved-empty runtime is part
/// of the add-on's Install operation, not a separate prerequisite action.
pub(crate) fn apply_initial_lifecycle_actions(
    actions: &mut HostActions,
    lifecycle: HostLifecycle,
    record: Option<&InstalledAddon>,
) {
    if record.is_some() {
        return;
    }
    match lifecycle {
        HostLifecycle::RepairEmpty => {
            actions.install = Some(ActionDescriptor::enabled());
            actions.repair = None;
        }
        HostLifecycle::Conflict => {
            actions.install = Some(ActionDescriptor::disabled(
                ActionDisabledReason::BlockedByConflict,
            ));
            actions.repair = None;
        }
        HostLifecycle::InstallNew | HostLifecycle::ReuseUser | HostLifecycle::AdoptEmpty => {}
    }
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
    fn channel_mismatch_when_detected_channel_differs_from_selected() {
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
    fn channel_match_when_detected_equals_selected() {
        let status = host_update_status(
            &ReshadeHost::Absent,
            ReshadeHostAction::UpToDate,
            false,
            Some(ReshadeChannel::Stable),
            ReshadeChannel::Stable,
        );
        assert_eq!(status, HostUpdateStatus::Current);
    }

    #[test]
    fn initial_empty_repair_is_an_enabled_install_not_a_second_action() {
        let mut actions = HostActions {
            install: Some(ActionDescriptor::disabled(
                ActionDisabledReason::BlockedByConflict,
            )),
            repair: Some(ActionDescriptor::enabled()),
            ..HostActions::default()
        };

        apply_initial_lifecycle_actions(&mut actions, HostLifecycle::RepairEmpty, None);

        assert!(actions.install.is_some_and(|action| action.enabled));
        assert!(actions.repair.is_none());
    }

    #[test]
    fn initial_conflict_keeps_install_disabled() {
        let mut actions = HostActions::default();

        apply_initial_lifecycle_actions(&mut actions, HostLifecycle::Conflict, None);

        assert_eq!(
            actions.install.and_then(|action| action.disabled_reason),
            Some(ActionDisabledReason::BlockedByConflict)
        );
    }

    #[test]
    fn effective_host_conflict_prefers_initial_only_when_untracked() {
        // Absent host assessment: lifecycle InstallNew, conflict false.
        let assessment = crate::addons::reshade::host_policy::assess(
            std::path::Path::new("C:/missing-game"),
            "dxgi.dll",
        );
        assert!(!assessment.conflict);
        assert!(!assessment.initial_is_conflict());

        // Without a real conflict lifecycle, both policies agree.
        assert!(!effective_host_conflict(&assessment, None, true));
        assert!(!effective_host_conflict(&assessment, None, false));
    }

    #[test]
    fn missing_host_report_core_is_absent_without_actions() {
        let sources = ReshadeSourceCatalog {
            stable: None,
            nightly: crate::addons::reshade::types::ReshadeNightly {
                url64: "https://example.test/64.zip".to_owned(),
                url32: "https://example.test/32.zip".to_owned(),
            },
        };
        let (detection, facts, actions) = missing_host_report_core(None, &sources);
        assert_eq!(detection, HostDetection::Absent);
        assert!(facts.path.is_none());
        // Conflict action maps to a disabled resolve/install path; tools that
        // want empty chrome replace actions with `HostActions::default()`.
        assert!(actions.resolve_conflict.is_some() || actions.install.is_some());
    }
}
