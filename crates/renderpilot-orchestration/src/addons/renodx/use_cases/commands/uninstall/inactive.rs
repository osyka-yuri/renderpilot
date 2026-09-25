use std::path::PathBuf;

use renderpilot_domain::{AddonKind, GameId, InstalledAddon};

use crate::addons::game_analysis::{analyze_game, install_target_dir};
use crate::addons::records;
use crate::addons::renodx::errors;
use crate::addons::renodx::game_context::{executable_override, require_game};
use crate::addons::renodx::install::PreparedRenoDxUninstall;
use crate::game_mutation_lock;
use crate::{Context, ServiceError};

pub(super) fn uninstall_shared_locked(
    context: &Context,
    guards: &crate::mutation_boundary::GameSharedMutationGuards,
    game_id: &GameId,
    record: &InstalledAddon,
) -> Result<(), ServiceError> {
    let mut plan =
        PreparedRenoDxUninstall::prepare(record, resolved_game_dir(context, game_id).as_deref())?;
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
        super::route::registered_vulkan_exe_for_uninstall(record).ok_or_else(|| {
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
        return commit_game_uninstall(
            context,
            guards.game(),
            game_id,
            record,
            &plan,
            scope.as_ref(),
        );
    }

    let game_intents = plan.take_file_intents()?;
    let has_game_files = !game_intents.is_empty();
    let removes_canonical_layer = shared_plan.authorizes_canonical_layer_removal();
    let mut composed = crate::addons::shared_vulkan_mutation::compose(None, Some(shared_plan))?;
    // Unregister the game from the shared loader before removing its local
    // payload. SVAM recovery reverses this manifest order when rolling back.
    composed.extend_files(game_intents)?;
    let roots = if has_game_files {
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
    } else {
        crate::addons::shared_vulkan_mutation::TrustedRoots::game_shared_without_game_files(
            &layer_dir,
        )?
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
    crate::addons::engine_config::service::release_record(
        context.storage(),
        game_id,
        record,
        &format!("renodx-release-{}", ulid::Ulid::generate()),
    )
    .map_err(|error| {
        ServiceError::command_failed(format!(
            "RenoDX Engine.ini release blocked uninstall: {error}"
        ))
    })?;
    crate::addons::shared_vulkan_mutation::execute(
        crate::addons::shared_vulkan_mutation::Request::new(
            context, identity, physical, projection,
        ),
    )?;
    plan.remove_logs_best_effort();
    Ok(())
}

pub(super) fn uninstall_locked(
    context: &Context,
    guard: &game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
) -> Result<(), ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;
    if super::route::registered_vulkan_exe_for_uninstall(&record).is_some() {
        return Err(ServiceError::invalid_input(
            "shared Vulkan uninstall requires the combined mutation boundary",
        ));
    }

    let mut plan =
        PreparedRenoDxUninstall::prepare(&record, resolved_game_dir(context, game_id).as_deref())?;
    let affected_paths = plan.affected_paths();
    let roots = affected_paths
        .iter()
        .filter_map(|path| path.parent().map(PathBuf::from));
    if let Some(scope) = crate::file_mutation::MutationScope::try_from_reachable_roots(roots)? {
        plan.retain_reachable(Some(&scope));
        let affected_paths = plan.affected_paths();
        if affected_paths.is_empty() {
            commit_prepared_uninstall(context, game_id, &record, &plan, None)?;
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
                    commit_prepared_uninstall(context, game_id, &record, &plan, Some(mutation_id))
                },
                |()| {},
                || {},
            )?;
        }
    } else {
        plan.retain_reachable(None);
        commit_prepared_uninstall(context, game_id, &record, &plan, None)?;
    }
    plan.remove_logs_best_effort();
    Ok(())
}

fn commit_prepared_uninstall(
    context: &Context,
    game_id: &GameId,
    record: &InstalledAddon,
    plan: &PreparedRenoDxUninstall,
    mutation_id: Option<&str>,
) -> Result<(), ServiceError> {
    crate::addons::engine_config::service::release_record(
        context.storage(),
        game_id,
        record,
        &format!("renodx-release-{}", ulid::Ulid::generate()),
    )
    .map_err(|error| {
        ServiceError::command_failed(format!(
            "RenoDX Engine.ini release blocked uninstall: {error}"
        ))
    })?;
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
    record: &InstalledAddon,
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
                |mutation_id| {
                    commit_prepared_uninstall(context, game_id, record, plan, Some(mutation_id))
                },
                |()| {},
                || {},
            )?;
        }
        _ => commit_prepared_uninstall(context, game_id, record, plan, None)?,
    }
    plan.remove_logs_best_effort();
    Ok(())
}

fn resolved_game_dir(context: &Context, game_id: &GameId) -> Option<PathBuf> {
    let game = require_game(context, game_id).ok()?;
    let analysis = analyze_game(&game, executable_override(context, game_id).as_deref());
    install_target_dir(&analysis).ok()
}
