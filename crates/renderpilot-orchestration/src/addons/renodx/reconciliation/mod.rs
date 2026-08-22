use std::path::{Path, PathBuf};

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, InstalledAddonHostKind, PathRef,
    SharedArtifactOrigin, TrackedSource, TrackedSourceRole,
};

use crate::addons::records;
use crate::addons::vulkan_lock;
use crate::file_mutation::V2DiskObservation;
use crate::{Context, ServiceError};

use super::{errors, source, vulkan};
use crate::addons::reshade::host_policy::{self, HostLifecycle};
use crate::addons::reshade::scan as reshade;
use crate::addons::reshade::source::reshade_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};

/// An on-disk RenoDX install found while no per-game DB row exists.
#[derive(Debug, Clone)]
pub(crate) struct OrphanedInstall {
    pub(crate) game_id: GameId,
    pub(crate) game_dir: PathBuf,
    pub(crate) addon_file: PathBuf,
    /// Detected runtime file, when one is actually present. A DB-loss recovery
    /// may legitimately find only the add-on payload; absent runtime files are
    /// never invented or claimed.
    pub(crate) host_file: Option<PathBuf>,
    pub(crate) host_kind: InstalledAddonHostKind,
    pub(crate) registered_exe_path: Option<PathBuf>,
    pub(crate) reshade_config: ReshadeSourceCatalog,
    pub(crate) game_arch: Option<Architecture>,
    pub(crate) addon_url: Option<String>,
}

/// Adopts an on-disk RenoDX install into the per-game DB if the row is still
/// missing. The returned record is re-read from storage so DB timestamps are
/// present and the caller reports the same state future queries will see.
///
/// Callers must already hold the per-game `game_mutation_lock` (see
/// `load_availability`). Nested `try_lock` would silently no-op.
///
/// A record already present for a **different** addon kind (e.g. Luma) is never
/// adopted over: this returns `Ok(None)` exactly as if there were nothing to
/// adopt, so a foreign-tool install is never silently overwritten.
pub(crate) fn reconcile_orphaned_install_locked(
    context: &Context,
    candidate: &OrphanedInstall,
) -> Result<Option<InstalledAddon>, ServiceError> {
    if records::foreign_record(context, &candidate.game_id, AddonKind::RenoDx)?.is_some() {
        return Ok(None);
    }
    if let Some(record) =
        records::active_record_of_kind(context, &candidate.game_id, AddonKind::RenoDx)?
    {
        return Ok(Some(record));
    }

    let record = build_adopted_record(candidate)?;
    context.storage().upsert_installed_addon(&record)?;

    if matches!(
        candidate.host_kind,
        InstalledAddonHostKind::SharedVulkanLayer
    ) && candidate.host_file.is_some()
    {
        record_shared_vulkan_layer_best_effort(context);
    }

    let record = records::record_of_kind(context, &candidate.game_id, AddonKind::RenoDx)?
        .ok_or_else(|| errors::failed("adopted RenoDX install was not persisted".to_owned()))?;
    Ok(Some(record))
}

fn build_adopted_record(candidate: &OrphanedInstall) -> Result<InstalledAddon, ServiceError> {
    let mut record = InstalledAddon::new(
        candidate.game_id.clone(),
        AddonKind::RenoDx,
        path_ref("add-on", &candidate.addon_file)?,
    )
    .with_host_kind(candidate.host_kind);

    let adopts_proxy_runtime = matches!(candidate.host_kind, InstalledAddonHostKind::Proxy)
        && candidate
            .host_file
            .as_deref()
            .is_some_and(|host_file| may_adopt_proxy_runtime(&record, candidate, host_file));
    record = attach_advisory_provenance(record, candidate, adopts_proxy_runtime);

    match candidate.host_kind {
        InstalledAddonHostKind::Proxy => {
            if let Some(host_file) = candidate.host_file.as_deref()
                && adopts_proxy_runtime
            {
                record = with_created_path(record, host_file)?;
                let paths = reshade::resolve_paths(&candidate.game_dir, Some(host_file));
                if let Some(ini_path) = paths.ini_path.filter(|path| path.is_file()) {
                    record = with_created_path(record, &ini_path)?;
                }
            }
        }
        InstalledAddonHostKind::SharedVulkanLayer => {
            let exe_path = candidate.registered_exe_path.as_deref().ok_or_else(|| {
                errors::invalid(
                    "cannot adopt a Vulkan RenoDX install without a resolved executable".to_owned(),
                )
            })?;
            record = record.with_registered_exe_path(path_ref("registered executable", exe_path)?);
        }
    }

    // The binding resolver is the authority for whole-row DLSS adoption: no
    // prefix scan or game-architecture-derived companion is permitted.
    if let Some(source) = build_advisory_dlss_fix_source(&record) {
        record = record.with_tracked_source(source);
    }
    // A whole-row DB-loss adoption may record both advisory provenance and the
    // one exact regular created path. Active rows are handled separately.
    let binding = super::dlss_fix_binding::resolve(&record);
    if binding.state == super::dlss_fix_binding::DlssFixBindingState::SourceOnly
        && matches!(binding.observation, V2DiskObservation::Regular { .. })
    {
        record = with_created_path(record, &binding.target)?;
    }

    Ok(record)
}

