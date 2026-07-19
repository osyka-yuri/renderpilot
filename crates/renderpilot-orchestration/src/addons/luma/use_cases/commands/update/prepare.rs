//! Network and validation phase for a Luma update. Nothing in this module
//! mutates the game folder or persistence state.

use std::path::PathBuf;

use renderpilot_domain::{InstalledAddon, TrackedSource, TrackedSourceRole};

use super::dgvoodoo::{self, PreparedDgVoodooUpdate};
use super::host::{self, PreparedHostUpdate};
use crate::addons::luma::dgvoodoo as managed_dgvoodoo;
use crate::addons::luma::errors;
use crate::addons::luma::fetch;
use crate::addons::luma::fetch::types::LumaPayload;
use crate::addons::luma::tracking::{
    payload_disk_intact, promote_advisory_payload_source, refresh_addon_validators,
    resolved_addon_version,
};
use crate::addons::luma::types::LumaManifest;
use crate::addons::luma::use_cases::update_target::{self, ResolvedUpdateTarget};
use crate::addons::records::source_with_role;
use crate::addons::reshade::types::ReshadeSourceCatalog;
use crate::addons::update::{UpdateStatus, validator_fast_path};
use crate::net::{ProgressObserver, head_validators};
use crate::{Context, ServiceError};

/// Fully prepared update. Every remote artifact has already been downloaded
/// and validated; applying either variant is a local transaction.
pub(super) enum PreparedUpdate {
    HostOnly(Box<PreparedHostOnly>),
    Full(Box<PreparedFullUpdate>),
}

impl PreparedUpdate {
    pub(super) fn host_write_path(&self) -> Option<&std::path::Path> {
        match self {
            Self::HostOnly(p) => p.host.write_path(),
            Self::Full(p) => p.host.write_path(),
        }
    }

    pub(super) fn dgvoodoo_write_paths(&self) -> Vec<std::path::PathBuf> {
        match self {
            Self::HostOnly(p) => p.dgvoodoo.write_paths(),
            Self::Full(p) => p.dgvoodoo.write_paths(),
        }
    }
}

pub(super) struct PreparedHostOnly {
    pub(super) target: ResolvedUpdateTarget,
    pub(super) sources: Vec<TrackedSource>,
    pub(super) host: PreparedHostUpdate,
    pub(super) dgvoodoo: PreparedDgVoodooUpdate,
    pub(super) addon_version: Option<String>,
}

pub(super) struct PreparedFullUpdate {
    /// Complete live catalogue match used for every apply-time path decision.
    pub(super) target: ResolvedUpdateTarget,
    pub(super) payload: LumaPayload,
    pub(super) host: PreparedHostUpdate,
    pub(super) dgvoodoo: PreparedDgVoodooUpdate,
    /// Absolute managed-dependency paths excluded from the payload set-diff.
    pub(super) dependency_paths: Vec<PathBuf>,
}

pub(super) async fn prepare_update(
    context: &Context,
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
    record: &InstalledAddon,
    progress: Option<&ProgressObserver<'_>>,
    had_torn_marker: bool,
    force_full: bool,
) -> Result<PreparedUpdate, ServiceError> {
    let addon_source = source_with_role(record, TrackedSourceRole::AddonPayload)
        .cloned()
        .ok_or_else(|| {
            errors::invalid("invalid Luma install record: missing add-on payload provenance")
        })?;
    let target = update_target::require_update_target(context, manifest, record.game_id())?;
    let (addon_current, needs_full_payload) =
        resolve_payload_fetch_flags(record, &addon_source, had_torn_marker, force_full).await;

    let payload =
        fetch_payload_if_needed(&target, addon_current, needs_full_payload, progress).await?;
    let host =
        host::prepare_host_update_if_needed(manifest, reshade_sources, &target, record, progress)
            .await?;
    let dgvoodoo = dgvoodoo::prepare_if_needed(&target, record, progress, force_full).await?;

    classify_prepared_update(PreparedClassification {
        record,
        addon_source: &addon_source,
        target,
        payload,
        needs_full_payload,
        host,
        dgvoodoo,
    })
    .await
}

