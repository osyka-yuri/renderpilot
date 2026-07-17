use std::path::PathBuf;

use renderpilot_domain::{GameId, InstalledAddonHostKind};

use crate::addons::game_analysis::{GameAnalysis, install_target_dir};
use crate::addons::renodx::matcher::RenoDxResolution;
use crate::addons::renodx::reconciliation::OrphanedInstall;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::types::ReshadeSourceCatalog;

use super::super::host_report::{self, ReshadeReport};

pub(super) fn orphaned_install_candidate(
    game_id: &GameId,
    analysis: &GameAnalysis,
    resolution: &RenoDxResolution,
    host_report: &ReshadeReport,
    reshade_config: &ReshadeSourceCatalog,
) -> Option<OrphanedInstall> {
    // Adoption only trusts the exact resolved-slug filename — never the loose
    // `discovered_path` fallback (see `discover_renodx_addon`), which could
    // otherwise attribute an unrelated stray add-on file to this game. A slug
    // that no longer matches any on-disk file (e.g. after a manifest rename)
    // simply isn't adopted; the next real update re-fetches under the current
    // slug anyway.
    let addon = host_report
        .addon
        .as_ref()
        .filter(|addon| addon.expected_path.is_file())?;
    let host_kind = installed_host_kind(host_report::plan_host_kind(resolution)?);
    let game_dir = install_target_dir(analysis).ok()?;
    let host_file = host_report.facts.path.clone();
    let addon_file = addon.expected_path.clone();
    let registered_exe_path = if matches!(host_kind, InstalledAddonHostKind::SharedVulkanLayer) {
        Some(PathBuf::from(
            analysis.primary_executable.as_ref()?.as_str(),
        ))
    } else {
        None
    };

    let addon_url = match resolution {
        RenoDxResolution::Installable(plan) => Some(plan.addon_url.clone()),
        _ => None,
    };

    Some(OrphanedInstall {
        game_id: game_id.clone(),
        game_dir,
        addon_file,
        host_file,
        host_kind,
        registered_exe_path,
        reshade_config: reshade_config.clone(),
        game_arch: analysis.facts.graphics.architecture(),
        addon_url,
    })
}

fn installed_host_kind(host_kind: HostKind) -> InstalledAddonHostKind {
    match host_kind {
        HostKind::Proxy => InstalledAddonHostKind::Proxy,
        HostKind::Vulkan => InstalledAddonHostKind::SharedVulkanLayer,
    }
}
