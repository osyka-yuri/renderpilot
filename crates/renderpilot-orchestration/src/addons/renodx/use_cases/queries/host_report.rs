/// Maps ReShade host detection state to the availability query's DTOs.
use renderpilot_domain::{InstalledAddon, TrackedSourceRole};

use crate::addons::renodx::channel;
use crate::addons::renodx::dto::actions::{ActionDescriptor, ActionDisabledReason};
use crate::addons::renodx::dto::availability::*;
use crate::addons::renodx::facts::{GameAnalysis, install_target_dir};
use crate::addons::renodx::host_policy;
use crate::addons::renodx::matcher::RenoDxResolution;
use crate::addons::renodx::policy::HostKind;
use crate::addons::renodx::reshade::{self, RenoDxAddonState, ReshadeHost, ReshadeHostAction};
use crate::addons::renodx::source;
use crate::addons::renodx::types::{ReshadeChannel, ReshadeConfig};
use crate::addons::renodx::vulkan;

pub(super) struct ReshadeReport {
    pub(super) detection: HostDetection,
    pub(super) facts: HostFacts,
    pub(super) actions: RenoDxActions,
    pub(super) addon: Option<RenoDxAddonState>,
}

pub(super) fn reshade_report(
    analysis: &GameAnalysis,
    resolution: &RenoDxResolution,
    record: Option<&InstalledAddon>,
    reshade_config: &ReshadeConfig,
) -> ReshadeReport {
    let Some(target_dir) = install_target_dir(analysis).ok() else {
        return missing_host_report(record, reshade_config);
    };
    let host_kind = plan_host_kind(resolution);
    let assessment = if matches!(host_kind, Some(HostKind::Vulkan)) {
        let Some(layer_dir) = vulkan::layer_dir() else {
            return missing_host_report(record, reshade_config);
        };
        host_policy::assess(&layer_dir, "ReShade64.dll")
    } else {
        let Some(active_proxy) = active_proxy_slot(resolution) else {
            return missing_host_report(record, reshade_config);
        };
        host_policy::assess(&target_dir, active_proxy)
    };

    let addon_host_path = if matches!(host_kind, Some(HostKind::Vulkan)) {
        None
    } else {
        assessment.host.as_present().map(|present| present.path)
    };
    let paths = reshade::resolve_paths(&target_dir, addon_host_path);
    let addon = expected_addon_file_name(resolution)
        .map(|file_name| reshade::renodx_addon_state(&paths, &file_name));
    let detected_channel = recorded_channel(record);
    let is_custom_build = assessment.is_known_custom_build();

    ReshadeReport {
        detection: host_detection(&assessment.host, assessment.conflict),
        facts: host_facts(
            &assessment.host,
            assessment.action,
            assessment.conflict,
            is_custom_build,
            detected_channel,
            reshade_config,
        ),
        actions: host_actions(
            &assessment.host,
            assessment.action,
            assessment.conflict,
            is_custom_build,
            detected_channel,
            reshade_config,
            record,
        ),
        addon,
    }
}

/// Extracts the [`HostKind`] from a resolution, if it has one (installable plans
/// and file-installable external titles).
pub(super) fn plan_host_kind(resolution: &RenoDxResolution) -> Option<HostKind> {
    match resolution {
        RenoDxResolution::Installable(plan) => Some(plan.host_kind),
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => Some(plan.host_kind),
        _ => None,
    }
}

/// The host report for an install whose install target directory or active proxy
/// slot cannot be resolved: no host, a conflict verdict, and only the channel
/// carried through from the install record.
fn missing_host_report(
    record: Option<&InstalledAddon>,
    reshade_config: &ReshadeConfig,
) -> ReshadeReport {
    let detected_channel = recorded_channel(record);
    ReshadeReport {
        detection: HostDetection::Absent,
        facts: host_facts(
            &ReshadeHost::Absent,
            ReshadeHostAction::Conflict,
            false,
            false,
            detected_channel,
            reshade_config,
        ),
        actions: RenoDxActions::default(),
        addon: None,
    }
}

