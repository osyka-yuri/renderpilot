/// Queries RenoDX availability for a specific game.
use std::path::{Path, PathBuf};

use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, InstalledAddonHostKind, RenoDxInstallState,
};

use crate::Context;
use crate::ServiceError;

use crate::addons::anticheat::{RiskSeverity, assess_risk};
use crate::addons::availability_pipeline::{self, AvailabilityPreflight};
use crate::addons::game_analysis::{GameAnalysis, install_target_dir};
use crate::addons::renodx::dto::availability::*;
use crate::addons::renodx::game_context::analyze_and_resolve;
use crate::addons::renodx::matcher::{
    MatchFacts, RenoDxResolution, file_installable, matched_slug,
};
use crate::addons::renodx::reconciliation::{self, OrphanedInstall};
use crate::addons::renodx::source;
use crate::addons::renodx::tracking;
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::renodx::vulkan;
use crate::addons::reshade::proxy::{HostKind, host_decision, primary_api};
use crate::addons::reshade::types::{ReshadeChannel, ReshadeConfig};

use super::host_report::{self, ReshadeReport};

/// Previews whether RenoDX can be installed for the game, without changing disk.
pub fn availability(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
) -> Result<AvailabilityReport, ServiceError> {
    let AvailabilityPreflight {
        mut record,
        game,
        blocked,
        analysis,
        resolution,
        ..
    } = availability_pipeline::preflight(
        context,
        game_id,
        AddonKind::RenoDx,
        manifest,
        analyze_and_resolve,
    )?;
    let scan_dir = Path::new(game.install_path().as_str());
    let report = |record: Option<&InstalledAddon>| {
        host_report::reshade_report(&analysis, &resolution, record, &manifest.reshade)
    };
    let mut host_report = report(record.as_ref());

    if blocked.is_none()
        && record.is_none()
        && let Some(candidate) = orphaned_install_candidate(
            game_id,
            &analysis,
            &resolution,
            &host_report,
            &manifest.reshade,
        )
    {
        record = reconciliation::reconcile_orphaned_install(context, &candidate)?;

        // The freshly adopted record carries the advisory channel and tracking
        // sources the host report needs for accurate status/actions.
        host_report = report(record.as_ref());
    }

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
                notes_keys: plan.notes_keys,
                host_kind: plan.host_kind,
            },
            RenoDxResolution::External {
                url,
                label_key,
                file_install,
            } => AvailabilityOutcome::External {
                url,
                label_key,
                file_install: file_install.map(|fi| ExternalFileInstall {
                    confidence: fi.confidence,
                    risk: assess_risk(scan_dir, RiskSeverity::Info),
                    notes_keys: fi.notes_keys,
                    host_kind: fi.host_kind,
                }),
            },
            RenoDxResolution::NativeHdr => AvailabilityOutcome::NativeHdr,
            RenoDxResolution::Incompatible { reason } => {
                AvailabilityOutcome::Incompatible { reason }
            }
            RenoDxResolution::Unsupported { reason } => AvailabilityOutcome::Blacklisted { reason },
            RenoDxResolution::NoMatch => AvailabilityOutcome::Unsupported,
        }
    };

    Ok(AvailabilityReport {
        state,
        host_detection: host_report.detection,
        host_facts: host_report.facts,
        actions: host_report.actions,
        reshade_stable_supported: manifest.reshade.supports_channel(ReshadeChannel::Stable),
        renodx_addon: host_report.addon,
        outcome,
        manual_install,
        vulkan_layer: vulkan::layer_report(),
    })
}

