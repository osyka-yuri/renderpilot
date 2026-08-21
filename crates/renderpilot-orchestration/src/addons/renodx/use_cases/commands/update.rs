//! Applies updates to installed RenoDX add-ons and host artifacts.

use std::path::PathBuf;

use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, TrackedSource, TrackedSourceRole,
};

use crate::addons::engine::{self, FileOp, InstallPlan, InstallReceipt};
use crate::addons::file_update::{
    OriginalFile, Replacement, apply_replacements, persistence_failure_error, restore_originals,
    restore_originals_best_effort,
};
use crate::addons::progress::{emit_tool_finalizing, sequential_stage_observer};
use crate::addons::records::{self, addon_label, source_with_role};
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::renodx::use_cases::reshade_update::{
    HostUpdateTarget, recorded_reshade_channel, resolve_host_update_target,
};
use crate::addons::renodx::{errors, fetch, tracking};
use crate::addons::reshade::channel;
use crate::addons::reshade::fetch::{Download, fetch_reshade_from_source};
use crate::addons::reshade::types::{RecordedChannelParse, ReshadeSourceCatalog};
use crate::addons::reshade::update::host_binary_source;
use crate::game_mutation_lock;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Complete request for a generic RenoDX update.
pub struct UpdateRequest<'a> {
    /// Application services and storage.
    pub context: &'a Context,
    /// RenoDX manifest used to resolve the update.
    pub manifest: &'a RenoDxManifest,
    /// ReShade sources used when the host must be updated.
    pub reshade_sources: &'a ReshadeSourceCatalog,
    /// Game whose RenoDX installation is being updated.
    pub game_id: &'a GameId,
    /// Fresh permits for every mutation scope the resolved update may require.
    pub safety: crate::GameMutationSafetyPermits,
    /// Optional download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

