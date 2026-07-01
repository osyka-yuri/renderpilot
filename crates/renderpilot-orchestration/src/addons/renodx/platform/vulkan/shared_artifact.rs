/// Advisory shared-artifact persistence for the shared Vulkan layer.
use super::program_data::standard_paths;
use std::path::Path;

use crate::ServiceError;
use crate::addons::renodx::errors;
use crate::addons::renodx::fetch::Download;
use crate::addons::renodx::source::ReshadeSource;
use crate::addons::renodx::types::ReshadeChannel;
use renderpilot_application::SharedArtifactRepository;
use renderpilot_domain::PathRef;
use renderpilot_domain::{
    SharedArtifactKind, SharedArtifactOrigin, SharedArtifactRecord, SharedArtifactSource,
};

fn base_shared_record(origin: SharedArtifactOrigin) -> Result<SharedArtifactRecord, ServiceError> {
    let (install_dir, manifest_path, dll_path) = standard_paths()
        .ok_or_else(|| errors::failed("Failed to determine Vulkan layer paths".to_owned()))?;

    let install_dir_ref = path_ref("install", &install_dir)?;
    let manifest_path_ref = path_ref("manifest", &manifest_path)?;
    let dll_path_ref = path_ref("dll", &dll_path)?;

    Ok(SharedArtifactRecord::new(
        SharedArtifactKind::RenoDxVulkanLayer,
        install_dir_ref,
        manifest_path_ref,
        dll_path_ref,
        origin,
    ))
}

pub(crate) fn record_detected_layer(
    storage: &impl SharedArtifactRepository,
    origin: SharedArtifactOrigin,
    channel: Option<ReshadeChannel>,
) -> Result<(), ServiceError> {
    let mut record = base_shared_record(origin)?;
    if let Some(channel) = channel {
        record = record.with_source(SharedArtifactSource {
            url: None,
            etag: None,
            digest: None,
            last_modified: None,
            channel: Some(channel.as_str().to_owned()),
        });
    }
    storage
        .upsert_shared_artifact(&record)
        .map_err(|e| errors::failed(format!("Failed to record detected layer: {}", e)))
}

pub(crate) fn record_downloaded_layer(
    storage: &impl SharedArtifactRepository,
    source: &ReshadeSource,
    download: &Download,
    origin: SharedArtifactOrigin,
) -> Result<(), ServiceError> {
    let record = base_shared_record(origin)?
        .with_source(SharedArtifactSource {
            url: Some(source.url.clone()),
            etag: download.etag.clone(),
            digest: Some(download.digest.clone()),
            last_modified: download.last_modified.clone(),
            channel: Some(source.channel.as_str().to_owned()),
        })
        .with_created_files(shared_layer_created_files()?);
    storage
        .upsert_shared_artifact(&record)
        .map_err(|e| errors::failed(format!("Failed to record downloaded layer: {}", e)))
}

/// Deletes the advisory shared-artifact record after the layer itself has
/// already been removed from disk. Best-effort: the layer removal is the
/// operation that matters, and a stale advisory record is self-correcting —
/// the next `record_detected_layer`/`record_downloaded_layer` call overwrites
/// it — but a failure here is still worth a log line rather than silence.
pub(crate) fn forget_layer_record(storage: &impl SharedArtifactRepository) {
    if let Err(error) = storage.delete_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer) {
        log::warn!("failed to forget the shared Vulkan layer's advisory record: {error}");
    }
}

pub(crate) fn shared_layer_created_files() -> Result<Vec<PathRef>, ServiceError> {
    let (_, manifest_path, dll_path) = standard_paths()
        .ok_or_else(|| errors::failed("Failed to determine Vulkan layer paths".to_owned()))?;

    let manifest_path_ref = path_ref("manifest", &manifest_path)?;
    let dll_path_ref = path_ref("dll", &dll_path)?;

    Ok(vec![manifest_path_ref, dll_path_ref])
}

fn path_ref(label: &str, path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().into_owned())
        .map_err(|error| errors::failed(format!("Invalid {label} path: {error}")))
}

pub(crate) fn stored_layer_digest(storage: &impl SharedArtifactRepository) -> Option<String> {
    storage
        .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
        .ok()
        .flatten()
        .and_then(|r| r.source_digest().map(str::to_owned))
}

pub(crate) fn stored_layer_channel(
    storage: &impl SharedArtifactRepository,
) -> Option<ReshadeChannel> {
    storage
        .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
        .ok()
        .flatten()
        .and_then(|record| ReshadeChannel::parse_recorded(record.channel()).into_parsed())
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;
    use crate::addons::renodx::test_support::InMemorySharedArtifactRepository;

    #[test]
    fn record_detected_layer_persists_channel_without_empty_url() {
        let storage = InMemorySharedArtifactRepository::default();

        record_detected_layer(
            &storage,
            SharedArtifactOrigin::AdoptedOfficial,
            Some(ReshadeChannel::Nightly),
        )
        .expect("detected layer should persist");

        let record = storage
            .get_shared_artifact(SharedArtifactKind::RenoDxVulkanLayer)
            .expect("repository should read")
            .expect("record should exist");

        assert_eq!(record.origin(), SharedArtifactOrigin::AdoptedOfficial);
        assert_eq!(record.channel(), Some("nightly"));
        assert_eq!(record.source_url(), None);
    }
}
