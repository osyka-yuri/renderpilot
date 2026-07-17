//! Uninstalls RenoDX from a game and cleans shared Vulkan app registration.

use std::path::PathBuf;

use renderpilot_domain::{AddonKind, GameId, InstalledAddon, InstalledAddonHostKind};

use crate::addons::game_analysis::{analyze_game, install_target_dir};
use crate::addons::records;
use crate::addons::renodx::errors;
use crate::addons::renodx::game_context::{executable_override, require_game};
use crate::addons::renodx::install::uninstall as uninstall_files;
use crate::addons::renodx::vulkan;
use crate::addons::reshade::proxy::{HostKind, host_decision, primary_api};
use crate::addons::vulkan_lock;
use crate::game_mutation_lock;
use crate::{Context, ServiceError};

/// Uninstalls RenoDX from a game, restoring files and clearing install metadata.
/// A record belonging to a different addon kind (e.g. Luma) is never touched —
/// this reports "not installed" for RenoDX exactly as if there were no record.
///
/// When the recorded game folder is still reachable, the per-game file restore
/// and the DB row delete run inside one durable `DurableFileTransaction` so a
/// crash between them recovers the exact before-state. When every declared root
/// is unreachable (deleted install, offline volume, synthetic path), file
/// restore is a best-effort no-op and the install row is cleared by a
/// metadata-only commit — uninstall must still succeed so orphaned records do
/// not stick forever.
///
/// The shared Vulkan layer cleanup is advisory and lives outside that
/// transaction (it mutates system scope, not the game folder); it runs only
/// after the per-game uninstall committed.
pub fn uninstall(context: &Context, game_id: &GameId) -> Result<(), ServiceError> {
    let guard = game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;

    let game_dir_hint = resolved_game_dir(context, game_id);
    let targets = crate::addons::renodx::mutation_targets::uninstall_targets(
        &record,
        game_dir_hint.as_deref(),
    );
    let workset = targets.resolve_workset()?;

    crate::addons::durable::run_uninstall_workset(
        crate::addons::durable::UninstallWorkset {
            context,
            guard: &guard,
            workset,
            feature: crate::addons::mutation_features::RENODX_UNINSTALL,
            game_id,
        },
        |mutation_id| {
            // Per-game files: restore the game folder (add-on, ReShade.ini,
            // backups). `game_dir_hint` only helps locate a pre-existing
            // `ReShade.ini` the record itself doesn't reference. Metadata-only:
            // engine uninstall is still invoked so any path that happens to
            // exist is cleaned; missing files are not an error.
            uninstall_files(&record, game_dir_hint.as_deref())?;
            context.storage().commit_game_mutation(
                renderpilot_storage_sqlite::GameMutationCommit {
                    game_id,
                    component_set: None,
                    baseline_inserts: &[],
                    baseline_deletes: &[],
                    addon: renderpilot_storage_sqlite::InstalledAddonMutation::Delete(
                        AddonKind::RenoDx,
                    ),
                    mutation_id,
                },
            )?;
            Ok(())
        },
        || {},
    )?;

    // Shared Vulkan layer cleanup: unregister this game's exe from
    // ReShadeApps.ini. If it was the last app, remove the empty shared
    // layer (manifest registration + directory). Then forget the advisory
    // DB row. Best-effort and outside the durable game-file transaction.
    let registered_exe = registered_vulkan_exe_for_uninstall(context, game_id, &record);
    if let Some(exe) = registered_exe.as_deref() {
        let _shared_guard = vulkan_lock::blocking_shared_vulkan_lock();
        match vulkan::unregister_app(exe) {
            Ok(true) => vulkan::forget_layer_record(context.storage()),
            Ok(false) => {}
            Err(error) => log::warn!("failed to unregister Vulkan layer app: {error}"),
        }
    }
    Ok(())
}

/// Best-effort resolved game directory, `None` if the game can no longer be
/// resolved (e.g. removed from the library) or its install path can't be
/// determined. See the call site in [`uninstall`].
fn resolved_game_dir(context: &Context, game_id: &GameId) -> Option<PathBuf> {
    let game = require_game(context, game_id).ok()?;
    let analysis = analyze_game(&game, executable_override(context, game_id).as_deref());
    install_target_dir(&analysis).ok()
}

fn registered_vulkan_exe_for_uninstall(
    context: &Context,
    game_id: &GameId,
    record: &InstalledAddon,
) -> Option<PathBuf> {
    match record.host_kind() {
        Some(InstalledAddonHostKind::SharedVulkanLayer) => {
            return record
                .registered_exe_path()
                .map(|path| PathBuf::from(path.as_str()));
        }
        Some(InstalledAddonHostKind::Proxy) => return None,
        None => {}
    }

    legacy_vulkan_exe_for_uninstall(context, game_id)
}

fn legacy_vulkan_exe_for_uninstall(context: &Context, game_id: &GameId) -> Option<PathBuf> {
    let game = require_game(context, game_id).ok()?;
    let analysis = analyze_game(&game, executable_override(context, game_id).as_deref());
    let host_kind = host_decision(primary_api(&analysis.facts.graphics))?;
    if !matches!(host_kind, HostKind::Vulkan) {
        return None;
    }
    analysis
        .primary_executable
        .as_ref()
        .map(|exe| PathBuf::from(exe.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::PathRef;
    use tempfile::tempdir;

    #[test]
    fn uninstall_reports_not_installed_for_a_luma_record_and_leaves_it_untouched() {
        let db_dir = tempdir().expect("db dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1091500").expect("game id");
        let luma_record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::Luma,
            PathRef::new(r"C:\Games\Test\Luma-Test.addon").expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&luma_record)
            .expect("seed luma record");

        let error = uninstall(&context, &game_id).expect_err("renodx uninstall must be refused");
        assert!(matches!(error, ServiceError::InvalidInput(_)));

        let still_present = records::foreign_record(&context, &game_id, AddonKind::RenoDx)
            .expect("get")
            .expect("the luma record must survive untouched");
        assert_eq!(still_present.kind(), AddonKind::Luma);
    }

    #[test]
    fn uninstall_clears_metadata_when_recorded_paths_are_unreachable() {
        let db_dir = tempdir().expect("db dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:Z:/renderpilot-missing/RenoGame").expect("game id");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new("Z:/renderpilot-missing/RenoGame/renodx-renogame.addon64").expect("path"),
        )
        .with_addon_version("snapshot-2026.06");
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed record");

        uninstall(&context, &game_id).expect("orphan uninstall must clear metadata");
        assert!(
            context
                .storage()
                .get_installed_addon(&game_id)
                .expect("query")
                .is_none(),
            "unreachable install path must still clear the install record"
        );
    }

    #[test]
    fn uninstall_clears_record_for_reachable_temp_install() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let addon_path = game_dir.path().join("renodx-temp.addon64");
        std::fs::write(&addon_path, b"addon").expect("write addon");
        let game_id = GameId::new(format!(
            "manual:{}",
            game_dir.path().to_string_lossy().replace('\\', "/")
        ))
        .expect("game id");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(addon_path.to_string_lossy().as_ref()).expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed record");

        uninstall(&context, &game_id).expect("reachable uninstall");
        assert!(!addon_path.exists(), "addon file should be removed");
        assert!(
            context
                .storage()
                .get_installed_addon(&game_id)
                .expect("query")
                .is_none()
        );
    }
}
