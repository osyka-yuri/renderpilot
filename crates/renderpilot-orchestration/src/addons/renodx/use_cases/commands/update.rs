//! Applies updates to installed RenoDX add-ons and host artifacts.

use std::path::PathBuf;

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, TrackedSource, TrackedSourceRole,
};

use crate::addons::engine::{self, FileOp, InstallPlan, InstallReceipt};
use crate::addons::renodx::progress::emit_finalizing;
use crate::addons::renodx::types::{RecordedChannelParse, RenoDxManifest, ReshadeChannel};
use crate::addons::renodx::use_cases::reshade_update::{
    HostUpdateTarget, OriginalFile, Replacement, addon_label, apply_replacements,
    host_binary_source, persistence_failure_error, recorded_reshade_channel,
    resolve_host_update_target, restore_originals, restore_originals_best_effort, source_with_role,
};
use crate::addons::renodx::{channel, errors, fetch, install, operation_lock, reshade, tracking};
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Applies an update to tracked RenoDX sources and host artifacts.
pub async fn update(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<(), ServiceError> {
    let _guard = operation_lock::lock(game_id).await;
    let record = context
        .storage()
        .get_installed_addon(game_id)?
        .ok_or_else(errors::not_installed)?;
    let shared_vulkan_channel = if matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    ) {
        Some(
            recorded_reshade_channel(&record)
                .map(|channel| manifest.reshade.effective_install_channel(channel))
                .unwrap_or_else(|| {
                    manifest
                        .reshade
                        .effective_install_channel(ReshadeChannel::Stable)
                }),
        )
    } else {
        None
    };

    let addon = source_with_role(&record, TrackedSourceRole::AddonPayload);
    let host = match channel::single_host_source(&record) {
        Ok(host) => host,
        Err(channel::ChannelReadIssue::DuplicateHostSources) => {
            return Err(errors::duplicate_host_sources());
        }
    };
    let dlss_fix = source_with_role(&record, TrackedSourceRole::DlssFix);
    let host_channel = if shared_vulkan_channel.is_some() {
        None
    } else {
        match channel::installed_channel(&record).map_err(|_| errors::duplicate_host_sources())? {
            Some(channel) => Some(channel),
            None => host.and_then(|source| {
                channel::infer_legacy_channel_from_url(source.url())
                    .map(RecordedChannelParse::Parsed)
            }),
        }
    };
    // `resolve_host_update_target` returns `Ok(None)` for a recognized custom
    // build (e.g. GShade) — never replaced, its versioning is its own
    // maintainer's concern — so that guarantee holds here for free.
    let host_target = match host_channel.and_then(|c| c.into_parsed()) {
        Some(channel) => resolve_host_update_target(context, manifest, game_id, channel)?,
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

    let addon_tracked = addon.is_some_and(|source| !source.url().is_empty());
    if !addon_tracked
        && host.is_none()
        && dlss_fix.is_none()
        && !host_policy_writes
        && shared_vulkan_channel.is_none()
    {
        return Err(errors::invalid(
            "this RenoDX install has no recorded source to update from".to_owned(),
        ));
    }

    // Rebuild the tracked-source list with refreshed digests/validators, preserving
    // the install order (add-on first, then ReShade host, then DLSS-Fix).
    let mut refreshed_sources: Vec<TrackedSource> = Vec::new();
    let mut replacements: Vec<Replacement> = Vec::new();
    let mut host_install: Option<HostInstall> = None;

    if let Some(channel) = shared_vulkan_channel {
        crate::addons::renodx::use_cases::commands::update_reshade::UpdateReShadeCommand {
            context,
            manifest,
            channel,
            progress,
        }
        .execute()
        .await?;
    }

    if let Some(addon) = addon {
        if addon.url().is_empty() {
            refreshed_sources.push(addon.clone());
        } else {
            let prepared = prepare_addon_update(&record, addon, progress).await?;
            refreshed_sources.push(prepared.source);
            replacements.extend(prepared.replacement);
        }
    }

    if let (Some(host), Some(target)) = (host, host_target.as_ref()) {
        let prepared = prepare_policy_host_update(&record, target, host, progress).await?;
        refreshed_sources.push(prepared.source);
        if let Some(replacement) = prepared.replacement {
            match replacement {
                HostReplacement::InPlace(replacement) => replacements.push(replacement),
                HostReplacement::Install(install) => host_install = Some(install),
            }
        }
    } else if let Some(host) = host {
        refreshed_sources.push(host.clone());
    }

    if let Some(dlss_fix) = dlss_fix {
        let prepared = prepare_dlss_fix_update(&record, dlss_fix, progress).await?;
        refreshed_sources.push(prepared.source);
        replacements.extend(prepared.replacement);
    }

    emit_finalizing(progress);
    let mut originals = apply_replacements(&replacements)?;
    let host_receipt = match host_install.as_ref() {
        Some(install) => match apply_host_install(install, &mut originals) {
            Ok(receipt) => Some(receipt),
            Err(error) => {
                restore_originals_best_effort(&originals);
                return Err(error);
            }
        },
        None => None,
    };
    // `originals` now covers every file this update touched — replacements and,
    // if any, the host install — so one rollback call restores all of them
    // uniformly, whichever step fails from here.
    let refreshed = match tracking::rebuild_with_sources_and_receipt(
        &record,
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
    if let Err(error) = context.storage().upsert_installed_addon(&refreshed) {
        let restore_result = restore_originals(&originals);
        return Err(persistence_failure_error(
            error.into(),
            std::slice::from_ref(&restore_result),
        ));
    }
    Ok(())
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
    let download = fetch::fetch_reshade_from_source(&target.source, target.arch, progress).await?;
    let source = host_binary_source(
        target.source.url.clone(),
        download.etag.clone(),
        download.digest.clone(),
        download.last_modified.clone(),
        Some(target.channel),
    );

    let changed = download.digest != existing_source.digest() || target.action.writes_host();
    let replacement = if changed {
        match tracking::required_rollback_host_path(record) {
            Ok(path) if reshade::same_path(&path, &target.target_path) => {
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

async fn prepare_dlss_fix_update(
    record: &InstalledAddon,
    source: &TrackedSource,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedSourceUpdate, ServiceError> {
    let download = fetch::fetch_addon(source.url(), "DLSS-Fix", progress).await?;
    let refreshed = refreshed_source(source, &download);
    let replacement = if download.digest == source.digest() {
        None
    } else {
        Some(Replacement {
            path: dlss_fix_path(record)?,
            bytes: download.bytes,
            mtime: download.last_modified,
        })
    };
    Ok(PreparedSourceUpdate {
        source: refreshed,
        replacement,
    })
}

fn refreshed_source(source: &TrackedSource, download: &fetch::Download) -> TrackedSource {
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

fn dlss_fix_path(record: &InstalledAddon) -> Result<PathBuf, ServiceError> {
    install::dlss_fix_file_path(record)
        .ok_or_else(|| errors::invalid("no DLSS-Fix add-on in this install".to_owned()))
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
    install: &HostInstall,
    originals: &mut Vec<OriginalFile>,
) -> Result<InstallReceipt, ServiceError> {
    let path = install.game_dir.join(&install.name);
    let original_bytes = if path.is_file() {
        Some(crate::fs::read_file(&path)?)
    } else {
        None
    };
    let receipt = engine::install(
        &install.game_dir,
        &InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::Replace {
                name: install.name.clone(),
                bytes: install.bytes.clone(),
            }],
        },
    )?;
    originals.push(OriginalFile {
        path,
        bytes: original_bytes,
    });
    Ok(receipt)
}
