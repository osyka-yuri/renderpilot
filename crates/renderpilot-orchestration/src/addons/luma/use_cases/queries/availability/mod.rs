//! Queries Luma availability for a specific game.
use std::path::Path;

use renderpilot_domain::{AddonKind, Architecture, GameId, LumaInstallState};

use crate::Context;
use crate::ServiceError;

use super::host_report::host_report;
use crate::addons::anticheat::{RiskSeverity, assess_risk};
use crate::addons::availability_pipeline::{self, AvailabilityPreflight};
use crate::addons::engine;
use crate::addons::game_analysis::install_target_dir;
use crate::addons::luma::dgvoodoo;
use crate::addons::luma::dto::availability::*;
use crate::addons::luma::game_context::{analyze_and_resolve, effective_launch_args};
use crate::addons::luma::matcher::LumaResolution;
use crate::addons::luma::tracking;
use crate::addons::luma::types::LumaManifest;
use crate::addons::luma::vcredist;
use crate::addons::reshade::dto::{ActionDescriptor, ActionDisabledReason};
use crate::addons::reshade::host_policy;
use crate::addons::reshade::types::ReshadeSourceCatalog;
use crate::game_mutation_lock;

mod reconcile;
// Disk/PE integration tests require Windows primary-exe resolution
// (`game_executable` returns `None` on non-Windows). Gated like RenoDX.
#[cfg(all(test, windows))]
mod tests;

/// Reconciles recoverable local state under the same per-game lock used by
/// install/update/uninstall, then returns a pure availability snapshot.
///
/// Intentionally skips `recover_pending`: this is a read-oriented query path,
/// not a durable file-mutation boundary.
pub async fn load_availability(
    context: &Context,
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
) -> Result<AvailabilityReport, ServiceError> {
    let _guard = game_mutation_lock::lock(game_id).await;
    let mut preflight = availability_pipeline::preflight(
        context,
        game_id,
        AddonKind::Luma,
        manifest,
        analyze_and_resolve,
    )?;
    reconcile::maybe_adopt(context, &mut preflight, manifest, reshade_sources, game_id)?;
    build_report(preflight, manifest, reshade_sources)
}

/// Pure preview of whether Luma can be installed for the game. Never changes
/// filesystem or persistence state. Test-only (Windows integration tests):
/// production uses [`load_availability`].
#[cfg(all(test, windows))]
pub(crate) fn availability(
    context: &Context,
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
) -> Result<AvailabilityReport, ServiceError> {
    let preflight = availability_pipeline::preflight(
        context,
        game_id,
        AddonKind::Luma,
        manifest,
        analyze_and_resolve,
    )?;
    build_report(preflight, manifest, reshade_sources)
}

fn build_report(
    preflight: AvailabilityPreflight<LumaResolution>,
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
) -> Result<AvailabilityReport, ServiceError> {
    let AvailabilityPreflight {
        record,
        game,
        blocked,
        analysis,
        resolution,
        roots: install_roots,
    } = preflight;
    let scan_dir = Path::new(game.install_path().as_str());
    let min_version = manifest.min_reshade_version_parsed()?;
    let mut host = host_report(
        &analysis,
        &resolution,
        record.as_ref(),
        &min_version,
        reshade_sources,
    );

    if record.is_none() && initial_install_is_blocked(&analysis, &resolution, &min_version) {
        host.actions.install = Some(ActionDescriptor::disabled(
            ActionDisabledReason::BlockedByConflict,
        ));
    }

    // Borrow `resolution` here — `outcome`'s `match` below consumes it by value.
    // For installed records we still re-resolve to pick up current manifest args + auto DX11.
    let state = record
        .as_ref()
        .map(|record| {
            tracking::install_state_from_record(
                record,
                effective_launch_args(&analysis, &resolution),
            )
        })
        .unwrap_or(LumaInstallState::NotInstalled);

    let install_torn = install_roots
        .as_ref()
        .is_some_and(|roots| engine::is_install_torn(roots.sentinel_dir(), AddonKind::Luma));
    // A torn folder (a crash mid-install left debris with no record) is never
    // reported as `UnmanagedPresent` — the install command recovers it
    // automatically (see `luma::install::recover_torn_install`), so this stays
    // a normal, install-torn-flagged outcome rather than steering the user
    // toward an unnecessary manual cleanup.
    let unmanaged_present = blocked.is_none()
        && record.is_none()
        && !install_torn
        && install_roots.as_ref().is_some_and(|roots| {
            crate::addons::tool::unmanaged_files_present_in_dirs(
                &roots.scan_dir_paths(),
                AddonKind::Luma,
            )
        });

    // Compute user-facing launch args (manifest + auto DX11 for UE+D3D12) once.
    let launch_args = effective_launch_args(&analysis, &resolution);

    let outcome = if let Some(block) = blocked {
        let blocked = availability_pipeline::blocked_outcome(block);
        AvailabilityOutcome::BlockedByOtherAddon {
            other_kind: blocked.other_kind,
            unmanaged: blocked.unmanaged,
        }
    } else if unmanaged_present {
        AvailabilityOutcome::UnmanagedPresent
    } else {
        match resolution {
            LumaResolution::Installable(plan) => AvailabilityOutcome::Installable {
                confidence: plan.confidence,
                risk: assess_risk(scan_dir, RiskSeverity::Info),
                launch_args,
                profile: plan.profile,
                features: plan.features,
                guidance: plan.guidance,
                external_requirement: plan.external_requirement.map(Into::into),
            },
            LumaResolution::Incompatible { reason } => AvailabilityOutcome::Incompatible { reason },
            LumaResolution::Blacklisted { message } => AvailabilityOutcome::Blacklisted { message },
            LumaResolution::NoMatch => AvailabilityOutcome::Unsupported,
        }
    };

    let arch = analysis
        .facts
        .graphics
        .architecture()
        .unwrap_or(Architecture::X64);

    Ok(AvailabilityReport {
        state,
        host_detection: host.detection,
        host_facts: host.facts,
        actions: host.actions,
        min_reshade_version: manifest.min_reshade_version.clone(),
        vcredist_present: vcredist::vcredist_present(arch),
        vcredist_installer_url: vcredist::vcredist_installer_url(arch).to_owned(),
        install_torn,
        outcome,
    })
}

/// Determines whether a fresh install may write at all. A proved-empty runtime
/// can be repaired as part of Install; user content or an incomplete scan cannot.
fn initial_install_is_blocked(
    analysis: &crate::addons::game_analysis::GameAnalysis,
    resolution: &LumaResolution,
    min_version: &renderpilot_domain::Version,
) -> bool {
    let LumaResolution::Installable(plan) = resolution else {
        return false;
    };
    let Ok(target_dir) = install_target_dir(analysis) else {
        return true;
    };
    let host =
        host_policy::assess_for_tool(&target_dir, &plan.proxy_dll_name, "Luma", Some(min_version));
    if host.initial_is_conflict() {
        return true;
    }

    dgvoodoo::requirement(plan.external_requirement.as_ref()).is_some_and(|requirement| {
        matches!(
            dgvoodoo::assess_existing(&target_dir, requirement),
            dgvoodoo::ExistingDgVoodoo::Conflict(_)
        )
    })
}
