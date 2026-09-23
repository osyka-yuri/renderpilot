//! Preparation of independently distributed, manifest-pinned OptiScaler modules.

use std::collections::HashSet;

use renderpilot_domain::Sha256Hash;

use crate::net::ProgressObserver;
use crate::{ServiceError, failed};

use super::artifact_descriptor::VerifiedArtifactDescriptor;
use super::matcher::module_artifact_for_release;
use super::types::{OptiScalerManifest, OptiScalerModuleArtifact, OptiScalerRelease};

#[derive(Debug)]
pub(crate) struct PreparedModuleArtifact {
    pub(crate) module_id: String,
    pub(crate) manifest: OptiScalerModuleArtifact,
    pub(crate) bytes: Vec<u8>,
    pub(crate) sha256: Sha256Hash,
}

pub(crate) async fn prepare_selected(
    manifest: &OptiScalerManifest,
    release: &OptiScalerRelease,
    modules: &HashSet<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Vec<PreparedModuleArtifact>, ServiceError> {
    let mut prepared = Vec::new();
    for module_id in modules {
        let Some(artifact) = module_artifact_for_release(manifest, release, module_id) else {
            continue;
        };
        prepared.push(prepare_one(module_id, artifact, progress).await?);
    }
    prepared.sort_by(|left, right| left.module_id.cmp(&right.module_id));
    Ok(prepared)
}

async fn prepare_one(
    module_id: &str,
    artifact: &OptiScalerModuleArtifact,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedModuleArtifact, ServiceError> {
    let descriptor = VerifiedArtifactDescriptor::module(module_id, artifact)?;

    if let Some(bytes) = load_cached_module_off_runtime(descriptor.clone()).await? {
        return Ok(PreparedModuleArtifact {
            module_id: module_id.to_owned(),
            manifest: artifact.clone(),
            bytes,
            sha256: descriptor.sha256,
        });
    }

    let operation = format!("OptiScaler module {module_id}");
    let download = crate::net::download_exact_with_url_chain(
        descriptor.source_url.as_str(),
        descriptor.size,
        &operation,
        progress,
    )
    .await?;
    super::source::validate_download_chain(&descriptor.source_url, &download.url_chain)?;
    let bytes = validate_owned_bytes_off_runtime(descriptor.clone(), download.bytes).await?;
    let bytes = write_cached_module_off_runtime(descriptor.cache_path.clone(), bytes).await?;
    Ok(PreparedModuleArtifact {
        module_id: module_id.to_owned(),
        manifest: artifact.clone(),
        bytes,
        sha256: descriptor.sha256,
    })
}

async fn load_cached_module_off_runtime(
    descriptor: VerifiedArtifactDescriptor,
) -> Result<Option<Vec<u8>>, ServiceError> {
    let cache_path = descriptor.cache_path.clone();
    tokio::task::spawn_blocking(move || match std::fs::read(&cache_path) {
        Ok(bytes) if descriptor.validate_bytes(&bytes).is_ok() => Ok(Some(bytes)),
        Ok(_) => {
            tracing::warn!(
                "discarding invalid OptiScaler module cache {}",
                cache_path.display()
            );
            match std::fs::remove_file(&cache_path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(failed(format!(
                        "failed to remove invalid OptiScaler module cache {}: {error}",
                        cache_path.display()
                    )));
                }
            }
            Ok(None)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failed(format!(
            "failed to read OptiScaler module cache {}: {error}",
            cache_path.display()
        ))),
    })
    .await
    .map_err(|error| failed(format!("OptiScaler module cache task failed: {error}")))?
}

async fn validate_owned_bytes_off_runtime(
    descriptor: VerifiedArtifactDescriptor,
    bytes: Vec<u8>,
) -> Result<Vec<u8>, ServiceError> {
    tokio::task::spawn_blocking(move || {
        descriptor.validate_bytes(&bytes)?;
        Ok(bytes)
    })
    .await
    .map_err(|error| failed(format!("OptiScaler module validation task failed: {error}")))?
}

async fn write_cached_module_off_runtime(
    cache_path: std::path::PathBuf,
    bytes: Vec<u8>,
) -> Result<Vec<u8>, ServiceError> {
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                failed(format!(
                    "failed to create OptiScaler module cache {}: {error}",
                    parent.display()
                ))
            })?;
        }
        crate::fs::write_file_atomically(&cache_path, &bytes)?;
        Ok(bytes)
    })
    .await
    .map_err(|error| {
        failed(format!(
            "OptiScaler module cache write task failed: {error}"
        ))
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::optiscaler::archive::sha256_hex;
    use crate::addons::optiscaler::types::{OptiScalerReleaseProvider, OptiScalerReleaseSource};

    fn artifact() -> OptiScalerModuleArtifact {
        OptiScalerModuleArtifact {
            id: "fixture".to_owned(),
            source: OptiScalerReleaseSource {
                provider: OptiScalerReleaseProvider::GithubRelease,
                repository: "optiscaler/OptiPatcher".to_owned(),
                tag: "rolling".to_owned(),
                asset: "OptiPatcher.asi".to_owned(),
            },
            sha256: sha256_hex(b"not-a-pe"),
            size: 8,
            target: "plugins/OptiPatcher.asi".to_owned(),
            pe_x64: true,
        }
    }

    #[test]
    fn rejects_hash_valid_bytes_when_the_pinned_pe_identity_is_wrong() {
        let descriptor =
            VerifiedArtifactDescriptor::module("optipatcher", &artifact()).expect("descriptor");
        assert!(descriptor.validate_bytes(b"not-a-pe").is_err());
    }
}
