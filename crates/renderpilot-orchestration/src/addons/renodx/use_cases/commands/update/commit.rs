//! Locked authorization and host-install helper for RenoDX updates.

use renderpilot_domain::{GameId, InstalledAddon};
use std::path::Path;

use crate::addons::engine::{self, FileOp, InstallPlan, InstallReceipt};
use crate::addons::file_update::{
    OriginalFile, Replacement, apply_replacements, persistence_failure_error, restore_originals,
    restore_originals_best_effort,
};
use crate::addons::mutation_targets::MutationTargets;
use crate::addons::renodx::errors;
use crate::addons::renodx::tracking;
use crate::addons::renodx::use_cases::commands::update::prepare::HostInstall;
use crate::addons::renodx::use_cases::commands::update::prepare::PreparedUpdateArtifacts;
use crate::{Context, ServiceError};

use super::super::update_reshade::PreparedReShadeUpdate;

pub(super) struct CombinedUpdateRequest<'a> {
    pub(super) context: &'a Context,
    pub(super) guards: crate::mutation_boundary::GameMutationBoundary,
    pub(super) safety: &'a crate::GameMutationSafetyPermits,
    pub(super) shared_update: PreparedReShadeUpdate,
    pub(super) artifacts: PreparedUpdateArtifacts,
    pub(super) current: &'a InstalledAddon,
    pub(super) targets: MutationTargets,
    pub(super) game_id: &'a GameId,
}

pub(super) struct UpdateCommit<'a> {
    pub(super) context: &'a Context,
    pub(super) guard: &'a crate::game_mutation_lock::GameMutationGuard,
    pub(super) artifacts: PreparedUpdateArtifacts,
    pub(super) current: &'a InstalledAddon,
    pub(super) targets: MutationTargets,
    pub(super) game_id: &'a GameId,
}

pub(super) fn apply_update(commit: UpdateCommit<'_>) -> Result<(), ServiceError> {
    let UpdateCommit {
        context,
        guard,
        artifacts,
        current,
        targets,
        game_id,
    } = commit;
    crate::addons::durable::run_targets_mutation(
        crate::addons::durable::TargetsMutation {
            context,
            guard,
            targets,
            feature: crate::addons::mutation_features::RENODX_UPDATE,
            game_id,
        },
        |mutation_id| -> Result<(), ServiceError> {
            let mut originals = apply_replacements(artifacts.replacements)?;
            let host_receipt = match artifacts.host_install {
                Some(install) => match apply_host_install(install, &mut originals) {
                    Ok(receipt) => Some(receipt),
                    Err(error) => {
                        restore_originals_best_effort(&originals);
                        return Err(error);
                    }
                },
                None => None,
            };
            let refreshed = match tracking::rebuild_with_sources_and_receipt(
                current,
                artifacts.refreshed_sources,
                host_receipt.as_ref(),
                "RenoDX update rebuild",
            ) {
                Ok(refreshed) => refreshed,
                Err(error) => {
                    restore_originals_best_effort(&originals);
                    return Err(error);
                }
            };
            if let Err(error) = context.storage().commit_game_mutation(
                renderpilot_storage_sqlite::GameMutationCommit {
                    game_id,
                    component_set: None,
                    baseline_mutations: &[],
                    addon: renderpilot_storage_sqlite::InstalledAddonMutation::Upsert(&refreshed),
                    mutation_id: Some(mutation_id),
                },
            ) {
                let restore_result = restore_originals(&originals);
                return Err(persistence_failure_error(
                    error.into(),
                    std::slice::from_ref(&restore_result),
                ));
            }
            Ok(())
        },
        |_| {},
        || {},
    )
}

pub(super) fn authorize_update_commit<T>(
    context: &Context,
    guards: crate::mutation_boundary::GameMutationBoundary,
    safety: &crate::GameMutationSafetyPermits,
    game_commit: impl FnOnce(&crate::game_mutation_lock::GameMutationGuard) -> Result<T, ServiceError>,
) -> Result<T, ServiceError> {
    let feature = crate::addons::mutation_features::RENODX_UPDATE;
    let authority = crate::FileSafetyAuthority::new();
    match guards {
        crate::mutation_boundary::GameMutationBoundary::Game(guard) => authority
            .authorize_game_commit(context, feature, &guard, safety.game(), || {
                game_commit(&guard)
            }),
        crate::mutation_boundary::GameMutationBoundary::GameShared(_) => {
            Err(ServiceError::command_failed(
                "a shared Vulkan update requires the combined mutation boundary",
            ))
        }
    }
}