/// Attaches best-effort non-DLSS advisory provenance to a freshly adopted record: the
/// guessed ReShade channel (from the host file's PE identity strings) for
/// every host kind, a tracked `HostBinary` source for Proxy installs only, a
/// tracked `AddonPayload` source for the main add-on. DLSS-Fix provenance is
/// added separately through the central binding resolver. A Vulkan layer's real host
/// provenance lives in the shared-artifact table (see
/// [`record_shared_vulkan_layer_best_effort`]), so only the channel guess is
/// useful for its host binary here; DLSS-Fix is unaffected by host kind since
/// it is always a per-game file next to the main add-on. Every step degrades
/// gracefully: a file that cannot be inspected or hashed, or a channel/URL
/// that cannot be resolved, just leaves the record without that piece of
/// provenance. A recognized custom build (see
/// [`reshade::is_known_custom_build`], e.g. GShade) gets neither a channel
/// guess nor a `HostBinary` source at all -- RenoDX has no business guessing at
/// a build it doesn't own the update path for.
fn attach_advisory_provenance(
    mut record: InstalledAddon,
    candidate: &OrphanedInstall,
    owns_proxy_runtime: bool,
) -> InstalledAddon {
    let may_describe_host =
        candidate.host_kind == InstalledAddonHostKind::SharedVulkanLayer || owns_proxy_runtime;
    if may_describe_host {
        match candidate
            .host_file
            .as_deref()
            .and_then(renderpilot_detection::inspect_pe)
        {
            Some(pe) if reshade::is_known_custom_build(&candidate.game_dir, Some(&pe.identity)) => {
            }
            Some(pe) => {
                let channel = reshade::guess_advisory_channel(&pe.identity);
                record = record.with_reshade_channel(channel.as_str());

                if candidate.host_kind == InstalledAddonHostKind::Proxy
                    && let Some(source) = build_advisory_host_source(candidate, channel)
                {
                    record = record.with_tracked_source(source);
                }
            }
            None => {
                if let Some(host_file) = candidate.host_file.as_deref() {
                    log::debug!(
                        "Failed to inspect PE for adopted file: {}",
                        host_file.display()
                    );
                }
            }
        }
    }

    if let Some(source) = build_advisory_addon_source(candidate) {
        record = record.with_tracked_source(source);
    }

    record
}

/// Best-effort advisory `HostBinary` source for an adopted Proxy install: the
/// manifest URL for the guessed channel/architecture, and the on-disk file's
/// digest. `None` if the manifest has no URL for that channel/architecture or
/// the file cannot be hashed.
fn build_advisory_host_source(
    candidate: &OrphanedInstall,
    channel: ReshadeChannel,
) -> Option<TrackedSource> {
    let host_file = candidate.host_file.as_deref()?;
    let arch = candidate.game_arch.unwrap_or(Architecture::X64);
    let url = reshade_source(&candidate.reshade_config, channel, arch)?.url;

    let digest = match renderpilot_detection::sha256_file(host_file) {
        Ok(digest) => digest.to_string(),
        Err(error) => {
            log::debug!(
                "Failed to hash adopted file {}: {error}",
                host_file.display()
            );
            return None;
        }
    };

    Some(
        TrackedSource::new(TrackedSourceRole::HostBinary, url, None, digest)
            .with_channel(channel.as_str())
            .with_advisory(),
    )
}

/// Best-effort advisory `AddonPayload` source for an adopted install's add-on
/// file. `None` if the resolved add-on URL is unknown/empty or the file
/// cannot be hashed.
fn build_advisory_addon_source(candidate: &OrphanedInstall) -> Option<TrackedSource> {
    let url = candidate
        .addon_url
        .as_deref()
        .filter(|url| !url.is_empty())?;

    let digest = match renderpilot_detection::sha256_file(&candidate.addon_file) {
        Ok(digest) => digest.to_string(),
        Err(error) => {
            log::debug!(
                "Failed to hash adopted add-on {}: {error}",
                candidate.addon_file.display()
            );
            return None;
        }
    };

    Some(
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            url.to_owned(),
            None,
            digest,
        )
        .with_advisory(),
    )
}

