/// Advisory shared-artifact persistence for the shared Vulkan layer.
use super::program_data::standard_paths;
use std::path::Path;

use crate::ServiceError;
use crate::addons::renodx::errors;
use crate::addons::reshade::fetch::Download;
use crate::addons::reshade::source::ReshadeSource;
use crate::addons::reshade::types::ReshadeChannel;
use renderpilot_application::SharedArtifactRepository;
use renderpilot_domain::PathRef;
use renderpilot_domain::{
    SharedArtifactKind, SharedArtifactOrigin, SharedArtifactRecord, SharedArtifactSource,
};

pub(crate) fn detected_record(
    origin: SharedArtifactOrigin,
    channel: Option<ReshadeChannel>,
) -> Result<SharedArtifactRecord, ServiceError> {
    let (install_dir, manifest_path, dll_path) = standard_paths()
        .ok_or_else(|| errors::failed("Failed to determine Vulkan layer paths".to_owned()))?;

    let install_dir_ref = path_ref("install", &install_dir)?;
    let manifest_path_ref = path_ref("manifest", &manifest_path)?;
    let dll_path_ref = path_ref("dll", &dll_path)?;

    let mut record = SharedArtifactRecord::new(
        SharedArtifactKind::RenoDxVulkanLayer,
        install_dir_ref,
        manifest_path_ref,
        dll_path_ref,
        origin,
    );
    if let Some(channel) = channel {
        record = record.with_source(SharedArtifactSource {
            url: None,
            etag: None,
            digest: None,
            last_modified: None,
            channel: Some(channel.as_str().to_owned()),
        });
    }
    Ok(record)
}

/// Builds the exact advisory projection for a downloaded layer without
/// writing it.  The shared mutation coordinator uses this value in the same
/// storage commit as the filesystem participants.
pub(crate) fn downloaded_record(
    layer_dir: &Path,
    source: &ReshadeSource,
    download: &Download,
) -> Result<SharedArtifactRecord, ServiceError> {
    let install_dir = path_ref("install", layer_dir)?;
    let manifest_path = path_ref("manifest", &layer_dir.join("ReShade64.json"))?;
    let dll_path = path_ref("dll", &layer_dir.join("ReShade64.dll"))?;
    let created_files = vec![manifest_path.clone(), dll_path.clone()];
    Ok(SharedArtifactRecord::new(
        SharedArtifactKind::RenoDxVulkanLayer,
        install_dir,
        manifest_path,
        dll_path,
        SharedArtifactOrigin::RenderPilotCreated,
    )
    .with_source(SharedArtifactSource {
        url: Some(source.url.clone()),
        etag: download.etag.clone(),
        digest: Some(download.digest.clone()),
        last_modified: download.last_modified.clone(),
        channel: Some(source.channel.as_str().to_owned()),
    })
    .with_created_files(created_files))
}

/// Converts a platform path into the domain's validated path reference.
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

        let record = detected_record(
            SharedArtifactOrigin::AdoptedOfficial,
            Some(ReshadeChannel::Nightly),
        )
        .expect("detected layer record");
        storage
            .upsert_shared_artifact(&record)
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
