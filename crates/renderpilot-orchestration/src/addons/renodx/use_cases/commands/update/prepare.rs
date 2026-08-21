//! Unlocked network preparation for RenoDX updates.

use std::path::PathBuf;

use renderpilot_domain::{InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::ServiceError;
use crate::addons::file_update::Replacement;
use crate::addons::progress::sequential_stage_observer;
use crate::addons::records::addon_label;
use crate::addons::renodx::use_cases::reshade_update::HostUpdateTarget;
use crate::addons::renodx::{fetch, tracking};
use crate::addons::reshade::fetch::{Download, fetch_reshade_from_source};
use crate::addons::reshade::update::host_binary_source;
use crate::net::ProgressObserver;

use super::snapshot::UpdateSnapshot;

/// All network results needed by the locked commit phase.
pub(super) struct PreparedUpdateArtifacts {
    pub(super) refreshed_sources: Vec<TrackedSource>,
    pub(super) replacements: Vec<Replacement>,
    pub(super) host_install: Option<HostInstall>,
}

impl PreparedUpdateArtifacts {
    pub(super) fn replacement_paths(&self) -> Vec<PathBuf> {
        self.replacements
            .iter()
            .map(|replacement| replacement.path.clone())
            .collect()
    }

    pub(super) fn host_install_path(&self) -> Option<PathBuf> {
        self.host_install
            .as_ref()
            .map(|install| install.game_dir.join(&install.name))
    }
}

pub(super) async fn prepare_update_artifacts(
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
    let mut replacements = Vec::new();
    let mut host_install = None;
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

pub(super) struct HostInstall {
    pub(super) game_dir: PathBuf,
    pub(super) name: String,
    pub(super) bytes: Vec<u8>,
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
