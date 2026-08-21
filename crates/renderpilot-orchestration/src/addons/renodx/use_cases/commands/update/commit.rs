//! Locked authorization and host-install helper for RenoDX updates.

use renderpilot_domain::{GameId, InstalledAddon};

use crate::addons::engine::{self, FileOp, InstallPlan, InstallReceipt};
use crate::addons::file_update::{
    OriginalFile, apply_replacements, persistence_failure_error, restore_originals,
    restore_originals_best_effort,
};
use crate::addons::mutation_targets::MutationTargets;
use crate::addons::renodx::tracking;
use crate::addons::renodx::use_cases::commands::update::prepare::HostInstall;
use crate::addons::renodx::use_cases::commands::update::prepare::PreparedUpdateArtifacts;
use crate::game_mutation_lock;
use crate::{Context, ServiceError};

pub(super) struct UpdateCommit<'a> {
    pub(super) context: &'a Context,
    pub(super) guard: &'a game_mutation_lock::GameMutationGuard,
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
    guards: game_mutation_lock::GameMutationBoundary,
    safety: &crate::GameMutationSafetyPermits,
    shared_update: Option<
        crate::addons::renodx::use_cases::commands::update_reshade::PreparedReShadeUpdate,
    >,
    game_commit: impl FnOnce(&game_mutation_lock::GameMutationGuard) -> Result<T, ServiceError>,
) -> Result<T, ServiceError> {
    let feature = crate::addons::mutation_features::RENODX_UPDATE;
    let authority = crate::FileSafetyAuthority::new();
    match guards {
        game_mutation_lock::GameMutationBoundary::Game(guard) => {
            if shared_update.is_some() {
                return Err(ServiceError::command_failed(
                    "a prepared shared Vulkan update requires the combined mutation boundary",
                ));
            }
            authority.authorize_game_commit(context, feature, &guard, safety.game(), || {
                game_commit(&guard)
            })
        }
        game_mutation_lock::GameMutationBoundary::GameShared(guards) => {
            let shared_update = shared_update.ok_or_else(|| {
                ServiceError::command_failed(
                    "the combined mutation boundary has no prepared shared Vulkan update",
                )
            })?;
            authority.authorize_game_shared_commit(context, feature, &guards, safety, || {
                shared_update.commit(context)?;
                game_commit(guards.game())
            })
        }
    }
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
