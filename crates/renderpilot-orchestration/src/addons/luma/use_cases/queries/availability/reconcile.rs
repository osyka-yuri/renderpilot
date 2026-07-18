use renderpilot_domain::{AddonKind, GameId};

use crate::Context;
use crate::ServiceError;

use crate::addons::availability_pipeline::AvailabilityPreflight;
use crate::addons::engine;
use crate::addons::game_analysis::install_target_dir;
use crate::addons::luma::dgvoodoo;
use crate::addons::luma::matcher::LumaResolution;
use crate::addons::luma::reconciliation::{self, OrphanedLumaInstall};
use crate::addons::luma::tracking;
use crate::addons::luma::types::LumaManifest;
use crate::addons::reshade::host_policy;
use crate::addons::reshade::scan;
use crate::addons::reshade::source::require_reshade_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};

/// Performs the command-side recovery that may persist an adopted install or
/// advisory host source. Callers must hold the per-game `game_mutation_lock`.
///
/// Uses an existing preflight snapshot and does not re-run analysis. On
/// successful orphan adoption, updates `preflight.record` so the subsequent
/// report reflects the adopted install without a second preflight.
pub(super) fn maybe_adopt(
    context: &Context,
    preflight: &mut AvailabilityPreflight<LumaResolution>,
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
) -> Result<(), ServiceError> {
    if preflight.blocked.is_some() {
        return Ok(());
    }

    let min_version = manifest.min_reshade_version_parsed()?;
    if preflight.record.is_none()
        && let (Some(roots), LumaResolution::Installable(plan)) =
            (&preflight.roots, &preflight.resolution)
        && !engine::is_install_torn(roots.sentinel_dir(), AddonKind::Luma)
    {
        let scan_dirs = roots.scan_dir_paths();
        if let Some((addon_file, created_files)) =
            reconciliation::discover_orphaned_luma_payload(&scan_dirs, &plan.addon_file)
        {
            let mut candidate = OrphanedLumaInstall {
                game_id: game_id.clone(),
                asset: plan.asset.clone(),
                addon_file,
                created_files,
                advisory_host_source: None,
                advisory_dgvoodoo_source: None,
            };
            let target_dir = install_target_dir(&preflight.analysis)?;
            let allowed_addons = [plan.addon_file.as_str()];
            let recovery_host = host_policy::assess_for_tool_with_allowed_addons(
                &target_dir,
                &plan.proxy_dll_name,
                "Luma",
                Some(&min_version),
                &allowed_addons,
            );
            // Host: only a proved-empty Luma-compatible proxy — never a foreign
            // or user-content ReShade install.
            if recovery_host.lifecycle == host_policy::HostLifecycle::AdoptEmpty {
                for path in recovery_host.initial_owned_existing_paths(
                    scan::resolve_paths(&target_dir, Some(&recovery_host.target_path))
                        .ini_path
                        .as_deref(),
                ) {
                    push_unique_path(&mut candidate.created_files, path);
                }
                let nightly =
                    require_reshade_source(reshade_sources, ReshadeChannel::Nightly, plan.arch)?;
                candidate.advisory_host_source = Some(tracking::advisory_nightly_host_source(
                    &recovery_host.target_path,
                    nightly.url,
                )?);
            }
            // dgVoodoo is independent of the host lifecycle: CompatibleAdoptable
            // already requires a full map + PE identity + Luma-shaped config.
            // CompatibleReusable (user conf) is deliberately not claimed.
            if let Some(requirement) = dgvoodoo::requirement(plan.external_requirement.as_ref())
                && matches!(
                    dgvoodoo::assess_existing(&target_dir, requirement),
                    dgvoodoo::ExistingDgVoodoo::CompatibleAdoptable
                )
            {
                for path in dgvoodoo::adopted_existing(requirement, &target_dir).existing_paths {
                    push_unique_path(&mut candidate.created_files, path);
                }
                candidate.advisory_dgvoodoo_source =
                    Some(dgvoodoo::advisory_wrapper_source(requirement));
            }

            if let Some(adopted) =
                reconciliation::reconcile_orphaned_install_locked(context, &candidate)?
            {
                preflight.record = Some(adopted);
            }
        }
    }

    Ok(())
}

fn push_unique_path(paths: &mut Vec<std::path::PathBuf>, path: std::path::PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}
