//! Luma game-context: the shared game loaders plus the manifest-typed
//! analyze-and-resolve step.

use std::path::Path;

use renderpilot_application::GameRepository;
use renderpilot_domain::{GameId, GameInstallation};

use crate::addons::game_analysis::{GameAnalysis, analyze_game};
use crate::{Context, ServiceError};

use super::matcher::{LumaResolution, resolve};
use super::types::LumaManifest;

pub(super) use crate::addons::game_context::{executable_override, require_game};

/// Returns the manual DX11 launch argument required by Luma's Generic UE mod
/// when the game is Unreal Engine and its executable imports D3D12.
///
/// D3D12 imports are only an advisory signal: they are enough to show a manual
/// `-dx11` callout, but never to mutate a game's or a launcher's configuration.
/// Dedicated Luma profiles must declare their own launch arguments in the
/// manifest instead of receiving this generic requirement.
///
/// Callers must pass whether the matched title is a Generic Unreal profile
/// (see [`super::types::LumaTitle::is_generic_unreal`]); Unity engine profiles
/// and dedicated game builds never receive an implicit `-dx11`.
fn compute_ue_dx11_launch_arg(
    facts: &crate::addons::matching::MatchFacts,
    is_generic_unreal_profile: bool,
) -> Option<&'static str> {
    use renderpilot_domain::GraphicsApi;

    let is_unreal = matches!(
        facts.engine,
        Some(crate::addons::matching::Engine::Unreal)
            | Some(crate::addons::matching::Engine::UnrealExtended)
    );
    let has_d3d12 = facts.graphics.apis().contains(&GraphicsApi::D3D12);

    if is_generic_unreal_profile && is_unreal && has_d3d12 {
        Some("-dx11")
    } else {
        None
    }
}

/// Inspects the game on disk and resolves it against the manifest in one step.
pub(super) fn analyze_and_resolve(
    game: &GameInstallation,
    manifest: &LumaManifest,
    override_path: Option<&Path>,
) -> (GameAnalysis, LumaResolution) {
    let analysis = analyze_game(game, override_path);
    let resolution = resolve(manifest, &analysis.facts);
    (analysis, resolution)
}

/// Pure launch-args derivation from already-resolved analysis + resolution.
/// Canonical home for manifest arguments plus the manual DX11 requirement for
/// Generic UE profiles that import D3D12.
#[must_use]
pub(super) fn effective_launch_args(
    analysis: &GameAnalysis,
    resolution: &LumaResolution,
) -> Vec<String> {
    let LumaResolution::Installable(plan) = resolution else {
        return Vec::new();
    };

    launch_args_for_profile(
        &plan.launch_args,
        &analysis.facts,
        plan.profile.is_generic_unreal(),
    )
}

fn launch_args_for_profile(
    declared_args: &[String],
    facts: &crate::addons::matching::MatchFacts,
    is_generic_unreal_profile: bool,
) -> Vec<String> {
    let mut args = declared_args.to_vec();

    // This list is displayed as a manual callout only. It is never written to
    // launcher settings, shortcuts, Engine.ini, or the game executable.
    if let Some(extra) = compute_ue_dx11_launch_arg(facts, is_generic_unreal_profile)
        && !args.iter().any(|a| a == extra)
    {
        args.push(extra.to_owned());
    }

    args
}

/// The launch arguments a matched title requires, re-resolved from the manifest
/// at query time rather than read from the install record (which never
/// persists them — see [`renderpilot_domain::LumaInstallState::Installed::launch_args`]).
/// Empty when the game can no longer be resolved (e.g. removed from the
/// library) or no longer matches an installable title.
pub(super) fn resolve_launch_args(
    context: &Context,
    manifest: &LumaManifest,
    game_id: &GameId,
) -> Result<Vec<String>, ServiceError> {
    let Some(game) = context.storage().find_game(game_id)? else {
        return Ok(Vec::new());
    };
    let override_path = executable_override(context, game_id);
    let (analysis, resolution) = analyze_and_resolve(&game, manifest, override_path.as_deref());
    Ok(effective_launch_args(&analysis, &resolution))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::matching::{Engine, MatchFacts};
    use renderpilot_domain::{Architecture, ExeGraphicsInfo, GraphicsApi, Launcher};

    fn make_facts(engine: Option<Engine>, apis: &[GraphicsApi]) -> MatchFacts {
        MatchFacts {
            launcher: Launcher::Steam,
            external_id: None,
            exe_file_name: None,
            exe_sha256: None,
            engine,
            graphics: ExeGraphicsInfo::new(apis.to_vec(), Some(Architecture::X64)),
        }
    }

    #[test]
    fn generic_ue_d3d12_gets_dx11_arg() {
        let facts = make_facts(Some(Engine::Unreal), &[GraphicsApi::D3D12]);
        assert_eq!(launch_args_for_profile(&[], &facts, true), vec!["-dx11"]);
    }

    #[test]
    fn generic_ue_with_only_d3d11_does_not_get_extra_arg() {
        let facts = make_facts(Some(Engine::Unreal), &[GraphicsApi::D3D11]);
        assert!(launch_args_for_profile(&[], &facts, true).is_empty());
    }

    #[test]
    fn generic_non_unreal_d3d12_gets_no_arg() {
        let facts = make_facts(None, &[GraphicsApi::D3D12]);
        assert!(launch_args_for_profile(&[], &facts, true).is_empty());
    }

    #[test]
    fn generic_ue_d3d12_and_d3d11_still_gets_arg() {
        let facts = make_facts(
            Some(Engine::Unreal),
            &[GraphicsApi::D3D12, GraphicsApi::D3D11],
        );
        assert_eq!(launch_args_for_profile(&[], &facts, true), vec!["-dx11"]);
    }

    #[test]
    fn dedicated_ue_d3d12_does_not_get_an_implicit_dx11_arg() {
        let facts = make_facts(Some(Engine::Unreal), &[GraphicsApi::D3D12]);
        assert!(launch_args_for_profile(&[], &facts, false).is_empty());
    }

    #[test]
    fn explicit_dx11_is_not_duplicated_for_a_generic_profile() {
        let facts = make_facts(Some(Engine::Unreal), &[GraphicsApi::D3D12]);
        assert_eq!(
            launch_args_for_profile(&["-dx11".to_owned()], &facts, true),
            vec!["-dx11"]
        );
    }
}
