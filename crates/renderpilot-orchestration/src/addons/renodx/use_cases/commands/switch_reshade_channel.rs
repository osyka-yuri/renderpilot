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
    Healed(InstalledAddon),
    SharedVulkan { record: InstalledAddon },
    Proxy(ProxyChannelSwitchSnapshot),
}

struct ProxyChannelSwitchSnapshot {
    record: InstalledAddon,
    target: HostUpdateTarget,
    target_channel: ReshadeChannel,
}

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
    let guard = game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
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
            let guards =
                game_mutation_lock::enter_game_shared_mutation_boundary_async(context, game_id)
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
            crate::FileSafetyAuthority::new().authorize_game_shared_commit(
                context,
                crate::addons::mutation_features::RENODX_SWITCH_RESHADE_CHANNEL,
                &guards,
                &safety,
                || {
                    prepared.commit(context)?;
                    let updated = current.with_reshade_channel(target_channel.as_str());
                    context.storage().upsert_installed_addon(&updated)?;
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
        // Resolve the metadata repair without writing it. The caller persists
        // it only after the final guard-bound safety validation.
        let healed = if let Some(host_source) = host_source {
            let healed_source = channel::with_host_channel(host_source, target_channel);
            tracking::replace_host_source(&record, &healed_source)?
        } else {
            record.with_reshade_channel(target_channel.as_str())
        }
        .with_reshade_channel(target_channel.as_str());
        return Ok(ChannelSwitchPhase1::Healed(healed));
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
    use std::fs;

    use renderpilot_application::{GameRepository, InstalledAddonRepository};
    use renderpilot_domain::{
        AddonKind, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };
    use tempfile::{TempDir, tempdir};

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

    struct SameChannelFixture {
        _db_root: TempDir,
        game_root: TempDir,
        context: Context,
        game_id: GameId,
        before_record: InstalledAddon,
    }

    fn same_channel_fixture(suffix: &str) -> SameChannelFixture {
        let db_root = tempdir().expect("db root");
        let game_root = tempdir().expect("game root");
        let context = Context::open_at(db_root.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new(format!("manual:channel-safety-{suffix}")).expect("game id");
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Channel Safety Test", Launcher::Manual)
                .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_root.path().to_string_lossy()).expect("game path"),
        );
        context.storage().upsert_game(&game).expect("game");

        let addon_path = game_root.path().join("renodx-game.addon64");
        fs::write(&addon_path, b"addon").expect("add-on");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(addon_path.to_string_lossy()).expect("add-on path"),
        )
        .with_tracked_source(source(
            TrackedSourceRole::HostBinary,
            "https://reshade.me/downloads/ReShade_Setup.exe",
            "host-digest",
        ))
        .with_reshade_channel("stable");
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("record");
        let before_record = records::record_of_kind(&context, &game_id, AddonKind::RenoDx)
            .expect("record query")
            .expect("record remains");

        SameChannelFixture {
            _db_root: db_root,
            game_root,
            context,
            game_id,
            before_record,
        }
    }

    async fn switch_same_channel(
        fixture: &SameChannelFixture,
        safety: crate::GameMutationSafetyPermits,
    ) -> Result<RenoDxInstallState, ServiceError> {
        let manifest = crate::addons::renodx::test_support::manifest(Vec::new());
        let reshade_sources = crate::addons::renodx::test_support::reshade_sources();
        switch_reshade_channel(SwitchChannelRequest {
            context: &fixture.context,
            manifest: &manifest,
            reshade_sources: &reshade_sources,
            game_id: &fixture.game_id,
            target_channel: ReshadeChannel::Stable,
            safety,
            progress: None,
        })
        .await
    }

    fn assert_record_unchanged(fixture: &SameChannelFixture) {
        assert_eq!(
            records::record_of_kind(&fixture.context, &fixture.game_id, AddonKind::RenoDx)
                .expect("record query")
                .expect("record remains"),
            fixture.before_record,
            "safety rejection must precede the metadata heal"
        );
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

    #[tokio::test]
    async fn same_channel_heal_rejects_stale_safety_before_persisting_metadata() {
        let fixture = same_channel_fixture("stale");
        let authority = crate::FileSafetyAuthority::new();
        let assessment = authority
            .issue_game_assessment(&fixture.context, &fixture.game_id)
            .expect("assessment");
        let safety = authority
            .game_mutation_permits(
                fixture.game_id.clone(),
                Some(&assessment.context_token),
                None,
            )
            .expect("permits");
        fs::create_dir(fixture.game_root.path().join("EasyAntiCheat")).expect("anti-cheat marker");

        let error = switch_same_channel(&fixture, safety)
            .await
            .expect_err("stale context must reject the metadata heal");

        assert!(matches!(error, ServiceError::SafetyContextStale { .. }));
        assert_record_unchanged(&fixture);
    }

    #[tokio::test]
    async fn same_channel_heal_rejects_another_game_scope_before_persisting_metadata() {
        let fixture = same_channel_fixture("scope");
        let other_root = tempdir().expect("other game root");
        let other_game_id = GameId::new("manual:channel-safety-other").expect("other game id");
        let other_game = GameInstallation::new(
            GameIdentity::new(other_game_id.clone(), "Other Game", Launcher::Manual)
                .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(other_root.path().to_string_lossy()).expect("game path"),
        );
        fixture
            .context
            .storage()
            .upsert_game(&other_game)
            .expect("other game");
        let authority = crate::FileSafetyAuthority::new();
        let assessment = authority
            .issue_game_assessment(&fixture.context, &other_game_id)
            .expect("assessment");
        let safety = authority
            .game_mutation_permits(
                fixture.game_id.clone(),
                Some(&assessment.context_token),
                None,
            )
            .expect("well-formed permits");

        let error = switch_same_channel(&fixture, safety)
            .await
            .expect_err("another game scope must reject the metadata heal");

        assert!(matches!(
            error,
            ServiceError::SafetyContextScopeMismatch { .. }
        ));
        assert_record_unchanged(&fixture);
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
