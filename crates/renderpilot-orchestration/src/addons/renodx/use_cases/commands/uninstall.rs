//! Uninstalls RenoDX from a game and cleans shared Vulkan app registration.

use std::path::PathBuf;

use renderpilot_domain::{AddonKind, GameId, InstalledAddon, InstalledAddonHostKind};

use crate::addons::game_analysis::{analyze_game, install_target_dir};
use crate::addons::records;
use crate::addons::renodx::errors;
use crate::addons::renodx::game_context::{executable_override, require_game};
use crate::addons::renodx::install::PreparedRenoDxUninstall;
use crate::addons::reshade::proxy::{HostKind, host_decision, primary_api};
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
/// A shared Vulkan registration is folded into the same game/shared durable
/// transaction as the per-game restore. Proxy installs retain the game-only
/// transaction because they have no shared system participant.
pub fn uninstall(context: &Context, game_id: &GameId) -> Result<(), ServiceError> {
    loop {
        let guard = crate::mutation_boundary::enter_game_mutation_boundary(context, game_id)?;
        let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
            .ok_or_else(errors::not_installed)?;
        if registered_vulkan_exe_for_uninstall(context, game_id, &record).is_none() {
            return uninstall_locked(context, &guard, game_id);
        }
        drop(guard);

        let guards =
            crate::mutation_boundary::enter_game_shared_mutation_boundary(context, game_id)?;
        let current = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
            .ok_or_else(errors::not_installed)?;
        if registered_vulkan_exe_for_uninstall(context, game_id, &current).is_none() {
            drop(guards);
            continue;
        }
        return uninstall_shared_locked(context, &guards, game_id, &current);
    }
}

pub(crate) fn uninstall_shared_locked(
    context: &Context,
    guards: &crate::mutation_boundary::GameSharedMutationGuards,
    game_id: &GameId,
    record: &renderpilot_domain::InstalledAddon,
) -> Result<(), ServiceError> {
    let mut plan =
        PreparedRenoDxUninstall::prepare(record, resolved_game_dir(context, game_id).as_deref());
    let affected_paths = plan.affected_paths();
    let scope = crate::file_mutation::MutationScope::try_from_reachable_roots(
        affected_paths
            .iter()
            .filter_map(|path| path.parent().map(PathBuf::from)),
    )?;
    if let Some(scope) = &scope {
        plan.retain_reachable(Some(scope));
    } else {
        plan.retain_reachable(None);
    }

    let registered_exe =
        registered_vulkan_exe_for_uninstall(context, game_id, record).ok_or_else(|| {
            errors::invalid("Vulkan uninstall has no registered executable".to_owned())
        })?;
    let layer_dir = crate::addons::renodx::platform::vulkan::program_data::layer_dir()
        .ok_or_else(errors::vulkan_unsupported_platform)?;
    let registry = crate::addons::renodx::platform::vulkan::native_registry()
        .ok_or_else(errors::vulkan_unsupported_platform)?;
    let observation = renderpilot_platform_windows::vulkan_layer::observe_shared_vulkan_layer(
        registry, &layer_dir,
    )
    .map_err(|error| errors::failed(format!("failed to inspect shared Vulkan layer: {error}")))?;
    let shared_plan = renderpilot_platform_windows::vulkan_layer::plan_unregister_app_only(
        observation,
        &registered_exe,
    )
    .map_err(|error| errors::failed(error.to_string()))?;

    if shared_plan.unregister_outcome
        == Some(renderpilot_platform_windows::vulkan_layer::AppUnregisterOutcome::TargetAbsent)
    {
        return commit_game_uninstall(context, guards.game(), game_id, &plan, scope.as_ref());
    }

    let game_intents = plan.take_file_intents()?;
    let has_game_files = !game_intents.is_empty();
    let removes_canonical_layer = shared_plan.authorizes_canonical_layer_removal();
    let mut composed = crate::addons::shared_vulkan_mutation::compose(None, Some(shared_plan))?;
    // Unregister the game from the shared loader before removing its local
    // payload. SVAM recovery reverses this manifest order when rolling back.
    composed.extend_files(game_intents)?;
    let roots = if !has_game_files {
        crate::addons::shared_vulkan_mutation::TrustedRoots::game_shared_without_game_files(
            &layer_dir,
        )?
    } else {
        match scope.as_ref() {
            Some(game_scope) => crate::addons::shared_vulkan_mutation::TrustedRoots::game_shared(
                game_scope, &layer_dir,
            )?,
            None => {
                return Err(errors::invalid(
                    "reachable game participants have no mutation scope".to_owned(),
                ));
            }
        }
    };
    let shared_artifact = if removes_canonical_layer {
        renderpilot_storage_sqlite::SharedArtifactMutation::Delete(
            renderpilot_domain::SharedArtifactKind::RenoDxVulkanLayer,
        )
    } else {
        renderpilot_storage_sqlite::SharedArtifactMutation::Keep
    };
    let id = ulid::Ulid::generate().to_string();
    let identity = crate::addons::shared_vulkan_mutation::MutationIdentity::new(
        &id,
        crate::addons::shared_vulkan_mutation::ScopeSpec::game_delete(game_id, AddonKind::RenoDx),
        crate::addons::mutation_features::RENODX_UNINSTALL,
    );
    let physical = crate::addons::shared_vulkan_mutation::PhysicalParticipants::new(
        roots,
        composed,
        Some(registry),
    );
    let projection = crate::addons::shared_vulkan_mutation::CatalogProjection::new(shared_artifact);
    crate::addons::shared_vulkan_mutation::execute(
        crate::addons::shared_vulkan_mutation::Request::new(
            context, identity, physical, projection,
        ),
    )?;
    plan.remove_logs_best_effort();
    Ok(())
}

