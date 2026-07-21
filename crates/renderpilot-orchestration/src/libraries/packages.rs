//! Materialization and lifecycle of explicit catalog packages.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};

use renderpilot_application::ArtifactRepository;
use renderpilot_domain::{ArtifactId, LibraryArtifact};

use crate::ServiceError;
use crate::net::{DownloadProgress, ProgressObserver};

use super::resolved::{ResolvedPackage, ValidatedCatalog};
use super::storage::LibraryStorage;
use super::types::{LibraryArtifactRecord, LibraryPackage, LibraryPackageState};
use super::{artifact_builder, compression, library_error, validate};

pub(super) async fn ensure_package_downloaded(
    context: &crate::Context,
    resolved: &ResolvedPackage<'_>,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<LibraryPackageState, ServiceError> {
    let storage = LibraryStorage::discover()?;
    let package = resolved.package();

    // Refuse unsupported semantic families before downloading any bytes.
    let virtual_artifact =
        artifact_builder::build_catalog_artifact(resolved, None)?.ok_or_else(|| {
            library_error(format!(
                "package `{}` uses unsupported technology `{}`",
                package.package_id, package.technology
            ))
        })?;

    let total_bytes = resolved.members().try_fold(0_u64, |total, artifact| {
        total
            .checked_add(artifact.transport.size_bytes)
            .ok_or_else(|| {
                library_error(format!(
                    "package `{}` transport size overflows u64",
                    package.package_id
                ))
            })
    })?;
    let mut local_paths = Vec::with_capacity(resolved.members().len());
    let mut completed_bytes = 0_u64;
    for artifact in resolved.members() {
        let offset = completed_bytes;
        let aggregate_progress = progress.map(|observer| {
            move |member: DownloadProgress<'_>| {
                observer(DownloadProgress {
                    downloaded_bytes: offset + member.downloaded_bytes,
                    total_bytes,
                    phase: member.phase,
                });
            }
        });
        let member_progress = aggregate_progress
            .as_ref()
            .map(|observer| observer as &ProgressObserver<'_>);
        local_paths.push(ensure_artifact(&storage, artifact, member_progress).await?);
        completed_bytes += artifact.transport.size_bytes;
    }

    let local_artifact =
        artifact_builder::build_catalog_artifact(resolved, Some(local_paths.as_slice()))?
            .ok_or_else(|| {
                library_error(format!(
                    "package `{}` became unsupported during materialization",
                    package.package_id
                ))
            })?;
    debug_assert_eq!(local_artifact.id(), virtual_artifact.id());

    register_and_commit(context, package, &local_artifact)
}

async fn ensure_artifact(
    storage: &LibraryStorage,
    artifact: &LibraryArtifactRecord,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PathBuf, ServiceError> {
    ensure_artifact_with(storage, artifact, progress, || async {
        crate::net::download_exact_bytes(
            &crate::cdn::cdn_url(&artifact.transport.object_key),
            artifact.transport.size_bytes,
            "library artifact download",
            progress,
        )
        .await
    })
    .await
}

pub(super) async fn ensure_artifact_with<F, Fut>(
    storage: &LibraryStorage,
    artifact: &LibraryArtifactRecord,
    progress: Option<&ProgressObserver<'_>>,
    download: F,
) -> Result<PathBuf, ServiceError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Vec<u8>, ServiceError>>,
{
    let _lock = super::locks::acquire(format!("artifact-digest:{}", artifact.dll.sha256)).await;
    ensure_artifact_locked(storage, artifact, progress, download).await
}

async fn ensure_artifact_locked<F, Fut>(
    storage: &LibraryStorage,
    artifact: &LibraryArtifactRecord,
    progress: Option<&ProgressObserver<'_>>,
    download: F,
) -> Result<PathBuf, ServiceError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Vec<u8>, ServiceError>>,
{
    let dll_path = storage.local_dll_path(&artifact.dll.sha256, &artifact.file_name);
    if file_has_content(&dll_path, artifact.dll.size_bytes, &artifact.dll.sha256)? {
        report_complete(progress, artifact.transport.size_bytes);
        return Ok(dll_path);
    }
    crate::fs::remove_file_if_exists(&dll_path)?;

    let archive_path = storage.local_archive_path(&artifact.transport.sha256);
    let transport_lock =
        super::locks::acquire(format!("transport-digest:{}", artifact.transport.sha256)).await;
    let payload = match read_valid_archive(&archive_path, artifact)? {
        Some(bytes) => {
            report_complete(progress, artifact.transport.size_bytes);
            bytes
        }
        None => {
            crate::fs::remove_file_if_exists(&archive_path)?;
            let bytes = download().await?;
            validate::validate_transport(artifact, &bytes)?;
            crate::fs::write_file_atomically(&archive_path, &bytes)?;
            bytes
        }
    };
    drop(transport_lock);

    let dll_bytes = compression::decompress_library(artifact, &payload)?;
    validate::validate_dll_hash(&artifact.artifact_id, &artifact.dll.sha256, &dll_bytes)?;
    crate::fs::write_file_atomically(&dll_path, &dll_bytes)?;
    Ok(dll_path)
}

pub(super) fn read_valid_archive(
    path: &Path,
    artifact: &LibraryArtifactRecord,
) -> Result<Option<Vec<u8>>, ServiceError> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = crate::fs::read_file(path)?;
    match validate::validate_transport(artifact, &bytes) {
        Ok(()) => Ok(Some(bytes)),
        Err(error) => {
            log::warn!(
                "discarding invalid cached archive for {}: {error}",
                artifact.artifact_id
            );
            Ok(None)
        }
    }
}