async fn resolve_payload_fetch_flags(
    record: &InstalledAddon,
    addon_source: &TrackedSource,
    had_torn_marker: bool,
    force_full: bool,
) -> (bool, bool) {
    let addon_current = if force_full || addon_source.is_advisory() {
        false
    } else {
        match head_validators(addon_source.url(), "Luma update check").await {
            Ok(validators) => matches!(
                validator_fast_path(addon_source.etag(), validators.cache_validator().as_deref()),
                Some(UpdateStatus::Current)
            ),
            Err(_) => false,
        }
    };
    let needs_full_payload = force_full || had_torn_marker || !payload_disk_intact(record);
    (addon_current, needs_full_payload)
}

async fn fetch_payload_if_needed(
    target: &ResolvedUpdateTarget,
    addon_current: bool,
    needs_full_payload: bool,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Option<LumaPayload>, ServiceError> {
    if addon_current && !needs_full_payload {
        return Ok(None);
    }
    Ok(Some(
        fetch::download::fetch_luma_payload(
            &target.asset,
            &target.addon_file,
            target.arch,
            progress,
        )
        .await?,
    ))
}

struct PreparedClassification<'a> {
    record: &'a InstalledAddon,
    addon_source: &'a TrackedSource,
    target: ResolvedUpdateTarget,
    payload: Option<LumaPayload>,
    needs_full_payload: bool,
    host: PreparedHostUpdate,
    dgvoodoo: PreparedDgVoodooUpdate,
}

async fn classify_prepared_update(
    prepared: PreparedClassification<'_>,
) -> Result<PreparedUpdate, ServiceError> {
    let PreparedClassification {
        record,
        addon_source,
        target,
        payload,
        needs_full_payload,
        host,
        dgvoodoo,
    } = prepared;

    if let Some(payload) = payload {
        let payload_matches_record = if addon_source.is_advisory() {
            fetch::digest::recovery_payload_digest(&payload) == addon_source.digest()
        } else {
            payload.zip_digest == addon_source.digest()
        };
        if needs_full_payload || !payload_matches_record {
            let dependency_paths = managed_dependency_paths(&target, record);
            return Ok(PreparedUpdate::Full(Box::new(PreparedFullUpdate {
                target,
                payload,
                host,
                dgvoodoo,
                dependency_paths,
            })));
        }

        if addon_source.is_advisory() {
            let mut sources = record.tracked_sources().to_vec();
            promote_advisory_payload_source(&mut sources, addon_source, &payload);
            refresh_addon_validators(&mut sources).await;
            return Ok(PreparedUpdate::HostOnly(Box::new(PreparedHostOnly {
                target,
                sources,
                host,
                dgvoodoo,
                addon_version: resolved_addon_version(record, &payload),
            })));
        }
    }

    let mut sources = record.tracked_sources().to_vec();
    refresh_addon_validators(&mut sources).await;
    Ok(PreparedUpdate::HostOnly(Box::new(PreparedHostOnly {
        target,
        sources,
        host,
        dgvoodoo,
        addon_version: record.addon_version().map(str::to_owned),
    })))
}

/// Current-profile dependency paths plus historically owned dependency paths.
/// The latter remain excluded from the payload set-diff across catalogue drift.
fn managed_dependency_paths(
    target: &ResolvedUpdateTarget,
    record: &InstalledAddon,
) -> Vec<PathBuf> {
    use crate::paths::same_path;

    let mut paths: Vec<PathBuf> =
        managed_dgvoodoo::requirement(target.external_requirement.as_ref())
            .map(|requirement| {
                managed_dgvoodoo::game_file_names(requirement)
                    .into_iter()
                    .map(|name| target.game_dir.join(name))
                    .collect()
            })
            .unwrap_or_default();
    for owned in crate::addons::luma::tracking::owned_dependency_paths(record) {
        if !paths.iter().any(|existing| same_path(existing, &owned)) {
            paths.push(owned);
        }
    }
    paths
}
