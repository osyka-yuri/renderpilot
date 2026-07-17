//! Queries RenoDX availability for a specific game.
use std::path::Path;

use renderpilot_domain::{AddonKind, Architecture, GameId, RenoDxInstallState};

use crate::Context;
use crate::ServiceError;

use crate::addons::anticheat::{RiskSeverity, assess_risk};
use crate::addons::availability_pipeline::{self, AvailabilityPreflight};
use crate::addons::matching::MatchFacts;
use crate::addons::renodx::dto::availability::*;
use crate::addons::renodx::game_context::analyze_and_resolve;
use crate::addons::renodx::matcher::{RenoDxResolution, file_installable, matched_slug};
use crate::addons::renodx::source;
use crate::addons::renodx::tracking;
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::renodx::vulkan;
use crate::addons::reshade::proxy::{host_decision, primary_api};
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::game_mutation_lock;

use super::host_report;

mod candidate;
mod reconcile;

/// Reconciles recoverable local state under the same per-game lock used by
/// install/update/uninstall, then returns a pure availability snapshot.
///
/// Intentionally skips `recover_pending`: this is a read-oriented query path,
/// not a durable file-mutation boundary.
pub async fn load_availability(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
) -> Result<AvailabilityReport, ServiceError> {
    let _guard = game_mutation_lock::lock(game_id).await;
    let mut preflight = availability_pipeline::preflight(
        context,
        game_id,
        AddonKind::RenoDx,
        manifest,
        analyze_and_resolve,
    )?;
    reconcile::maybe_adopt(context, &mut preflight, reshade_sources, game_id)?;
    build_report(preflight, manifest, reshade_sources)
}

/// Pure preview of whether RenoDX can be installed for the game. Never changes
/// filesystem or persistence state. Test-only (Windows integration tests):
/// production uses [`load_availability`].
#[cfg(all(test, windows))]
pub(crate) fn availability(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
) -> Result<AvailabilityReport, ServiceError> {
    let preflight = availability_pipeline::preflight(
        context,
        game_id,
        AddonKind::RenoDx,
        manifest,
        analyze_and_resolve,
    )?;
    build_report(preflight, manifest, reshade_sources)
}

fn build_report(
    preflight: AvailabilityPreflight<RenoDxResolution>,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
) -> Result<AvailabilityReport, ServiceError> {
    let AvailabilityPreflight {
        record,
        game,
        blocked,
        analysis,
        resolution,
        ..
    } = preflight;
    let scan_dir = Path::new(game.install_path().as_str());
    let host_report =
        host_report::reshade_report(&analysis, &resolution, record.as_ref(), reshade_sources);

    let state = record
        .as_ref()
        .map(tracking::install_state_from_record)
        .unwrap_or(RenoDxInstallState::NotInstalled);

    // The manual file-install escape hatch would let a user bypass the
    // exclusivity block by hand-installing RenoDX anyway; withhold it too. Must
    // run before `resolution` is consumed by the `outcome` match below.
    let manual_install = if blocked.is_none() {
        manual_file_install(manifest, &analysis.facts, &resolution, scan_dir)
    } else {
        None
    };

    let outcome = if let Some(block) = blocked {
        let blocked = availability_pipeline::blocked_outcome(block);
        AvailabilityOutcome::BlockedByOtherAddon {
            other_kind: blocked.other_kind,
            unmanaged: blocked.unmanaged,
        }
    } else {
        match resolution {
            RenoDxResolution::Installable(plan) => AvailabilityOutcome::Installable {
                confidence: plan.confidence,
                risk: assess_risk(scan_dir, RiskSeverity::Info),
                generic_profile: plan.generic_profile,
                host_kind: plan.host_kind,
            },
            RenoDxResolution::External {
                url,
                message,
                file_install,
            } => AvailabilityOutcome::External {
                url,
                message,
                file_install: file_install.map(|fi| ExternalFileInstall {
                    confidence: fi.confidence,
                    risk: assess_risk(scan_dir, RiskSeverity::Info),
                    host_kind: fi.host_kind,
                    generic_profile: fi.generic_profile,
                }),
            },
            RenoDxResolution::NativeHdr => AvailabilityOutcome::NativeHdr,
            RenoDxResolution::Incompatible { reason } => {
                AvailabilityOutcome::Incompatible { reason }
            }
            RenoDxResolution::Blacklisted { message } => {
                AvailabilityOutcome::Blacklisted { message }
            }
            RenoDxResolution::NoMatch => AvailabilityOutcome::Unsupported,
        }
    };

    Ok(AvailabilityReport {
        state,
        host_detection: host_report.detection,
        host_facts: host_report.facts,
        actions: host_report.actions,
        reshade_stable_supported: reshade_sources.supports_channel(ReshadeChannel::Stable),
        renodx_addon: host_report.addon,
        outcome,
        manual_install,
        vulkan_layer: vulkan::layer_report(),
    })
}

/// The manual file-install escape hatch for the availability preview: offered only
/// when a matched title cannot use the automatic path but the renderer can still
/// load RenoDX. An unmatched, blacklisted, native-HDR, automatic, or external title
/// gets `None` — the manual path would be misleading, redundant, or deliberately
/// withheld.
fn manual_file_install(
    manifest: &RenoDxManifest,
    facts: &MatchFacts,
    resolution: &RenoDxResolution,
    scan_dir: &Path,
) -> Option<ManualFileInstall> {
    let offered = matches!(resolution, RenoDxResolution::Incompatible { .. });
    let host_kind = host_decision(primary_api(&facts.graphics))?;
    if !offered || !file_installable(facts) {
        return None;
    }
    Some(ManualFileInstall {
        risk: assess_risk(scan_dir, RiskSeverity::Info),
        host_kind,
        expected_addon_name: matched_slug(manifest, facts)
            .map(|slug| source::addon_file_stem(&slug)),
        game_arch: facts.graphics.architecture().map(arch_str),
    })
}

/// Stable wire string for a game's architecture, for the UI's add-on-arch check.
fn arch_str(arch: Architecture) -> String {
    match arch {
        Architecture::X64 => "x64",
        Architecture::X86 => "x86",
    }
    .to_owned()
}
