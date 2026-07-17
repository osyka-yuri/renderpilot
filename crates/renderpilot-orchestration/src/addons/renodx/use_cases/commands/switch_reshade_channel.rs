//! Switches the recorded ReShade channel for RenoDX installs.
//!
//! Proxy host switches use the same 3-phase contract as RenoDX update: snapshot
//! under the game lock, network fetch unlocked, revalidate + durable apply under
//! lock. Same-channel metadata heal stays lock-only (no network). Shared Vulkan
//! channel switches delegate to the shared-layer command outside the per-game
//! durable file transaction.

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, RenoDxInstallState, TrackedSource,
    TrackedSourceRole,
};

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
use crate::addons::renodx::use_cases::reshade_update::{
    HostUpdateTarget, recorded_reshade_channel, resolve_host_update_target,
};
use crate::addons::reshade::channel;
use crate::addons::reshade::fetch::fetch_reshade_from_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::addons::reshade::update::host_binary_source;
use crate::game_mutation_lock;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

enum ChannelSwitchPhase1 {
    Healed(RenoDxInstallState),
    SharedVulkan { record: InstalledAddon },
    Proxy(ProxyChannelSwitchSnapshot),
}

struct ProxyChannelSwitchSnapshot {
    record: InstalledAddon,
    target: HostUpdateTarget,
    target_channel: ReshadeChannel,
}

