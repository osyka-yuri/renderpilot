//! Switches the recorded ReShade channel for RenoDX installs.
//!
//! Proxy host switches use the same 3-phase contract as RenoDX update: snapshot
//! under the game lock, network fetch unlocked, revalidate + durable apply under
//! lock. Same-channel metadata heal stays lock-only (no network). Shared Vulkan
//! channel switches publish the shared files and game-owned catalog projection
//! through one combined durable transaction.

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{AddonKind, GameId, InstalledAddonHostKind, RenoDxInstallState};

use crate::addons::engine::InstallReceipt;
use crate::addons::file_update::{
    Replacement, apply_replacements, persistence_failure_error, restore_originals,
    restore_originals_best_effort,
};
use crate::addons::progress::emit_tool_finalizing;
use crate::addons::records;
use crate::addons::renodx::errors;
use crate::addons::renodx::tracking;
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::reshade::fetch::fetch_reshade_from_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::addons::reshade::update::host_binary_source;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

mod record;
mod snapshot;

#[cfg(test)]
mod tests;

use record::rebuild_proxy_switch_record;
use snapshot::{
    ChannelSwitchPhase1, ensure_proxy_channel_switch_matches, ensure_target_channel,
    resolve_channel_switch_phase1,
};

/// Complete request for a RenoDX ReShade channel switch.
pub struct SwitchChannelRequest<'a> {
    /// Application services and storage.
    pub context: &'a Context,
    /// RenoDX manifest used to resolve the current host.
    pub manifest: &'a RenoDxManifest,
    /// ReShade sources containing the requested channel.
    pub reshade_sources: &'a ReshadeSourceCatalog,
    /// Game whose ReShade channel is being changed.
    pub game_id: &'a GameId,
    /// ReShade release channel to install.
    pub target_channel: ReshadeChannel,
    /// Fresh permits for every mutation scope the resolved host may require.
    pub safety: crate::GameMutationSafetyPermits,
    /// Optional download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