pub(super) fn authorize_combined_update(
    request: CombinedUpdateRequest<'_>,
) -> Result<(), ServiceError> {
    let CombinedUpdateRequest {
        context,
        guards,
        safety,
        shared_update,
        artifacts,
        current,
        targets,
        game_id,
    } = request;
    let crate::mutation_boundary::GameMutationBoundary::GameShared(guards) = guards else {
        return Err(ServiceError::command_failed(
            "a shared Vulkan update requires the combined mutation boundary",
        ));
    };
    let authority = crate::FileSafetyAuthority::new();
    let shared = shared_update.plan_locked(context)?;
    if shared.plan.is_noop() {
        // The final plan under both locks is authoritative. A shared no-op is
        // an ordinary game mutation and therefore does not consume a shared
        // safety permit or reserve SVAM.
        return authority.authorize_game_commit(
            context,
            crate::addons::mutation_features::RENODX_UPDATE,
            guards.game(),
            safety.game(),
            || {
                apply_update(UpdateCommit {
                    context,
                    guard: guards.game(),
                    artifacts,
                    current,
                    targets,
                    game_id,
                })
            },
        );
    }
    let super::super::update_reshade::PreparedSharedVulkanUpdate {
        layer_dir,
        plan,
        shared_record,
        changed: _,
    } = shared;
    authority.authorize_game_shared_commit(
        context,
        crate::addons::mutation_features::RENODX_UPDATE,
        &guards,
        safety,
        || {
            let PreparedUpdateArtifacts {
                refreshed_sources,
                replacements,
                host_install,
            } = artifacts;
            let replacement_mtimes = replacements
                .iter()
                .filter_map(|replacement| {
                    replacement
                        .mtime
                        .as_deref()
                        .map(|mtime| (replacement.path.clone(), mtime.to_owned()))
                })
                .collect::<Vec<_>>();
            let host_receipt = host_install.as_ref().map(|install| InstallReceipt {
                created_files: vec![install.game_dir.join(&install.name)],
                backed_up_files: Vec::new(),
            });
            let game_intents = update_file_intents(replacements, host_install)?;
            let composed = crate::addons::shared_vulkan_mutation::compose(None, Some(plan))?;
            let (game_scope, _) = targets.into_scope_and_paths()?;
            let roots = if game_intents.is_empty() {
                crate::addons::shared_vulkan_mutation::TrustedRoots::game_shared_without_game_files(
                    &layer_dir,
                )?
            } else {
                crate::addons::shared_vulkan_mutation::TrustedRoots::game_shared(
                    &game_scope,
                    &layer_dir,
                )?
            };
            let refreshed = tracking::rebuild_with_sources_and_receipt(
                current,
                refreshed_sources,
                host_receipt.as_ref(),
                "RenoDX combined update rebuild",
            )?;
            let mutation_id = ulid::Ulid::generate().to_string();
            let mut composed = composed;
            composed.prepend_files(game_intents)?;
            let registry = crate::addons::renodx::platform::vulkan::native_registry()
                .ok_or_else(errors::vulkan_unsupported_platform)?;
            let identity = crate::addons::shared_vulkan_mutation::MutationIdentity::new(
                &mutation_id,
                crate::addons::shared_vulkan_mutation::ScopeSpec::game_upsert(game_id, &refreshed),
                crate::addons::mutation_features::RENODX_UPDATE,
            );
            let physical = crate::addons::shared_vulkan_mutation::PhysicalParticipants::new(
                roots,
                composed,
                Some(registry),
            );
            let projection = crate::addons::shared_vulkan_mutation::CatalogProjection::new(
                renderpilot_storage_sqlite::SharedArtifactMutation::Upsert(&shared_record),
            );
            crate::addons::shared_vulkan_mutation::execute(
                crate::addons::shared_vulkan_mutation::Request::new(
                    context, identity, physical, projection,
                ),
            )?;
            for (path, mtime) in replacement_mtimes {
                crate::fs::stamp_mtime_best_effort(&path, Some(&mtime), None);
            }
            Ok(())
        },
    )
}

fn update_file_intents(
    replacements: Vec<Replacement>,
    host_install: Option<HostInstall>,
) -> Result<Vec<crate::addons::shared_vulkan_mutation::FileIntent>, ServiceError> {
    let mut intents = Vec::with_capacity(replacements.len() + usize::from(host_install.is_some()));
    for replacement in replacements {
        intents.push(crate::addons::shared_vulkan_mutation::FileIntent {
            before: read_regular_file(&replacement.path)?,
            live_path: replacement.path,
            after: Some(replacement.bytes),
        });
    }
    if let Some(install) = host_install {
        let path = install.game_dir.join(&install.name);
        intents.push(crate::addons::shared_vulkan_mutation::FileIntent {
            before: read_regular_file(&path)?,
            live_path: path,
            after: Some(install.bytes),
        });
    }
    Ok(intents)
}

fn read_regular_file(path: &Path) -> Result<Option<Vec<u8>>, ServiceError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(crate::failed(error.to_string())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(crate::failed(format!(
            "RenoDX update participant is not a regular file: {}",
            path.display()
        )));
    }
    std::fs::read(path)
        .map(Some)
        .map_err(|error| crate::failed(error.to_string()))
}

/// Installs a host artifact without an engine backup and records its previous
/// bytes for the outer transaction's rollback path.
pub(super) fn apply_host_install(
    install: HostInstall,
    originals: &mut Vec<OriginalFile>,
) -> Result<InstallReceipt, ServiceError> {
    let HostInstall {
        game_dir,
        name,
        bytes,
    } = install;
    let path = game_dir.join(&name);
    let original_bytes = if path.is_file() {
        Some(crate::fs::read_file(&path)?)
    } else {
        None
    };
    let receipt = engine::install(
        &game_dir,
        &InstallPlan {
            kind: renderpilot_domain::AddonKind::RenoDx,
            ops: vec![FileOp::Replace { name, bytes }],
        },
    )?;
    originals.push(OriginalFile {
        path,
        bytes: original_bytes,
    });
    Ok(receipt)
}