/// Switches the recorded ReShade host binary artifact between stable and nightly.
pub async fn switch_reshade_channel(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
    target_channel: ReshadeChannel,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    ensure_target_channel(reshade_sources, target_channel)?;

    // Phase 1: snapshot / heal under the per-game lock.
    let phase1 = {
        let _guard =
            game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
        resolve_channel_switch_phase1(context, manifest, reshade_sources, game_id, target_channel)?
    };

    match phase1 {
        ChannelSwitchPhase1::Healed(state) => Ok(state),
        ChannelSwitchPhase1::SharedVulkan { record } => {
            // Shared Vulkan layer update is system-wide; no per-game durable FS tx.
            switch_vulkan_reshade_channel(
                context,
                reshade_sources,
                record,
                target_channel,
                progress,
            )
            .await
        }
        ChannelSwitchPhase1::Proxy(snapshot) => {
            // Phase 2: download only — no game-folder mutation.
            let download =
                fetch_reshade_from_source(&snapshot.target.source, snapshot.target.arch, progress)
                    .await?;

            // Phase 3: re-lock, revalidate, durable apply.
            let guard =
                game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
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
            crate::addons::durable::run_targets_mutation(
                crate::addons::durable::TargetsMutation {
                    context,
                    guard: &guard,
                    targets,
                    feature: crate::addons::mutation_features::RENODX_SWITCH_RESHADE_CHANNEL,
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
                    // The record may not have tracked this exact path before (a legacy record
                    // adopted without host provenance, or the active slot changed) — carry it
                    // through as a receipt so the rebuild below adds it to `created_files`.
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
                            baseline_inserts: &[],
                            baseline_deletes: &[],
                            addon: renderpilot_storage_sqlite::InstalledAddonMutation::Upsert(
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
        }
    }
}

fn resolve_channel_switch_phase1(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
    target_channel: ReshadeChannel,
) -> Result<ChannelSwitchPhase1, ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;
    if matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    ) {
        return Ok(ChannelSwitchPhase1::SharedVulkan { record });
    }
    let host_source =
        channel::single_host_source(&record).map_err(|_| errors::duplicate_host_sources())?;

    let current = recorded_reshade_channel(&record);

    if current == Some(target_channel) {
        // Same-channel metadata heal is intentional non-durable (no FS mutation).
        let healed = if let Some(host_source) = host_source {
            let healed_source = channel::with_host_channel(host_source, target_channel);
            tracking::replace_host_source(&record, &healed_source)?
        } else {
            record.with_reshade_channel(target_channel.as_str())
        }
        .with_reshade_channel(target_channel.as_str());
        context.storage().upsert_installed_addon(&healed)?;
        return Ok(ChannelSwitchPhase1::Healed(
            tracking::install_state_from_record(&healed),
        ));
    }

    // `resolve_host_update_target` also returns `None` for a recognized custom
    // build (e.g. GShade) — RenoDX doesn't manage its channel either, and the
    // action isn't offered in the UI for one in the first place.
    let target =
        resolve_host_update_target(context, manifest, reshade_sources, game_id, target_channel)?
            .ok_or_else(|| {
                errors::invalid("cannot resolve the ReShade proxy slot for this game".to_owned())
            })?;
    if target.conflict {
        return Err(errors::invalid(
            "ReShade host conflict must be resolved before switching channel".to_owned(),
        ));
    }
    if !target.target_path.is_file() {
        return Err(errors::invalid(
            "ReShade host binary is missing; repair it before switching channel".to_owned(),
        ));
    }

    Ok(ChannelSwitchPhase1::Proxy(ProxyChannelSwitchSnapshot {
        record,
        target,
        target_channel,
    }))
}

fn ensure_proxy_channel_switch_matches(
    snapshot: &ProxyChannelSwitchSnapshot,
    current: &ProxyChannelSwitchSnapshot,
) -> Result<(), ServiceError> {
    if snapshot.record != current.record
        || snapshot.target != current.target
        || snapshot.target_channel != current.target_channel
    {
        return Err(errors::state_changed_retry_update());
    }
    Ok(())
}

fn replace_or_append_host_source(
    record: &InstalledAddon,
    new_source: TrackedSource,
) -> Vec<TrackedSource> {
    let mut sources = record.tracked_sources().to_vec();
    let mut replaced = false;
    for entry in &mut sources {
        if entry.role() == TrackedSourceRole::HostBinary {
            *entry = new_source.clone();
            replaced = true;
        }
    }
    if !replaced {
        sources.push(new_source);
    }
    sources
}

fn rebuild_proxy_switch_record(
    record: &InstalledAddon,
    new_source: TrackedSource,
    receipt: Option<&InstallReceipt>,
    target_channel: ReshadeChannel,
) -> Result<InstalledAddon, ServiceError> {
    tracking::rebuild_with_sources_and_receipt(
        record,
        replace_or_append_host_source(record, new_source),
        receipt,
        "RenoDX channel switch",
    )
    .map(|updated| updated.with_reshade_channel(target_channel.as_str()))
}

async fn switch_vulkan_reshade_channel(
    context: &Context,
    reshade_sources: &ReshadeSourceCatalog,
    record: InstalledAddon,
    target_channel: ReshadeChannel,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    crate::addons::renodx::use_cases::commands::update_reshade::UpdateReShadeCommand {
        context,
        reshade_sources,
        channel: target_channel,
        progress,
    }
    .execute()
    .await?;

    let updated = record.with_reshade_channel(target_channel.as_str());
    context.storage().upsert_installed_addon(&updated)?;
    Ok(tracking::install_state_from_record(&updated))
}

fn ensure_target_channel(
    reshade_sources: &ReshadeSourceCatalog,
    target_channel: ReshadeChannel,
) -> Result<(), ServiceError> {
    if reshade_sources.supports_channel(target_channel) {
        Ok(())
    } else {
        Err(errors::channel_unavailable(target_channel))
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{AddonKind, GameId, PathRef};

    use super::*;

    fn record_with_sources(sources: Vec<TrackedSource>) -> InstalledAddon {
        let addon = PathRef::new(r"C:\Games\Test\renodx-test.addon64").expect("path");
        InstalledAddon::from_parts(
            GameId::new("steam:42").expect("id"),
            AddonKind::RenoDx,
            addon.clone(),
            None,
            vec![addon],
            Vec::new(),
            sources,
        )
        .expect("record")
    }

    fn source(role: TrackedSourceRole, url: &str, digest: &str) -> TrackedSource {
        TrackedSource::new(role, url, None, digest)
    }

    #[test]
    fn host_source_replacement_appends_for_legacy_records_without_host_source() {
        let record = record_with_sources(vec![source(
            TrackedSourceRole::AddonPayload,
            "https://example/renodx.addon64",
            "addon-digest",
        )]);
        let host = source(
            TrackedSourceRole::HostBinary,
            "https://reshade.me/downloads/ReShade_Setup.exe",
            "host-digest",
        );

        let sources = replace_or_append_host_source(&record, host);

        assert_eq!(sources.len(), 2);
        assert_eq!(
            sources
                .iter()
                .filter(|source| source.role() == TrackedSourceRole::HostBinary)
                .count(),
            1
        );
    }

    #[test]
    fn host_source_replacement_replaces_existing_host_source() {
        let record = record_with_sources(vec![
            source(
                TrackedSourceRole::AddonPayload,
                "https://example/renodx.addon64",
                "addon-digest",
            ),
            source(
                TrackedSourceRole::HostBinary,
                "https://old.example/ReShade.exe",
                "old-host-digest",
            ),
        ]);
        let host = source(
            TrackedSourceRole::HostBinary,
            "https://reshade.me/downloads/ReShade_Setup.exe",
            "new-host-digest",
        );

        let sources = replace_or_append_host_source(&record, host);

        assert_eq!(sources.len(), 2);
        let host = sources
            .iter()
            .find(|source| source.role() == TrackedSourceRole::HostBinary)
            .expect("host source");
        assert_eq!(host.digest(), "new-host-digest");
    }

    #[test]
    fn proxy_switch_record_updates_top_level_channel() {
        let record = record_with_sources(vec![source(
            TrackedSourceRole::HostBinary,
            "https://reshade.me/downloads/ReShade_Setup.exe",
            "old-host-digest",
        )])
        .with_reshade_channel("stable");
        let host = source(
            TrackedSourceRole::HostBinary,
            "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip",
            "new-host-digest",
        )
        .with_channel("nightly");

        let updated = rebuild_proxy_switch_record(&record, host, None, ReshadeChannel::Nightly)
            .expect("switch record");

        assert_eq!(updated.reshade_channel(), Some("nightly"));
        assert_eq!(
            recorded_reshade_channel(&updated),
            Some(ReshadeChannel::Nightly)
        );
    }

    #[test]
    fn every_switch_path_rejects_an_explicit_unavailable_stable_channel() {
        let mut reshade_sources = crate::addons::renodx::test_support::reshade_sources();
        reshade_sources.stable = None;

        let error = ensure_target_channel(&reshade_sources, ReshadeChannel::Stable)
            .expect_err("Stable must not silently remap to Nightly");

        assert!(error.to_string().contains("stable"));
    }
}