fn authorize_update_commit<T>(
    context: &Context,
    guards: crate::game_mutation_lock::GameMutationBoundary,
    safety: &crate::GameMutationSafetyPermits,
    shared_update: Option<
        crate::addons::renodx::use_cases::commands::update_reshade::PreparedReShadeUpdate,
    >,
    game_commit: impl FnOnce(&crate::game_mutation_lock::GameMutationGuard) -> Result<T, ServiceError>,
) -> Result<T, ServiceError> {
    let feature = crate::addons::mutation_features::RENODX_UPDATE;
    let authority = crate::FileSafetyAuthority::new();
    match guards {
        crate::game_mutation_lock::GameMutationBoundary::Game(guard) => {
            if shared_update.is_some() {
                return Err(ServiceError::command_failed(
                    "a prepared shared Vulkan update requires the combined mutation boundary",
                ));
            }
            authority.authorize_game_commit(context, feature, &guard, safety.game(), || {
                game_commit(&guard)
            })
        }
        crate::game_mutation_lock::GameMutationBoundary::GameShared(guards) => {
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

/// Applies an update to the main RenoDX add-on and host artifacts only.
/// DLSS-Fix has an independent update/repair command and its source/path
/// projection is copied byte-for-byte through this generic transaction.
///
/// Network prepare for per-game artifacts runs **outside** the per-game lock
/// (same 3-phase contract as Luma update). Shared Vulkan layer updates still
/// apply under the lock in phase 3 (system-wide mutation).
pub async fn update(request: UpdateRequest<'_>) -> Result<(), ServiceError> {
    let UpdateRequest {
        context,
        manifest,
        reshade_sources,
        game_id,
        safety,
        progress,
    } = request;
    // Phase 1: snapshot under the per-game lock.
    let snapshot = {
        let _guard =
            game_mutation_lock::enter_game_mutation_boundary_async(context, game_id).await?;
        resolve_update_snapshot(context, manifest, reshade_sources, game_id)?
    };

    // Phase 2: downloads only for per-game sources (no disk apply).
    let prepared = prepare_update_artifacts(&snapshot, progress).await?;
    let shared_update = match snapshot.shared_vulkan_channel {
        Some(channel) => Some(
            crate::addons::renodx::use_cases::commands::update_reshade::PreparedReShadeUpdate::prepare(
                reshade_sources,
                channel,
                progress,
            )
            .await?,
        ),
        None => None,
    };

    // Phase 3: re-lock, revalidate, shared Vulkan (if any), apply.
    // Peer exclusivity is not re-checked here: one installed-addon row per game
    // plus our own record already blocks foreign tools for the duration of prepare.
    let guards = game_mutation_lock::enter_mutation_boundary_async(
        context,
        game_id,
        shared_update.is_some(),
    )
    .await?;
    let revalidated = resolve_update_snapshot(context, manifest, reshade_sources, game_id)?;
    ensure_update_snapshot_matches(&snapshot, &revalidated)?;
    let current = &revalidated.record;

    emit_tool_finalizing(progress, AddonKind::RenoDx);
    let replacement_paths: Vec<PathBuf> = prepared
        .replacements
        .iter()
        .map(|r| r.path.clone())
        .collect();
    let host_install_path = prepared
        .host_install
        .as_ref()
        .map(|install| install.game_dir.join(&install.name));
    let targets = crate::addons::renodx::mutation_targets::update_targets(
        &revalidated.record,
        &replacement_paths,
        host_install_path.as_deref(),
    )?;
    let PreparedUpdateArtifacts {
        refreshed_sources,
        replacements,
        host_install,
    } = prepared;

    authorize_update_commit(context, guards, &safety, shared_update, |guard| {
        crate::addons::durable::run_targets_mutation(
            crate::addons::durable::TargetsMutation {
                context,
                guard,
                targets,
                feature: crate::addons::mutation_features::RENODX_UPDATE,
                game_id,
            },
            |mutation_id| -> Result<(), ServiceError> {
                let mut originals = apply_replacements(replacements)?;
                let host_receipt = match host_install {
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
                    refreshed_sources,
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
                        addon: renderpilot_storage_sqlite::InstalledAddonMutation::Upsert(
                            &refreshed,
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
                Ok(())
            },
            |_| {},
            || {},
        )
    })
}

struct UpdateSnapshot {
    record: InstalledAddon,
    shared_vulkan_channel: Option<crate::addons::reshade::types::ReshadeChannel>,
    /// Cloned tracked sources needed for unlocked prepare (owned).
    addon: Option<TrackedSource>,
    host: Option<TrackedSource>,
    host_target: Option<HostUpdateTarget>,
}

struct PreparedUpdateArtifacts {
    refreshed_sources: Vec<TrackedSource>,
    replacements: Vec<Replacement>,
    host_install: Option<HostInstall>,
}

fn resolve_update_snapshot(
    context: &Context,
    manifest: &RenoDxManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
) -> Result<UpdateSnapshot, ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;
    if crate::addons::renodx::dlss_fix_binding::resolve(&record).main_payload_collides() {
        return Err(errors::invalid(
            "RenoDX main payload collides with the reserved DLSS-Fix companion target".to_owned(),
        ));
    }
    let shared_vulkan_host = matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    );
    let shared_vulkan_channel = if shared_vulkan_host {
        match recorded_reshade_channel(&record) {
            Some(channel) if reshade_sources.supports_channel(channel) => Some(channel),
            Some(_) => None,
            None => Some(reshade_sources.default_install_channel()),
        }
    } else {
        None
    };

    let addon = source_with_role(&record, TrackedSourceRole::AddonPayload).cloned();
    let host = match channel::single_host_source(&record) {
        Ok(host) => host.cloned(),
        Err(channel::ChannelReadIssue::DuplicateHostSources) => {
            return Err(errors::duplicate_host_sources());
        }
    };
    let host_channel = if shared_vulkan_host {
        None
    } else {
        match channel::installed_channel(&record).map_err(|_| errors::duplicate_host_sources())? {
            Some(channel) => Some(channel),
            None => host.as_ref().and_then(|source| {
                channel::infer_legacy_channel_from_url(source.url())
                    .map(RecordedChannelParse::Parsed)
            }),
        }
    };
    let host_target = match host_channel.and_then(|c| c.into_parsed()) {
        Some(channel) => {
            resolve_host_update_target(context, manifest, reshade_sources, game_id, channel)?
        }
        None => None,
    };
    if let Some(target) = host_target.as_ref()
        && target.conflict
    {
        return Err(errors::invalid(
            "ReShade host conflict must be resolved before updating RenoDX".to_owned(),
        ));
    }
    let host_policy_writes = host.is_some()
        && host_target
            .as_ref()
            .is_some_and(|target| target.action.writes_host());

    let addon_tracked = addon
        .as_ref()
        .is_some_and(|source| !source.url().is_empty());
    if !addon_tracked && host.is_none() && !host_policy_writes && shared_vulkan_channel.is_none() {
        return Err(errors::invalid(
            "this RenoDX install has no recorded source to update from".to_owned(),
        ));
    }

    Ok(UpdateSnapshot {
        record,
        shared_vulkan_channel,
        addon,
        host,
        host_target,
    })
}

fn ensure_update_snapshot_matches(
    snapshot: &UpdateSnapshot,
    current: &UpdateSnapshot,
) -> Result<(), ServiceError> {
    if snapshot.record != current.record
        || snapshot.shared_vulkan_channel != current.shared_vulkan_channel
        || snapshot.host_target != current.host_target
    {
        return Err(errors::state_changed_retry_update());
    }
    Ok(())
}

async fn prepare_update_artifacts(
    snapshot: &UpdateSnapshot,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedUpdateArtifacts, ServiceError> {
    let record = &snapshot.record;
    let addon_tracked = snapshot
        .addon
        .as_ref()
        .is_some_and(|source| !source.url().is_empty());
    // The generic update has no authority to fetch, write, or refresh DLSS-Fix.
    // Preserve its exact phase-3 projection, including advisory/partial evidence.
    let mut refreshed_sources: Vec<TrackedSource> = record
        .tracked_sources()
        .iter()
        .filter(|source| source.role() == TrackedSourceRole::DlssFix)
        .cloned()
        .collect();
    let mut replacements: Vec<Replacement> = Vec::new();
    let mut host_install: Option<HostInstall> = None;
    // Shared Vulkan is applied in phase 3; do not count it in unlocked stages.
    let stage_count = u64::from(addon_tracked)
        + u64::from(snapshot.host.is_some() && snapshot.host_target.is_some());
    let mut stage_index = 0;

    if let Some(addon) = snapshot.addon.as_ref() {
        if addon.url().is_empty() {
            refreshed_sources.push(addon.clone());
        } else {
            let stage_progress_fn = sequential_stage_observer(progress, stage_index, stage_count);
            let stage_progress = stage_progress_fn
                .as_ref()
                .map(|observer| observer as &ProgressObserver<'_>);
            let prepared = prepare_addon_update(record, addon, stage_progress).await?;
            refreshed_sources.push(prepared.source);
            replacements.extend(prepared.replacement);
            stage_index += 1;
        }
    }

    if let (Some(host), Some(target)) = (snapshot.host.as_ref(), snapshot.host_target.as_ref()) {
        let stage_progress_fn = sequential_stage_observer(progress, stage_index, stage_count);
        let stage_progress = stage_progress_fn
            .as_ref()
            .map(|observer| observer as &ProgressObserver<'_>);
        let prepared = prepare_policy_host_update(record, target, host, stage_progress).await?;
        refreshed_sources.push(prepared.source);
        if let Some(replacement) = prepared.replacement {
            match replacement {
                HostReplacement::InPlace(replacement) => replacements.push(replacement),
                HostReplacement::Install(install) => host_install = Some(install),
            }
        }
        stage_index += 1;
    } else if let Some(host) = snapshot.host.as_ref() {
        refreshed_sources.push(host.clone());
    }

    debug_assert_eq!(stage_index, stage_count);

    Ok(PreparedUpdateArtifacts {
        refreshed_sources,
        replacements,
        host_install,
    })
}

struct PreparedSourceUpdate {
    source: TrackedSource,
    replacement: Option<Replacement>,
}

struct HostInstall {
    game_dir: PathBuf,
    name: String,
    bytes: Vec<u8>,
}

enum HostReplacement {
    InPlace(Replacement),
    Install(HostInstall),
}

struct PreparedHostPolicyUpdate {
    source: TrackedSource,
    replacement: Option<HostReplacement>,
}

async fn prepare_addon_update(
    record: &InstalledAddon,
    source: &TrackedSource,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedSourceUpdate, ServiceError> {
    let download = fetch::fetch_addon(source.url(), addon_label(record), progress).await?;
    let changed = download.digest != source.digest();
    let refreshed = refreshed_source(source, &download);
    Ok(PreparedSourceUpdate {
        source: refreshed,
        replacement: changed.then(|| Replacement {
            path: addon_path(record),
            bytes: download.bytes,
            mtime: download.last_modified.clone(),
        }),
    })
}

async fn prepare_policy_host_update(
    record: &InstalledAddon,
    target: &HostUpdateTarget,
    existing_source: &TrackedSource,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedHostPolicyUpdate, ServiceError> {
    let download = fetch_reshade_from_source(&target.source, target.arch, progress).await?;
    let changed = download.digest != existing_source.digest() || target.action.writes_host();
    let source = host_binary_source(
        target.source.url.clone(),
        download.etag,
        download.digest,
        download.last_modified,
        Some(target.channel),
    );

    let replacement = if changed {
        match tracking::required_rollback_host_path(record) {
            Ok(path) if crate::paths::same_path(&path, &target.target_path) => {
                Some(HostReplacement::InPlace(Replacement {
                    path,
                    bytes: download.bytes,
                    mtime: None,
                }))
            }
            Ok(_) | Err(_) => Some(HostReplacement::Install(HostInstall {
                game_dir: target.game_dir.clone(),
                name: target.slot.clone(),
                bytes: download.bytes,
            })),
        }
    } else {
        None
    };

    Ok(PreparedHostPolicyUpdate {
        source,
        replacement,
    })
}

fn refreshed_source(source: &TrackedSource, download: &Download) -> TrackedSource {
    TrackedSource::new(
        source.role(),
        source.url().to_owned(),
        download.etag.clone(),
        download.digest.clone(),
    )
    .with_last_modified(download.last_modified.clone())
}

fn addon_path(record: &InstalledAddon) -> PathBuf {
    PathBuf::from(record.addon_file().as_str())
}

/// Installs `install`'s bytes at its destination via a no-backup `Replace` op
/// (so the returned receipt still updates the record's `created_files` the way
/// it always has), then appends the destination's pre-write state to
/// `originals` — so a later failure, before this update's result is durably
/// persisted, can restore it via [`restore_originals`]/
/// [`restore_originals_best_effort`] alongside every other file this update
/// touched, in one uniform pass. A failure from the write itself needs no entry
/// here: the engine's own single-op rollback already leaves the destination
/// exactly as it was.
fn apply_host_install(
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
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::Replace { name, bytes }],
        },
    )?;
    originals.push(OriginalFile {
        path,
        bytes: original_bytes,
    });
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };
    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::addons::reshade::types::ReshadeChannel;

    fn record() -> InstalledAddon {
        InstalledAddon::new(
            GameId::new("steam:1091500").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new("C:/Games/Test/renodx-test.addon64").expect("add-on path"),
        )
        .with_addon_version("1")
    }

    fn snapshot(record: InstalledAddon, channel: Option<ReshadeChannel>) -> UpdateSnapshot {
        UpdateSnapshot {
            record,
            shared_vulkan_channel: channel,
            addon: None,
            host: None,
            host_target: None,
        }
    }

    struct SafetyFixture {
        _db_dir: TempDir,
        game_dir: TempDir,
        context: Context,
        game_id: GameId,
    }

    fn safety_fixture(suffix: &str) -> SafetyFixture {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new(format!("manual:update-safety-{suffix}")).expect("game id");
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Update Safety Test", Launcher::Manual)
                .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_dir.path().to_string_lossy()).expect("game path"),
        );
        context.storage().upsert_game(&game).expect("game");

        SafetyFixture {
            _db_dir: db_dir,
            game_dir,
            context,
            game_id,
        }
    }

    async fn assert_update_barrier_rejects(
        fixture: &SafetyFixture,
        safety: crate::GameMutationSafetyPermits,
        expected: fn(&ServiceError) -> bool,
    ) {
        let guards = game_mutation_lock::enter_mutation_boundary_async(
            &fixture.context,
            &fixture.game_id,
            false,
        )
        .await
        .expect("game boundary");
        let mut commit_called = false;
        let error = authorize_update_commit(&fixture.context, guards, &safety, None, |_| {
            commit_called = true;
            Ok(())
        })
        .expect_err("invalid safety must reject the update commit");

        assert!(expected(&error), "unexpected error: {error:?}");
        assert!(!commit_called, "safety rejection must precede first write");
        assert!(
            fixture
                .context
                .storage()
                .pending_file_mutations_for_game(&fixture.game_id)
                .expect("pending mutations")
                .is_empty()
        );
    }

    #[test]
    fn update_snapshot_rejects_any_install_record_drift() {
        let prepared = snapshot(record(), None);
        let current = snapshot(record().with_addon_version("2"), None);

        assert!(ensure_update_snapshot_matches(&prepared, &current).is_err());
    }

    #[test]
    fn update_snapshot_rejects_shared_vulkan_channel_drift() {
        let prepared = snapshot(record(), Some(ReshadeChannel::Stable));
        let current = snapshot(record(), Some(ReshadeChannel::Nightly));

        assert!(ensure_update_snapshot_matches(&prepared, &current).is_err());
    }

    #[tokio::test]
    async fn update_commit_barrier_rejects_stale_game_context_before_first_write() {
        let fixture = safety_fixture("stale");
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
        fs::create_dir(fixture.game_dir.path().join("EasyAntiCheat")).expect("anti-cheat marker");

        assert_update_barrier_rejects(&fixture, safety, |error| {
            matches!(error, ServiceError::SafetyContextStale { .. })
        })
        .await;
    }

    #[tokio::test]
    async fn update_commit_barrier_rejects_another_game_scope_before_first_write() {
        let fixture = safety_fixture("scope");
        let other = safety_fixture("other");
        fixture
            .context
            .storage()
            .upsert_game(
                &other
                    .context
                    .storage()
                    .require_game(&other.game_id)
                    .expect("other game"),
            )
            .expect("copy other game");
        let authority = crate::FileSafetyAuthority::new();
        let assessment = authority
            .issue_game_assessment(&fixture.context, &other.game_id)
            .expect("assessment");
        let safety = authority
            .game_mutation_permits(
                fixture.game_id.clone(),
                Some(&assessment.context_token),
                None,
            )
            .expect("well-formed permits");

        assert_update_barrier_rejects(&fixture, safety, |error| {
            matches!(error, ServiceError::SafetyContextScopeMismatch { .. })
        })
        .await;
    }

    #[tokio::test]
    async fn generic_prepare_preserves_the_dlss_projection_without_network_or_writes() {
        let dlss = TrackedSource::new(
            TrackedSourceRole::DlssFix,
            "https://example.test/renodx-dlssfix.addon64",
            Some("etag".to_owned()),
            "live-digest",
        )
        .with_last_modified(Some("Mon, 01 Jan 2024 00:00:00 GMT".to_owned()))
        .with_advisory();
        let prepared = prepare_update_artifacts(
            &snapshot(record().with_tracked_source(dlss.clone()), None),
            None,
        )
        .await
        .expect("generic prepare does not fetch DLSS");

        assert_eq!(prepared.refreshed_sources, vec![dlss]);
        assert!(prepared.replacements.is_empty());
        assert!(prepared.host_install.is_none());
    }
}