/// Switches the recorded ReShade host binary artifact between stable and nightly.
pub async fn switch_reshade_channel(
    request: SwitchChannelRequest<'_>,
) -> Result<RenoDxInstallState, ServiceError> {
    let SwitchChannelRequest {
        context,
        manifest,
        reshade_sources,
        game_id,
        target_channel,
        safety,
        progress,
    } = request;
    ensure_target_channel(reshade_sources, target_channel)?;

    // Phase 1: resolve under the per-game lock. A same-channel metadata heal
    // has no unlocked prepare phase, so it crosses the safety barrier while
    // this guard is still held.
    let guard =
        crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
    let phase1 =
        resolve_channel_switch_phase1(context, manifest, reshade_sources, game_id, target_channel)?;

    match phase1 {
        ChannelSwitchPhase1::Healed(healed) => crate::FileSafetyAuthority::new()
            .authorize_game_commit(
                context,
                crate::addons::mutation_features::RENODX_SWITCH_RESHADE_CHANNEL,
                &guard,
                safety.game(),
                || {
                    context.storage().upsert_installed_addon(&healed)?;
                    Ok(tracking::install_state_from_record(&healed))
                },
            ),
        ChannelSwitchPhase1::SharedVulkan { record } => {
            drop(guard);
            let prepared = crate::addons::renodx::use_cases::commands::update_reshade::PreparedReShadeUpdate::prepare(
                reshade_sources,
                target_channel,
                progress,
            )
            .await?;
            let guards = crate::mutation_boundary::enter_game_shared_mutation_boundary_async(
                context, game_id,
            )
            .await?;
            let current = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
                .ok_or_else(errors::not_installed)?;
            if current != record
                || !matches!(
                    current.host_kind(),
                    Some(InstalledAddonHostKind::SharedVulkanLayer)
                )
            {
                return Err(errors::state_changed_retry_update());
            }
            let updated = current.with_reshade_channel(target_channel.as_str());
            let shared = prepared.plan_locked(context)?;
            if shared.plan.is_noop() {
                return crate::FileSafetyAuthority::new().authorize_game_commit(
                    context,
                    crate::addons::mutation_features::RENODX_SWITCH_RESHADE_CHANNEL,
                    guards.game(),
                    safety.game(),
                    || {
                        context.storage().upsert_installed_addon(&updated)?;
                        Ok(tracking::install_state_from_record(&updated))
                    },
                );
            }
            crate::FileSafetyAuthority::new().authorize_game_shared_commit(
                context,
                crate::addons::mutation_features::RENODX_SWITCH_RESHADE_CHANNEL,
                &guards,
                &safety,
                || {
                    let crate::addons::renodx::use_cases::commands::update_reshade::PreparedSharedVulkanUpdate {
                        layer_dir,
                        plan,
                        shared_record,
                        changed: _,
                    } = shared;
                    let composed = crate::addons::shared_vulkan_mutation::compose(None, Some(plan))?;
                    // Channel switching changes the shared layer and the
                    // game-owned catalog projection only.  There is no game
                    // file participant, so do not mint a synthetic game
                    // capability for an unrelated root.
                    let roots =
                        crate::addons::shared_vulkan_mutation::TrustedRoots::game_shared_without_game_files(
                            &layer_dir,
                        )?;
                    let registry = crate::addons::renodx::platform::vulkan::native_registry()
                        .ok_or_else(errors::vulkan_unsupported_platform)?;
                    let mutation_id = ulid::Ulid::generate().to_string();
                    let identity = crate::addons::shared_vulkan_mutation::MutationIdentity::new(
                        &mutation_id,
                        crate::addons::shared_vulkan_mutation::ScopeSpec::game_upsert(
                            game_id, &updated,
                        ),
                        crate::addons::mutation_features::RENODX_SWITCH_RESHADE_CHANNEL,
                    );
                    let physical =
                        crate::addons::shared_vulkan_mutation::PhysicalParticipants::new(
                            roots,
                            composed,
                            Some(registry),
                        );
                    let projection =
                        crate::addons::shared_vulkan_mutation::CatalogProjection::new(
                            renderpilot_storage_sqlite::SharedArtifactMutation::Upsert(
                                &shared_record,
                            ),
                        );
                    crate::addons::shared_vulkan_mutation::execute(
                        crate::addons::shared_vulkan_mutation::Request::new(
                            context, identity, physical, projection,
                        ),
                    )?;
                    Ok(tracking::install_state_from_record(&updated))
                },
            )
        }
        ChannelSwitchPhase1::Proxy(snapshot) => {
            drop(guard);
            // Phase 2: download only — no game-folder mutation.
            let download =
                fetch_reshade_from_source(&snapshot.target.source, snapshot.target.arch, progress)
                    .await?;

            // Phase 3: re-lock, revalidate, durable apply.
            let guard =
                crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id)
                    .await?;
            let current = match resolve_channel_switch_phase1(
                context,
                manifest,
                reshade_sources,
                game_id,
                target_channel,
            )? {
                ChannelSwitchPhase1::Proxy(current) => current,
                _ => return Err(errors::state_changed_retry_update()),
            };
            ensure_proxy_channel_switch_matches(&snapshot, &current)?;

            let targets = crate::addons::renodx::mutation_targets::channel_switch_targets(
                &current.target.target_path,
                &current.target.game_dir,
            );

            emit_tool_finalizing(progress, AddonKind::RenoDx);
            crate::FileSafetyAuthority::new().authorize_game_commit(
                context,
                crate::addons::mutation_features::RENODX_SWITCH_RESHADE_CHANNEL,
                &guard,
                safety.game(),
                || {
                    crate::addons::durable::run_targets_mutation(
                        crate::addons::durable::TargetsMutation {
                            context,
                            guard: &guard,
                            targets,
                            feature:
                                crate::addons::mutation_features::RENODX_SWITCH_RESHADE_CHANNEL,
                            game_id,
                        },
                        |mutation_id| -> Result<RenoDxInstallState, ServiceError> {
                            let originals = apply_replacements(vec![Replacement {
                                path: current.target.target_path.clone(),
                                bytes: download.bytes,
                                mtime: None,
                            }])?;

                            let new_source = host_binary_source(
                                current.target.source.url.clone(),
                                download.etag,
                                download.digest,
                                download.last_modified,
                                Some(target_channel),
                            );
                            // The record may not have tracked this exact path before (a legacy
                            // record adopted without host provenance, or the active slot changed)
                            // — carry it through as a receipt so the rebuild below adds it to
                            // `created_files`.
                            let receipt = InstallReceipt {
                                created_files: vec![current.target.target_path.clone()],
                                backed_up_files: Vec::new(),
                            };
                            let updated = match rebuild_proxy_switch_record(
                                &current.record,
                                new_source,
                                Some(&receipt),
                                target_channel,
                            ) {
                                Ok(updated) => updated,
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
                                    addon:
                                        renderpilot_storage_sqlite::InstalledAddonMutation::Upsert(
                                            &updated,
                                        ),
                                    mutation_id: Some(mutation_id),
                                },
                            ) {
                                let restore_result = restore_originals(&originals);
                                return Err(persistence_failure_error(
                                    error.into(),
                                    std::slice::from_ref(&restore_result),
                                ));
                            }
                            Ok(tracking::install_state_from_record(&updated))
                        },
                        |_| {},
                        || {},
                    )
                },
            )
        }
    }
}
