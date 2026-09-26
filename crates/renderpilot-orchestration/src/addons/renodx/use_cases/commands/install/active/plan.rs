use std::path::PathBuf;

use renderpilot_domain::{Architecture, RenoDxReshadeIniFeature};

use crate::ServiceError;
use crate::addons::game_analysis::{GameAnalysis, analyze_game, install_target_dir};
use crate::addons::renodx::errors;
use crate::addons::renodx::game_context::{analyze_and_resolve, executable_override, require_game};
use crate::addons::renodx::matcher::{
    ResolvedInstall, generic_file_install_plan, has_unsupported_settings, resolve_external_install,
};
use crate::addons::renodx::peer::InstallCommandVariant;

use super::model::{ActiveInstallSource, ResolveActiveInstallRequest};
use super::validation;

/// Resolved game analysis, RenoDX plan, and the typed configuration feature.
pub(super) struct ActiveInstallPlan {
    pub(super) analysis: GameAnalysis,
    pub(super) plan: ResolvedInstall,
    pub(super) feature: RenoDxReshadeIniFeature,
    pub(super) variant: InstallCommandVariant,
}

/// Resolves the same catalogue or local-file plan for both active phases.
pub(super) fn resolve(
    request: &ResolveActiveInstallRequest<'_>,
) -> Result<ActiveInstallPlan, ServiceError> {
    let game = require_game(request.context, request.game_id)?;
    let override_path = executable_override(request.context, request.game_id);
    let resolved = match request.source {
        ActiveInstallSource::Catalog => {
            let (analysis, resolution) =
                analyze_and_resolve(&game, request.manifest, override_path.as_deref());
            let plan = validation::catalog_plan(resolution)?;
            ActiveInstallPlan {
                analysis,
                plan,
                feature: request.source.reshade_ini_feature(),
                variant: request.source.command_variant(),
            }
        }
        ActiveInstallSource::InstallFromFile { architecture } => {
            let analysis = analyze_game(&game, override_path.as_deref());
            ensure_game_architecture(&analysis, architecture)?;
            if has_unsupported_settings(request.manifest, &analysis.facts) {
                return Err(errors::unsupported_settings());
            }
            let plan = resolve_external_install(request.manifest, &analysis.facts)
                .or_else(|| generic_file_install_plan(&analysis.facts, architecture))
                .ok_or_else(|| {
                    errors::invalid(
                        "RenoDX cannot be installed for this game: its renderer is not Direct3D"
                            .to_owned(),
                    )
                })?;
            validation::ensure_file_architecture(architecture, plan.arch)?;
            ActiveInstallPlan {
                analysis,
                plan,
                feature: request.source.reshade_ini_feature(),
                variant: request.source.command_variant(),
            }
        }
    };
    Ok(resolved)
}

fn ensure_game_architecture(
    analysis: &GameAnalysis,
    file_architecture: Architecture,
) -> Result<(), ServiceError> {
    if let Some(game_architecture) = analysis.facts.graphics.architecture()
        && game_architecture != file_architecture
    {
        return Err(errors::invalid(format!(
            "this add-on is {} but the game is {} — download the matching add-on",
            architecture_label(file_architecture),
            architecture_label(game_architecture),
        )));
    }
    Ok(())
}

pub(super) fn target_dir(analysis: &GameAnalysis) -> Result<PathBuf, ServiceError> {
    install_target_dir(analysis)
}

pub(super) fn architecture_label(architecture: Architecture) -> &'static str {
    match architecture {
        Architecture::X64 => "64-bit",
        Architecture::X86 => "32-bit",
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{GameRepository, InstalledAddonRepository};
    use renderpilot_domain::{
        FileReceipt, GameId, GameIdentity, GameInstallation, GameProxyTopology, GameRuntime,
        Launcher, PathRef, Platform, ProxyImplementation, ProxyLink, ProxyRootPrestate, Sha256Hash,
    };
    use tempfile::tempdir;

    use super::*;
    use crate::addons::renodx::test_support::{manifest, rule, title};
    use crate::addons::renodx::types::{MatchKind, Status};
    use crate::addons::renodx::use_cases::commands::install::active::model::{
        ActiveInstallSource, ResolveActiveInstallRequest,
    };
    use crate::addons::reshade::types::ReshadeChannel;

    fn topology(game_id: &GameId, root: &std::path::Path) -> GameProxyTopology {
        let root_slot =
            PathRef::new(root.join("ReShade64.dll").to_string_lossy()).expect("root slot");
        GameProxyTopology {
            id: "optiscaler:test".to_owned(),
            game_id: game_id.clone(),
            root_slot: root_slot.clone(),
            outer: ProxyLink {
                implementation: ProxyImplementation::OptiScaler,
                path: root_slot,
                receipt: FileReceipt::owned(
                    "outer",
                    Sha256Hash::new("a".repeat(64)).expect("digest"),
                )
                .expect("receipt"),
            },
            downstream: None,
            downstream_origin: None,
            root_prestate: ProxyRootPrestate::Absent,
        }
    }

    #[test]
    fn active_catalog_and_file_install_plans_reject_unsupported_settings_before_fallback() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context =
            crate::Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:install-unsupported-settings-active").expect("game id");
        let identity = GameIdentity::new(
            game_id.clone(),
            "RenoDX Compatibility Test",
            Launcher::Steam,
        )
        .expect("identity")
        .with_external_id("install-unsupported-settings-active")
        .expect("external id");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_dir.path().to_string_lossy()).expect("game path"),
        );
        context.storage().upsert_game(&game).expect("game");

        let mut exact = title(
            "future-config-title",
            "future-config-title",
            Architecture::X64,
            Status::Working,
            vec![rule(
                MatchKind::SteamAppid,
                "install-unsupported-settings-active",
                100,
            )],
        );
        exact.has_unsupported_settings = true;
        let mut manifest = manifest(vec![exact]);
        manifest
            .generics
            .push(crate::addons::renodx::types::RenoDxGeneric {
                engine: crate::addons::renodx::types::Engine::Unity,
                status: Status::Working,
                slug: Some("unityengine".to_owned()),
                url64: Some("https://example.test/renodx-unityengine.addon64".to_owned()),
                url32: Some("https://example.test/renodx-unityengine.addon32".to_owned()),
                message: crate::addons::CatalogMessage::new("renodx.generic.unity", "Unity"),
                profile_id: Some("unity".to_owned()),
                generic_fallback: true,
                processing_path: Default::default(),
                guidance: Vec::new(),
            });

        for source in [
            ActiveInstallSource::Catalog,
            ActiveInstallSource::InstallFromFile {
                architecture: Architecture::X64,
            },
        ] {
            let request = ResolveActiveInstallRequest {
                context: &context,
                manifest: &manifest,
                game_id: &game_id,
                requested_channel: ReshadeChannel::Stable,
                selected_topology: topology(&game_id, game_dir.path()),
                source,
            };
            assert!(matches!(
                resolve(&request),
                Err(crate::ServiceError::InvalidInput(_))
            ));
        }

        assert!(
            context
                .storage()
                .get_installed_addon(&game_id)
                .expect("record")
                .is_none()
        );
    }
}