/// Uninstalls RenoDX while a compound operation owns the game mutation boundary.
pub(crate) fn uninstall_locked(
    context: &Context,
    guard: &game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
) -> Result<(), ServiceError> {
    if guard.game_id() != game_id {
        return Err(ServiceError::invalid_input(
            "RenoDX uninstall guard does not match the requested game",
        ));
    }
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;
    if registered_vulkan_exe_for_uninstall(context, game_id, &record).is_some() {
        return Err(ServiceError::invalid_input(
            "shared Vulkan uninstall requires the combined mutation boundary",
        ));
    }

    let mut plan =
        PreparedRenoDxUninstall::prepare(&record, resolved_game_dir(context, game_id).as_deref());
    let affected_paths = plan.affected_paths();
    let roots = affected_paths
        .iter()
        .filter_map(|path| path.parent().map(PathBuf::from));
    match crate::file_mutation::MutationScope::try_from_reachable_roots(roots)? {
        Some(scope) => {
            plan.retain_reachable(Some(&scope));
            let affected_paths = plan.affected_paths();
            if affected_paths.is_empty() {
                commit_prepared_uninstall(context, game_id, &plan, None)?;
            } else {
                crate::file_mutation::run_durable_mutation(
                    crate::file_mutation::DurableMutation {
                        context,
                        guard,
                        scope: &scope,
                        feature: crate::addons::mutation_features::RENODX_UNINSTALL,
                        subject_id: Some(game_id.as_str()),
                        paths: affected_paths,
                    },
                    |mutation_id| {
                        commit_prepared_uninstall(context, game_id, &plan, Some(mutation_id))
                    },
                    |_| {},
                    || {},
                )?;
            }
        }
        None => {
            plan.retain_reachable(None);
            commit_prepared_uninstall(context, game_id, &plan, None)?;
        }
    }
    plan.remove_logs_best_effort();
    Ok(())
}

fn commit_prepared_uninstall(
    context: &Context,
    game_id: &GameId,
    plan: &PreparedRenoDxUninstall,
    mutation_id: Option<&str>,
) -> Result<(), ServiceError> {
    plan.apply()?;
    context
        .storage()
        .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
            game_id,
            component_set: None,
            baseline_mutations: &[],
            addon: renderpilot_storage_sqlite::InstalledAddonMutation::Delete(AddonKind::RenoDx),
            mutation_id,
        })?;
    Ok(())
}

fn commit_game_uninstall(
    context: &Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    plan: &PreparedRenoDxUninstall,
    scope: Option<&crate::file_mutation::MutationScope>,
) -> Result<(), ServiceError> {
    let affected_paths = plan.affected_paths();
    match scope {
        Some(scope) if !affected_paths.is_empty() => {
            crate::file_mutation::run_durable_mutation(
                crate::file_mutation::DurableMutation {
                    context,
                    guard,
                    scope,
                    feature: crate::addons::mutation_features::RENODX_UNINSTALL,
                    subject_id: Some(game_id.as_str()),
                    paths: affected_paths,
                },
                |mutation_id| commit_prepared_uninstall(context, game_id, plan, Some(mutation_id)),
                |_| {},
                || {},
            )?;
        }
        _ => commit_prepared_uninstall(context, game_id, plan, None)?,
    }
    plan.remove_logs_best_effort();
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

pub(crate) fn registered_vulkan_exe_for_uninstall(
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

    #[test]
    fn game_only_entry_rejects_shared_vulkan_before_mutating_files_or_catalog() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let addon_path = game_dir.path().join("renodx-vulkan.addon64");
        std::fs::write(&addon_path, b"addon").expect("write addon");
        let game_id = GameId::new(format!(
            "manual:{}",
            game_dir.path().to_string_lossy().replace('\\', "/")
        ))
        .expect("game id");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(addon_path.to_string_lossy().as_ref()).expect("addon path"),
        )
        .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
        .with_registered_exe_path(
            PathRef::new(game_dir.path().join("game.exe").to_string_lossy().as_ref())
                .expect("executable path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed record");
        let guard = crate::game_mutation_lock::blocking_lock(&game_id);

        let error = uninstall_locked(&context, &guard, &game_id)
            .expect_err("game-only entry must reject a shared mutation");

        assert!(matches!(error, ServiceError::InvalidInput(_)));
        assert_eq!(std::fs::read(&addon_path).expect("addon remains"), b"addon");
        assert!(
            context
                .storage()
                .get_installed_addon(&game_id)
                .expect("query")
                .is_some()
        );
    }
}
