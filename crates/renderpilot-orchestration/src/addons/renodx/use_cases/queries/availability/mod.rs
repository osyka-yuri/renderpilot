//! Queries RenoDX availability for a specific game.
use renderpilot_domain::{AddonKind, Architecture, GameId, RenoDxInstallState};

use crate::Context;
use crate::ServiceError;

use crate::addons::availability_pipeline::{self, AvailabilityPreflight};
use crate::addons::engine;
use crate::addons::engine_config::service::{self, EngineConfigAvailability};
use crate::addons::engine_config::{EngineIniRecipe, EngineIniRecipeSet};
use crate::addons::matching::MatchFacts;
use crate::addons::renodx::dto::availability::*;
use crate::addons::renodx::game_context::analyze_and_resolve;
use crate::addons::renodx::matcher::{RenoDxResolution, matched_slug};
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
#[cfg(test)]
mod tests;

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
        blocked,
        analysis,
        resolution,
        roots: install_roots,
        engine_config_resolution,
    } = preflight;
    let engine_config = engine_config_report(
        &engine_config_resolution,
        record.as_ref(),
        guidance_for_resolution(&resolution),
    )?;
    let mut host_report =
        host_report::reshade_report(&analysis, &resolution, record.as_ref(), reshade_sources);
    if blocked.is_none() && matches!(&resolution, RenoDxResolution::UnsupportedSettings) {
        use crate::addons::reshade::dto::{ActionDescriptor, ActionDisabledReason};
        for action in [
            &mut host_report.actions.update,
            &mut host_report.actions.repair,
        ] {
            if action.is_some() {
                *action = Some(ActionDescriptor::disabled(
                    ActionDisabledReason::UnsupportedSettings,
                ));
            }
        }
    }

    let state = record.as_ref().map_or(
        RenoDxInstallState::NotInstalled,
        tracking::install_state_from_record,
    );
    let install_torn = install_roots
        .as_ref()
        .is_some_and(|roots| engine::is_install_torn(roots.sentinel_dir(), AddonKind::RenoDx));

    // The manual file-install escape hatch would let a user bypass the
    // exclusivity block by hand-installing RenoDX anyway; withhold it too. Must
    // run before `resolution` is consumed by the `outcome` match below.
    let manual_install = if blocked.is_none() {
        manual_file_install(manifest, &analysis.facts, &resolution)
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
                generic_profile: plan.generic_profile,
                profile_id: plan.profile_id,
                host_kind: plan.host_kind,
                guidance: plan.guidance,
                launch: plan.launch,
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
                    host_kind: fi.host_kind,
                    generic_profile: fi.generic_profile,
                    profile_id: fi.profile_id,
                    guidance: fi.guidance,
                    launch: fi.launch,
                }),
            },
            RenoDxResolution::NativeHdr => AvailabilityOutcome::NativeHdr,
            RenoDxResolution::UnsupportedSettings => AvailabilityOutcome::UnsupportedSettings,
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
        engine_config,
        state,
        host_detection: host_report.detection,
        host_facts: host_report.facts,
        actions: host_report.actions,
        reshade_stable_supported: reshade_sources.supports_channel(ReshadeChannel::Stable),
        renodx_addon: host_report.addon,
        install_torn,
        outcome,
        manual_install,
        vulkan_layer: vulkan::layer_report(),
    })
}

fn guidance_for_resolution(
    resolution: &RenoDxResolution,
) -> &[crate::addons::renodx::types::RenoDxGuidance] {
    match resolution {
        RenoDxResolution::Installable(plan) => &plan.guidance,
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => &plan.guidance,
        _ => &[],
    }
}

fn engine_config_report(
    resolution: &crate::addons::engine_config::EngineIniResolution,
    record: Option<&renderpilot_domain::InstalledAddon>,
    guidance: &[crate::addons::renodx::types::RenoDxGuidance],
) -> Result<EngineConfigAvailability, ServiceError> {
    let manual_only = guidance.iter().any(|item| {
        matches!(
            item.kind,
            crate::addons::renodx::types::RenoDxGuidanceKind::EngineIni
        ) && item.engine_ini.is_none()
    });
    let recipes = guidance
        .iter()
        .filter_map(|item| {
            item.engine_ini
                .as_ref()
                .map(|recipe| recipe.to_engine_config_recipe(&item.id))
        })
        .collect::<Result<Vec<EngineIniRecipe>, _>>()
        .map_err(|error| ServiceError::invalid_input(error.to_string()))?;
    let recipe_set = if recipes.is_empty() {
        None
    } else {
        Some(
            EngineIniRecipeSet::from_recipes(recipes.iter())
                .map_err(|error| ServiceError::invalid_input(error.to_string()))?,
        )
    };
    Ok(service::inspect_availability(
        resolution,
        recipe_set.as_ref(),
        manual_only,
        record.and_then(|value| value.engine_config_journal()),
    ))
}

/// The manual file-install escape hatch for the availability preview: offered only
/// when a matched title is incompatible with the catalogue install path but the
/// renderer can still load RenoDX. Unsupported-settings, unmatched, blacklisted,
/// native-HDR, automatic, or external titles get `None`.
fn manual_file_install(
    manifest: &RenoDxManifest,
    facts: &MatchFacts,
    resolution: &RenoDxResolution,
) -> Option<ManualFileInstall> {
    let offered = matches!(resolution, RenoDxResolution::Incompatible { .. });
    if !offered {
        return None;
    }
    let host_kind = host_decision(primary_api(&facts.graphics))?;
    Some(ManualFileInstall {
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