fn file_has_content(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<bool, ServiceError> {
    if !path.is_file() {
        return Ok(false);
    }
    let metadata = std::fs::metadata(path)
        .map_err(|error| library_error(format!("failed to read DLL metadata: {error}")))?;
    if metadata.len() != expected_size {
        return Ok(false);
    }
    let actual = crate::fs::sha256_of_non_empty_file(path).map_err(ServiceError::from)?;
    Ok(actual.as_str() == expected_sha256)
}

fn report_complete(progress: Option<&ProgressObserver<'_>>, total_bytes: u64) {
    if let Some(observer) = progress {
        observer(DownloadProgress {
            downloaded_bytes: total_bytes,
            total_bytes,
            phase: Some("library artifact cache"),
        });
    }
}

pub(super) fn register_and_commit(
    context: &crate::Context,
    package: &LibraryPackage,
    artifact: &LibraryArtifact,
) -> Result<LibraryPackageState, ServiceError> {
    context
        .storage()
        .upsert_artifact(artifact)
        .map_err(ServiceError::from)?;
    Ok(package_state(package, true, Some(artifact.id())))
}

pub(super) fn delete_package(
    context: &crate::Context,
    resolved: &ResolvedPackage<'_>,
) -> Result<LibraryPackageState, ServiceError> {
    let package = resolved.package();
    let artifact_id = resolved.artifact_id();
    unregister_package(context, package, artifact_id)
}

pub(super) fn unregister_package(
    context: &crate::Context,
    package: &LibraryPackage,
    artifact_id: &ArtifactId,
) -> Result<LibraryPackageState, ServiceError> {
    context
        .storage()
        .delete_artifact(artifact_id)
        .map_err(ServiceError::from)?;
    // Content-addressed DLLs and archives may be shared. They are intentionally
    // left for a future explicit orphan-GC pass.
    Ok(package_state(package, false, None))
}

pub(super) fn package_states(
    context: &crate::Context,
    catalog: &ValidatedCatalog,
) -> Result<Vec<LibraryPackageState>, ServiceError> {
    let storage = LibraryStorage::discover()?;
    let registered_ids: std::collections::HashSet<_> = context
        .storage()
        .list_artifacts()
        .map_err(ServiceError::from)?
        .into_iter()
        .map(|artifact| artifact.id().clone())
        .collect();
    let mut verified_content = HashMap::new();
    let mut states = Vec::new();
    for resolved in catalog.packages() {
        let package = resolved.package();
        if !artifact_builder::package_is_supported(package) {
            log::warn!(
                "catalog package `{}` uses unknown technology `{}`; skipping its state",
                package.package_id,
                package.technology
            );
            continue;
        }
        let artifact_id = resolved.artifact_id();
        let mut downloaded = registered_ids.contains(artifact_id);
        if downloaded {
            for artifact in resolved.members() {
                let path = storage.local_dll_path(&artifact.dll.sha256, &artifact.file_name);
                let has_content = match verified_content.get(&path) {
                    Some(result) => *result,
                    None => {
                        let result =
                            file_has_content(&path, artifact.dll.size_bytes, &artifact.dll.sha256)?;
                        verified_content.insert(path, result);
                        result
                    }
                };
                if !has_content {
                    downloaded = false;
                    break;
                }
            }
        }
        states.push(package_state(
            package,
            downloaded,
            downloaded.then_some(artifact_id),
        ));
    }
    Ok(states)
}

fn package_state(
    package: &LibraryPackage,
    is_downloaded: bool,
    artifact_id: Option<&ArtifactId>,
) -> LibraryPackageState {
    LibraryPackageState {
        package_id: package.package_id.clone(),
        version: package.release.version.clone(),
        is_downloaded,
        artifact_id: artifact_id.map(|id| id.as_str().to_owned()),
    }
}
