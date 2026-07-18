use renderpilot_domain::{InstalledAddon, Version};

use crate::addons::game_analysis::{GameAnalysis, install_target_dir};
use crate::addons::luma::dto::availability::{HostDetection, HostFacts, LumaActions};
use crate::addons::luma::matcher::LumaResolution;
use crate::addons::reshade::host_policy;
use crate::addons::reshade::report::{
    AssembleHostReport, HostReportPolicy, assemble_host_report, missing_host_report_core,
};
use crate::addons::reshade::types::ReshadeSourceCatalog;

pub(super) struct HostReport {
    pub(super) detection: HostDetection,
    pub(super) facts: HostFacts,
    pub(super) actions: LumaActions,
}

pub(super) fn host_report(
    analysis: &GameAnalysis,
    resolution: &LumaResolution,
    record: Option<&InstalledAddon>,
    min_version: &Version,
    reshade_config: &ReshadeSourceCatalog,
) -> HostReport {
    let Some(proxy_dll_name) = active_proxy_slot(resolution) else {
        return missing_host_report(record, reshade_config);
    };
    let Ok(target_dir) = install_target_dir(analysis) else {
        return missing_host_report(record, reshade_config);
    };

    let allowed_addons = allowed_addon_names(resolution);
    let assessment = host_policy::assess_for_tool_with_allowed_addons(
        &target_dir,
        proxy_dll_name,
        "Luma",
        Some(min_version),
        &allowed_addons,
    );
    let (detection, facts, actions) = assemble_host_report(AssembleHostReport {
        assessment: &assessment,
        record,
        reshade_config,
        switch_channel: None,
        policy: HostReportPolicy::PROXY_INITIAL,
    });

    HostReport {
        detection,
        facts,
        actions,
    }
}

fn missing_host_report(
    record: Option<&InstalledAddon>,
    reshade_config: &ReshadeSourceCatalog,
) -> HostReport {
    let (detection, facts, _) = missing_host_report_core(record, reshade_config);
    HostReport {
        detection,
        facts,
        actions: LumaActions::default(),
    }
}

fn active_proxy_slot(resolution: &LumaResolution) -> Option<&str> {
    match resolution {
        LumaResolution::Installable(plan) => Some(plan.proxy_dll_name.as_str()),
        _ => None,
    }
}

/// The exact Luma payload is not user ReShade content. Passing it to the
/// shared classifier keeps availability, DB-loss recovery, and install on the
/// same lifecycle decision; a different add-on remains a conflict signal.
fn allowed_addon_names(resolution: &LumaResolution) -> Vec<&str> {
    match resolution {
        LumaResolution::Installable(plan) => vec![plan.addon_file.as_str()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;
    use crate::addons::test_support::{MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports};

    fn write_compatible_host(game_dir: &Path) {
        fs::write(
            game_dir.join("dxgi.dll"),
            build_pe_with_exports(
                MACHINE_AMD64,
                PE32_PLUS_MAGIC,
                &[
                    "ReShadeVersion",
                    "ReShadeRegisterAddon",
                    "ReShadeUnregisterAddon",
                    "ReShadeRegisterEvent",
                    "ReShadeUnregisterEvent",
                ],
            ),
        )
        .expect("host");
    }

    #[test]
    fn exact_luma_payload_is_not_classified_as_foreign_reshade_content() {
        let dir = tempdir().expect("tempdir");
        write_compatible_host(dir.path());
        fs::write(dir.path().join("Luma-Game.addon"), b"luma").expect("payload");

        let without_allow_list = host_policy::assess_for_tool(dir.path(), "dxgi.dll", "Luma", None);
        let with_exact_allow_list = host_policy::assess_for_tool_with_allowed_addons(
            dir.path(),
            "dxgi.dll",
            "Luma",
            None,
            &["Luma-Game.addon"],
        );

        assert_eq!(
            without_allow_list.lifecycle,
            host_policy::HostLifecycle::ReuseUser
        );
        assert_eq!(
            with_exact_allow_list.lifecycle,
            host_policy::HostLifecycle::AdoptEmpty
        );
        assert!(
            with_exact_allow_list
                .initial_owned_existing_paths(None)
                .contains(&dir.path().join("dxgi.dll")),
            "the compatible empty host must become removable Luma ownership"
        );

        fs::write(dir.path().join("foreign.addon"), b"foreign").expect("foreign add-on");
        let with_foreign_addon = host_policy::assess_for_tool_with_allowed_addons(
            dir.path(),
            "dxgi.dll",
            "Luma",
            None,
            &["Luma-Game.addon"],
        );
        assert_eq!(
            with_foreign_addon.lifecycle,
            host_policy::HostLifecycle::ReuseUser,
            "only the exact manifest payload may be allow-listed"
        );
    }
}