/// Best-effort DLSS-Fix provenance for a whole-row adoption. The binding
/// resolver establishes the one exact companion from the recorded main add-on;
/// this code never discovers a companion by scanning a directory.
fn build_advisory_dlss_fix_source(record: &InstalledAddon) -> Option<TrackedSource> {
    let binding = super::dlss_fix_binding::resolve(record);
    let arch = binding.arch?;
    let V2DiskObservation::Regular { digest } = binding.observation else {
        return None;
    };
    Some(
        TrackedSource::new(
            TrackedSourceRole::DlssFix,
            source::dlss_fix_url(arch),
            None,
            digest,
        )
        .with_advisory(),
    )
}

fn may_adopt_proxy_runtime(
    record: &InstalledAddon,
    candidate: &OrphanedInstall,
    host_file: &Path,
) -> bool {
    let Some(proxy_dll_name) = host_file.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let mut allowed = vec![
        candidate
            .addon_file
            .file_name()
            .and_then(|name| name.to_str()),
    ];
    let binding = super::dlss_fix_binding::resolve(record);
    if matches!(binding.observation, V2DiskObservation::Regular { .. }) {
        allowed.push(binding.target.file_name().and_then(|name| name.to_str()));
    }
    let allowed: Vec<&str> = allowed.into_iter().flatten().collect();
    let assessment = host_policy::assess_for_tool_with_allowed_addons(
        &candidate.game_dir,
        proxy_dll_name,
        "RenoDX",
        None,
        &allowed,
    );
    assessment.lifecycle == HostLifecycle::AdoptEmpty
        && crate::paths::same_path(&assessment.target_path, host_file)
}

fn with_created_path(
    mut record: InstalledAddon,
    path: &Path,
) -> Result<InstalledAddon, ServiceError> {
    let path = path_ref("created file", path)?;
    if !record.created_files().contains(&path) {
        record = record.with_created_file(path);
    }
    Ok(record)
}

fn path_ref(label: &str, path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().into_owned())
        .map_err(|error| errors::failed(format!("invalid adopted {label} path: {error}")))
}

/// Persists an advisory record of the adopted shared Vulkan layer. Best-effort
/// by design: this is a read-path convenience (see
/// [`reconcile_orphaned_install_locked`]), so a failure here -- including losing
/// the race for the shared-layer lock -- leaves only optional provenance absent;
/// it never blocks or fails the availability read the caller actually asked for.
fn record_shared_vulkan_layer_best_effort(context: &Context) {
    let Some(_guard) = vulkan_lock::try_shared_vulkan_lock() else {
        log::debug!("deferred adopted Vulkan layer record: local layer boundary is busy");
        return;
    };
    match context.storage().pending_shared_vulkan_mutation() {
        Ok(Some(_)) => {
            log::debug!("deferred adopted Vulkan layer record: durable mutation is pending");
            return;
        }
        Ok(None) => {}
        Err(error) => {
            log::warn!(
                "skipped persisting adopted Vulkan layer record: pending mutation query failed: {error}"
            );
            return;
        }
    }

    if !matches!(
        vulkan::layer_report().detection(),
        crate::addons::renodx::vulkan::VulkanLayerDetection::Installed
            | crate::addons::renodx::vulkan::VulkanLayerDetection::InstalledDisabled
    ) {
        return;
    }
    let record = match super::platform::vulkan::shared_artifact::detected_record(
        SharedArtifactOrigin::AdoptedOfficial,
        None,
    ) {
        Ok(record) => record,
        Err(error) => {
            log::warn!("failed to build adopted Vulkan layer record: {error}");
            return;
        }
    };
    match context
        .storage()
        .try_upsert_shared_artifact_if_unreserved(&record)
    {
        Ok(renderpilot_storage_sqlite::ConditionalSharedArtifactWrite::Applied) => {}
        Ok(renderpilot_storage_sqlite::ConditionalSharedArtifactWrite::Deferred) => {
            log::debug!("deferred adopted Vulkan layer record: reservation won the database race");
        }
        Err(error) => log::warn!("failed to persist adopted Vulkan layer record: {error}"),
    }
}

#[cfg(test)]
mod tests;