fn orphaned_install_candidate(
    game_id: &GameId,
    analysis: &GameAnalysis,
    resolution: &RenoDxResolution,
    host_report: &ReshadeReport,
    reshade_config: &ReshadeConfig,
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

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use std::assert_matches;

    use super::*;
    #[cfg(windows)]
    use crate::addons::records;
    use crate::addons::renodx::matcher::IncompatibilityReason;
    use crate::addons::renodx::test_support::manifest;
    #[cfg(windows)]
    use crate::addons::renodx::test_support::{
        MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports, rule, title,
    };
    #[cfg(windows)]
    use renderpilot_application::{GameRepository, InstalledAddonRepository};
    use renderpilot_domain::{ExeGraphicsInfo, GraphicsApi, Launcher};
    #[cfg(windows)]
    use renderpilot_domain::{GameIdentity, GameInstallation, GameRuntime, PathRef, Platform};
    use tempfile::tempdir;

    fn directx_facts() -> MatchFacts {
        MatchFacts {
            launcher: Launcher::Steam,
            external_id: Some("1091500".to_owned()),
            exe_file_name: Some("game.exe".to_owned()),
            exe_sha256: None,
            engine: None,
            graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D11], Some(Architecture::X64))
                .with_graphics_dlls(vec!["dxgi.dll".to_owned()]),
        }
    }

    #[cfg(windows)]
    fn full_reshade_host_bytes() -> Vec<u8> {
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
        )
    }

    #[test]
    fn manual_install_is_not_offered_for_unmatched_games() {
        let dir = tempdir().expect("tempdir");
        let report = manual_file_install(
            &manifest(Vec::new()),
            &directx_facts(),
            &RenoDxResolution::NoMatch,
            dir.path(),
        );

        assert!(report.is_none());
    }

    #[test]
    fn manual_install_can_be_offered_for_matched_incompatible_directx_games() {
        let dir = tempdir().expect("tempdir");
        let report = manual_file_install(
            &manifest(Vec::new()),
            &directx_facts(),
            &RenoDxResolution::Incompatible {
                reason: IncompatibilityReason::ArchUnknown,
            },
            dir.path(),
        );

        assert!(report.is_some());
    }

    #[test]
    #[cfg(windows)]
    fn availability_auto_adopts_proxy_install_after_db_loss() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1091500").expect("game id");
        let exe_path = game_dir.path().join("Game.exe");

        std::fs::write(
            &exe_path,
            build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
        )
        .expect("write exe");
        std::fs::write(game_dir.path().join("dxgi.dll"), full_reshade_host_bytes())
            .expect("write host");
        std::fs::write(game_dir.path().join("renodx-cp2077.addon64"), b"addon")
            .expect("write addon");
        std::fs::write(
            game_dir.path().join("ReShade.ini"),
            "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
        )
        .expect("write ini");

        let identity = GameIdentity::new(game_id.clone(), "Cyberpunk 2077", Launcher::Steam)
            .expect("identity")
            .with_external_id("1091500")
            .expect("external id");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_dir.path().to_string_lossy().replace('\\', "/"))
                .expect("install path"),
        )
        .with_executable_candidate(
            PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
        );
        context.storage().upsert_game(&game).expect("seed game");

        let mut manifest = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            crate::addons::renodx::types::Status::Working,
            vec![rule(
                crate::addons::renodx::types::MatchKind::SteamAppid,
                "1091500",
                100,
            )],
        )]);
        manifest.reshade.stable = None;

        let report = availability(&context, &manifest, &game_id).expect("availability");

        assert_matches!(report.state, RenoDxInstallState::Installed { .. });
        assert!(report.actions.install.is_none());
        assert!(report.actions.use_existing.is_some());
        let record = records::record_of_kind(&context, &game_id, AddonKind::RenoDx)
            .expect("read adopted record")
            .expect("adopted record");
        assert!(record.installed_at().is_some());
        assert_eq!(record.addon_version(), None);
    }

    #[test]
    #[cfg(windows)]
    fn availability_auto_adopts_proxy_install_with_dlss_fix_companion() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        // A distinct appid from the other adoption tests in this module — the
        // per-game operation lock is a global `static`, so tests sharing one ID
        // would contend for the same lock when `cargo test` runs them in parallel.
        let game_id = GameId::new("steam:1091501").expect("game id");
        let exe_path = game_dir.path().join("Game.exe");

        std::fs::write(
            &exe_path,
            build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
        )
        .expect("write exe");
        std::fs::write(game_dir.path().join("dxgi.dll"), full_reshade_host_bytes())
            .expect("write host");
        std::fs::write(game_dir.path().join("renodx-cp2077.addon64"), b"addon")
            .expect("write addon");
        // The DLSS-Fix companion, co-located with the main addon.
        std::fs::write(game_dir.path().join("renodx-dlssfix.addon64"), b"dlssfix")
            .expect("write dlssfix");
        std::fs::write(
            game_dir.path().join("ReShade.ini"),
            "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
        )
        .expect("write ini");

        let identity = GameIdentity::new(game_id.clone(), "Cyberpunk 2077", Launcher::Steam)
            .expect("identity")
            .with_external_id("1091501")
            .expect("external id");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_dir.path().to_string_lossy().replace('\\', "/"))
                .expect("install path"),
        )
        .with_executable_candidate(
            PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
        );
        context.storage().upsert_game(&game).expect("seed game");

        let mut manifest = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            crate::addons::renodx::types::Status::Working,
            vec![rule(
                crate::addons::renodx::types::MatchKind::SteamAppid,
                "1091501",
                100,
            )],
        )]);
        manifest.reshade.stable = None;

        let report = availability(&context, &manifest, &game_id).expect("availability");

        assert_matches!(report.state, RenoDxInstallState::Installed { .. });

        let record = records::record_of_kind(&context, &game_id, AddonKind::RenoDx)
            .expect("read adopted record")
            .expect("adopted record");

        // Symptom 1 fixed: the adopted addon-file path (and its digest) come from
        // the real main addon, not the DLSS-Fix file.
        assert_eq!(
            record.addon_file().file_name(),
            Some("renodx-cp2077.addon64")
        );
        let addon_source = record
            .tracked_sources()
            .iter()
            .find(|s| s.role() == renderpilot_domain::TrackedSourceRole::AddonPayload)
            .expect("addon source recorded");
        let real_addon_digest =
            renderpilot_detection::sha256_file(&game_dir.path().join("renodx-cp2077.addon64"))
                .expect("hash real addon")
                .to_string();
        assert_eq!(addon_source.digest(), real_addon_digest);

        // Symptom 2 fixed: DLSS-Fix is recognized as installed.
        assert!(record.has_dlss_fix());
    }

    #[test]
    #[cfg(windows)]
    fn availability_does_not_adopt_a_stray_addon_file_under_the_wrong_name() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1091502").expect("game id");
        let exe_path = game_dir.path().join("Game.exe");

        std::fs::write(
            &exe_path,
            build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
        )
        .expect("write exe");
        std::fs::write(game_dir.path().join("dxgi.dll"), full_reshade_host_bytes())
            .expect("write host");
        // No renodx-cp2077.addon64 (the resolved slug's exact expected name).
        // Only an unrelated add-on file sits in the folder — must NOT be
        // mistaken for this game's add-on.
        std::fs::write(game_dir.path().join("renodx-othertitle.addon64"), b"addon")
            .expect("write stray addon");
        std::fs::write(
            game_dir.path().join("ReShade.ini"),
            "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
        )
        .expect("write ini");

        let identity = GameIdentity::new(game_id.clone(), "Cyberpunk 2077", Launcher::Steam)
            .expect("identity")
            .with_external_id("1091502")
            .expect("external id");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_dir.path().to_string_lossy().replace('\\', "/"))
                .expect("install path"),
        )
        .with_executable_candidate(
            PathRef::new(exe_path.to_string_lossy().replace('\\', "/")).expect("exe path"),
        );
        context.storage().upsert_game(&game).expect("seed game");

        let mut manifest = manifest(vec![title(
            "cp2077",
            "cp2077",
            Architecture::X64,
            crate::addons::renodx::types::Status::Working,
            vec![rule(
                crate::addons::renodx::types::MatchKind::SteamAppid,
                "1091502",
                100,
            )],
        )]);
        manifest.reshade.stable = None;

        let report = availability(&context, &manifest, &game_id).expect("availability");

        assert_eq!(report.state, RenoDxInstallState::NotInstalled);
        // Raw repository read on purpose: asserts no record of ANY kind was created.
        assert!(
            context
                .storage()
                .get_installed_addon(&game_id)
                .expect("read record")
                .is_none()
        );
    }
}