fn host_detection(host: &ReshadeHost, conflict: bool) -> HostDetection {
    if conflict {
        return HostDetection::Conflict;
    }
    if host.as_present().is_some() {
        HostDetection::Present
    } else {
        HostDetection::Absent
    }
}

fn host_facts(
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
        active: present.is_some_and(|host| host.active.state == reshade::SlotActivity::Active),
        path: present.map(|host| host.path.to_path_buf()),
        version: present.and_then(|host| host.version.map(ToString::to_string)),
        addon_support: present
            .map(|host| match host.addon_support {
                reshade::ReshadeAddonSupport::Full => HostAddonSupport::Full,
                reshade::ReshadeAddonSupport::None => HostAddonSupport::Limited,
                reshade::ReshadeAddonSupport::Unknown => HostAddonSupport::Unknown,
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

fn host_update_status(
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
/// an upstream-verified build. Never gates *confirmation*: both RenoDX's own
/// artifacts and this advisory case update silently, per the same "we already
/// PE-sanity-checked what we fetch" reasoning; only a recognized custom build
/// (see `is_custom_build`) is ever left untouched.
fn has_advisory_host_source(record: Option<&InstalledAddon>) -> bool {
    record.is_some_and(|record| {
        record
            .tracked_sources()
            .iter()
            .any(|source| source.role() == TrackedSourceRole::HostBinary && source.is_advisory())
    })
}

fn host_actions(
    host: &ReshadeHost,
    action: ReshadeHostAction,
    conflict: bool,
    is_custom_build: bool,
    detected_channel: Option<ReshadeChannel>,
    reshade_config: &ReshadeConfig,
    record: Option<&InstalledAddon>,
) -> RenoDxActions {
    if is_custom_build {
        // A recognized custom build (e.g. GShade): neither a conflict to
        // resolve nor ours to update/repair/switch channel — RenoDX offers no
        // host action for it at all.
        return RenoDxActions::default();
    }
    if conflict || action == ReshadeHostAction::Conflict {
        return RenoDxActions {
            resolve_conflict: Some(ActionDescriptor::disabled(
                ActionDisabledReason::BlockedByConflict,
            )),
            ..RenoDxActions::default()
        };
    }
    let switch_channel =
        detected_channel.map(|channel| switch_channel_action(channel, reshade_config));
    match action {
        ReshadeHostAction::UpdateHost if host.as_present().is_none() => RenoDxActions {
            install: Some(ActionDescriptor::enabled()),
            switch_channel,
            ..RenoDxActions::default()
        },
        ReshadeHostAction::UpdateHost => RenoDxActions {
            update: Some(ActionDescriptor::enabled()),
            switch_channel,
            ..RenoDxActions::default()
        },
        ReshadeHostAction::ReinstallWithAddonSupport | ReshadeHostAction::RepairHost => {
            RenoDxActions {
                repair: Some(ActionDescriptor::enabled()),
                switch_channel,
                ..RenoDxActions::default()
            }
        }
        ReshadeHostAction::UpToDate => {
            let update = has_advisory_host_source(record).then(ActionDescriptor::enabled);
            RenoDxActions {
                use_existing: host.as_present().map(|_| ActionDescriptor::enabled()),
                switch_channel,
                update,
                ..RenoDxActions::default()
            }
        }
        ReshadeHostAction::Conflict => RenoDxActions {
            resolve_conflict: Some(ActionDescriptor::disabled(
                ActionDisabledReason::BlockedByConflict,
            )),
            ..RenoDxActions::default()
        },
    }
}

fn switch_channel_action(
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

/// The ReShade channel recorded on the install's host binary artifact, if any. A
/// record with duplicate host sources or an unreadable channel degrades to `None`.
fn recorded_channel(record: Option<&InstalledAddon>) -> Option<ReshadeChannel> {
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

fn active_proxy_slot(resolution: &RenoDxResolution) -> Option<&str> {
    match resolution {
        RenoDxResolution::Installable(plan) => Some(plan.proxy_dll_name.as_str()),
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => Some(plan.proxy_dll_name.as_str()),
        _ => None,
    }
}

fn expected_addon_file_name(resolution: &RenoDxResolution) -> Option<String> {
    match resolution {
        RenoDxResolution::Installable(plan) => Some(source::addon_file_name(&plan.slug, plan.arch)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::matcher::MatchConfidence;
    use crate::addons::renodx::policy::generic_risk;
    use crate::addons::renodx::test_support::manifest;
    use renderpilot_domain::{AddonKind, Architecture, GameId, PathRef, TrackedSource};

    #[test]
    fn expected_addon_file_name_uses_resolved_canonical_slug() {
        let plan = crate::addons::renodx::matcher::ResolvedInstall {
            slug: "unityengine".to_owned(),
            addon_url: "https://example.com/renodx-unityengine.addon64".to_owned(),
            arch: Architecture::X64,
            proxy_dll_name: "dxgi.dll".to_owned(),
            risk: generic_risk(),
            confidence: MatchConfidence::Verified,
            notes_keys: Vec::new(),
            host_kind: HostKind::Proxy,
        };
        let resolution = RenoDxResolution::Installable(Box::new(plan));

        assert_eq!(
            expected_addon_file_name(&resolution).as_deref(),
            Some("renodx-unityengine.addon64")
        );
    }

    #[test]
    fn conflict_yields_only_resolve_conflict() {
        let actions = host_actions(
            &ReshadeHost::Absent,
            ReshadeHostAction::Conflict,
            true,
            false,
            None,
            &manifest(Vec::new()).reshade,
            None,
        );
        assert!(actions.resolve_conflict.is_some());
        assert!(!actions.resolve_conflict.unwrap().enabled);
        assert!(actions.install.is_none());
        assert!(actions.use_existing.is_none());
        assert!(actions.repair.is_none());
        assert!(actions.update.is_none());
        assert!(actions.switch_channel.is_none());
    }

    #[test]
    fn recognized_custom_build_offers_no_host_actions_even_when_otherwise_up_to_date() {
        let present = ReshadeHost::Present {
            path: std::path::PathBuf::from("C:\\game\\dxgi.dll"),
            slot: "dxgi.dll".to_owned(),
            version: None,
            addon_support: reshade::ReshadeAddonSupport::Full,
            identity: reshade::ReshadeIdentity::Confirmed,
            active: reshade::ActiveSlotState {
                state: reshade::SlotActivity::Active,
                reason: reshade::ActiveSlotReason::DetectedByMatcher,
            },
        };
        // `conflict: true` mirrors what `host_policy::assess` actually reports for
        // a recognized custom build (folded into the same conflict signal); the
        // `is_custom_build` bit must still short-circuit before the generic
        // conflict branch, offering nothing rather than a resolve-conflict action.
        let actions = host_actions(
            &present,
            ReshadeHostAction::UpToDate,
            true,
            true,
            Some(ReshadeChannel::Stable),
            &manifest(Vec::new()).reshade,
            None,
        );
        assert!(actions.resolve_conflict.is_none());
        assert!(actions.use_existing.is_none());
        assert!(actions.update.is_none());
        assert!(actions.repair.is_none());
        assert!(actions.switch_channel.is_none());
    }

    #[test]
    fn up_to_date_full_host_exposes_use_existing_without_remove_rights() {
        let present = ReshadeHost::Present {
            path: std::path::PathBuf::from("C:\\game\\dxgi.dll"),
            slot: "dxgi.dll".to_owned(),
            version: None,
            addon_support: reshade::ReshadeAddonSupport::Full,
            identity: reshade::ReshadeIdentity::Confirmed,
            active: reshade::ActiveSlotState {
                state: reshade::SlotActivity::Active,
                reason: reshade::ActiveSlotReason::DetectedByMatcher,
            },
        };
        let actions = host_actions(
            &present,
            ReshadeHostAction::UpToDate,
            false,
            false,
            Some(ReshadeChannel::Stable),
            &manifest(Vec::new()).reshade,
            None,
        );
        assert!(actions.use_existing.is_some());
        assert!(actions.install.is_none());
        assert!(actions.repair.is_none());
        assert!(actions.update.is_none());
        assert!(actions.resolve_conflict.is_none());
    }

    #[test]
    fn limited_addon_support_offers_repair() {
        let present = ReshadeHost::Present {
            path: std::path::PathBuf::from("C:\\game\\dxgi.dll"),
            slot: "dxgi.dll".to_owned(),
            version: None,
            addon_support: reshade::ReshadeAddonSupport::None,
            identity: reshade::ReshadeIdentity::Confirmed,
            active: reshade::ActiveSlotState {
                state: reshade::SlotActivity::Active,
                reason: reshade::ActiveSlotReason::DetectedByMatcher,
            },
        };
        let actions = host_actions(
            &present,
            ReshadeHostAction::ReinstallWithAddonSupport,
            false,
            false,
            Some(ReshadeChannel::Stable),
            &manifest(Vec::new()).reshade,
            None,
        );
        assert!(actions.repair.is_some());
        assert!(actions.repair.unwrap().enabled);
        assert!(actions.install.is_none());
        assert!(actions.use_existing.is_none());
    }

    #[test]
    fn switch_channel_to_unsupported_stable_is_disabled() {
        let mut manifest = manifest(Vec::new());
        manifest.reshade.stable = None;

        let actions = host_actions(
            &ReshadeHost::Absent,
            ReshadeHostAction::UpToDate,
            false,
            false,
            Some(ReshadeChannel::Nightly),
            &manifest.reshade,
            None,
        );
        let switch = actions.switch_channel.expect("switch action");

        assert!(!switch.enabled);
        assert_eq!(
            switch.disabled_reason,
            Some(ActionDisabledReason::StableUnavailable)
        );
        assert_eq!(switch.target_channel, Some(ReshadeChannel::Stable));
    }

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

    fn record_with_host_source(source: TrackedSource) -> InstalledAddon {
        InstalledAddon::new(
            GameId::new("steam:1091500").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new("C:/Games/Test/renodx-test.addon64").expect("addon path"),
        )
        .with_tracked_source(source)
    }

    #[test]
    fn up_to_date_offers_a_silent_update_for_an_advisory_host_source() {
        let present = ReshadeHost::Present {
            path: std::path::PathBuf::from("C:\\game\\dxgi.dll"),
            slot: "dxgi.dll".to_owned(),
            version: None,
            addon_support: reshade::ReshadeAddonSupport::Full,
            identity: reshade::ReshadeIdentity::Confirmed,
            active: reshade::ActiveSlotState {
                state: reshade::SlotActivity::Active,
                reason: reshade::ActiveSlotReason::DetectedByMatcher,
            },
        };
        let record = record_with_host_source(
            TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example/host",
                None,
                "digest",
            )
            .with_advisory(),
        );

        let actions = host_actions(
            &present,
            ReshadeHostAction::UpToDate,
            false,
            false,
            Some(ReshadeChannel::Stable),
            &manifest(Vec::new()).reshade,
            Some(&record),
        );

        // An advisory (adopted) host still offers an "Update" action even when
        // otherwise up to date, so the user can normalize onto an upstream-
        // verified build — but never requires confirmation. Only a recognized
        // custom build (`is_custom_build`) is left untouched entirely.
        let update = actions.update.expect("update offered for advisory source");
        assert!(update.enabled);
        assert!(!update.requires_confirmation);
        assert_eq!(update.confirmation_scope, None);
    }

    #[test]
    fn update_host_is_enabled_without_confirmation_for_non_advisory_source_without_etag() {
        let present = ReshadeHost::Present {
            path: std::path::PathBuf::from("C:\\game\\dxgi.dll"),
            slot: "dxgi.dll".to_owned(),
            version: None,
            addon_support: reshade::ReshadeAddonSupport::Full,
            identity: reshade::ReshadeIdentity::Confirmed,
            active: reshade::ActiveSlotState {
                state: reshade::SlotActivity::Active,
                reason: reshade::ActiveSlotReason::DetectedByMatcher,
            },
        };
        let record = record_with_host_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host",
            None,
            "digest",
        ));

        let actions = host_actions(
            &present,
            ReshadeHostAction::UpdateHost,
            false,
            false,
            Some(ReshadeChannel::Stable),
            &manifest(Vec::new()).reshade,
            Some(&record),
        );

        let update = actions.update.expect("update offered");
        assert!(!update.requires_confirmation);
        assert_eq!(update.confirmation_scope, None);
    }
}
