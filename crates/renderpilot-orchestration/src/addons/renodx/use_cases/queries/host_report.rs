/// Maps ReShade host detection state to the availability query's DTOs.
use renderpilot_domain::InstalledAddon;

use crate::addons::game_analysis::{GameAnalysis, install_target_dir};
use crate::addons::renodx::dto::availability::*;
use crate::addons::renodx::matcher::RenoDxResolution;
use crate::addons::renodx::reshade::{self, RenoDxAddonState};
use crate::addons::renodx::source;
use crate::addons::renodx::vulkan;
use crate::addons::reshade::host_policy;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::report::{
    build_host_report_core, recorded_channel, switch_channel_action,
};
use crate::addons::reshade::scan::{ReshadeHost, ReshadeHostAction};
use crate::addons::reshade::types::ReshadeConfig;

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
    let paths = crate::addons::reshade::scan::resolve_paths(&target_dir, addon_host_path);
    let addon = expected_addon_file_name(resolution)
        .map(|file_name| reshade::renodx_addon_state(&paths, &file_name));
    let detected_channel = recorded_channel(record);
    let is_custom_build = assessment.is_known_custom_build();
    let (detection, facts, actions) = build_host_report_core(
        &assessment.host,
        assessment.action,
        assessment.conflict,
        is_custom_build,
        record,
        reshade_config,
        detected_channel.map(|channel| switch_channel_action(channel, reshade_config)),
    );

    ReshadeReport {
        detection,
        facts,
        actions,
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
    let (detection, facts, _) = build_host_report_core(
        &ReshadeHost::Absent,
        ReshadeHostAction::Conflict,
        false,
        false,
        record,
        reshade_config,
        None,
    );
    ReshadeReport {
        detection,
        facts,
        actions: RenoDxActions::default(),
        addon: None,
    }
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
    use crate::addons::reshade::types::ReshadeChannel;

    use super::*;
    use crate::addons::renodx::matcher::MatchConfidence;
    use crate::addons::renodx::test_support::manifest;
    use crate::addons::reshade::dto::ActionDisabledReason;
    use crate::addons::reshade::report::host_action_core;
    use renderpilot_domain::{
        AddonKind, Architecture, GameId, PathRef, TrackedSource, TrackedSourceRole,
    };

    #[test]
    fn expected_addon_file_name_uses_resolved_canonical_slug() {
        let plan = crate::addons::renodx::matcher::ResolvedInstall {
            slug: "unityengine".to_owned(),
            addon_url: "https://example.com/renodx-unityengine.addon64".to_owned(),
            arch: Architecture::X64,
            proxy_dll_name: "dxgi.dll".to_owned(),
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
        let actions = host_action_core(
            &ReshadeHost::Absent,
            ReshadeHostAction::Conflict,
            true,
            false,
            None,
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
            addon_support: crate::addons::reshade::scan::ReshadeAddonSupport::Full,
            identity: crate::addons::reshade::scan::ReshadeIdentity::Confirmed,
            active: crate::addons::reshade::scan::ActiveSlotState {
                state: crate::addons::reshade::scan::SlotActivity::Active,
                reason: crate::addons::reshade::scan::ActiveSlotReason::DetectedByMatcher,
            },
        };
        // `conflict: true` mirrors what `host_policy::assess` actually reports for
        // a recognized custom build (folded into the same conflict signal); the
        // `is_custom_build` bit must still short-circuit before the generic
        // conflict branch, offering nothing rather than a resolve-conflict action.
        let actions = host_action_core(
            &present,
            ReshadeHostAction::UpToDate,
            true,
            true,
            None,
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
            addon_support: crate::addons::reshade::scan::ReshadeAddonSupport::Full,
            identity: crate::addons::reshade::scan::ReshadeIdentity::Confirmed,
            active: crate::addons::reshade::scan::ActiveSlotState {
                state: crate::addons::reshade::scan::SlotActivity::Active,
                reason: crate::addons::reshade::scan::ActiveSlotReason::DetectedByMatcher,
            },
        };
        let actions = host_action_core(
            &present,
            ReshadeHostAction::UpToDate,
            false,
            false,
            None,
            Some(switch_channel_action(
                ReshadeChannel::Stable,
                &manifest(Vec::new()).reshade,
            )),
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
            addon_support: crate::addons::reshade::scan::ReshadeAddonSupport::None,
            identity: crate::addons::reshade::scan::ReshadeIdentity::Confirmed,
            active: crate::addons::reshade::scan::ActiveSlotState {
                state: crate::addons::reshade::scan::SlotActivity::Active,
                reason: crate::addons::reshade::scan::ActiveSlotReason::DetectedByMatcher,
            },
        };
        let actions = host_action_core(
            &present,
            ReshadeHostAction::ReinstallWithAddonSupport,
            false,
            false,
            None,
            Some(switch_channel_action(
                ReshadeChannel::Stable,
                &manifest(Vec::new()).reshade,
            )),
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

        let actions = host_action_core(
            &ReshadeHost::Absent,
            ReshadeHostAction::UpToDate,
            false,
            false,
            None,
            Some(switch_channel_action(
                ReshadeChannel::Nightly,
                &manifest.reshade,
            )),
        );
        let switch = actions.switch_channel.expect("switch action");

        assert!(!switch.enabled);
        assert_eq!(
            switch.disabled_reason,
            Some(ActionDisabledReason::StableUnavailable)
        );
        assert_eq!(switch.target_channel, Some(ReshadeChannel::Stable));
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
            addon_support: crate::addons::reshade::scan::ReshadeAddonSupport::Full,
            identity: crate::addons::reshade::scan::ReshadeIdentity::Confirmed,
            active: crate::addons::reshade::scan::ActiveSlotState {
                state: crate::addons::reshade::scan::SlotActivity::Active,
                reason: crate::addons::reshade::scan::ActiveSlotReason::DetectedByMatcher,
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

        let actions = host_action_core(
            &present,
            ReshadeHostAction::UpToDate,
            false,
            false,
            Some(&record),
            Some(switch_channel_action(
                ReshadeChannel::Stable,
                &manifest(Vec::new()).reshade,
            )),
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
            addon_support: crate::addons::reshade::scan::ReshadeAddonSupport::Full,
            identity: crate::addons::reshade::scan::ReshadeIdentity::Confirmed,
            active: crate::addons::reshade::scan::ActiveSlotState {
                state: crate::addons::reshade::scan::SlotActivity::Active,
                reason: crate::addons::reshade::scan::ActiveSlotReason::DetectedByMatcher,
            },
        };
        let record = record_with_host_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host",
            None,
            "digest",
        ));

        let actions = host_action_core(
            &present,
            ReshadeHostAction::UpdateHost,
            false,
            false,
            Some(&record),
            Some(switch_channel_action(
                ReshadeChannel::Stable,
                &manifest(Vec::new()).reshade,
            )),
        );

        let update = actions.update.expect("update offered");
        assert!(!update.requires_confirmation);
        assert_eq!(update.confirmation_scope, None);
    }
}
