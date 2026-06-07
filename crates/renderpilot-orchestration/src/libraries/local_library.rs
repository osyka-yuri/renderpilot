use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::ServiceError;
use renderpilot_application::ArtifactRepository;
use renderpilot_domain::{
    ArtifactTrustLevel, ComponentFile, GraphicsTechnology, LibraryArtifact, PathRef,
};

use super::{
    artifact_builder,
    fsr_packages::FsrPackage,
    http, library_error, storage,
    types::{LibraryManifest, LibraryManifestEntry, LibraryState},
    validate,
};

pub(super) struct DownloadedLibrary {
    pub(super) dll_path: PathBuf,
    pub(super) artifact_id: String,
}

pub(super) async fn ensure_downloaded_and_registered(
    context: &crate::Context,
    entry: &LibraryManifestEntry,
) -> Result<DownloadedLibrary, ServiceError> {
    let archive_path = local_archive_path(entry)?;
    let dll_path = local_dll_path(entry)?;

    ensure_local_archive(entry, &archive_path).await?;

    let artifact_id = materialize_local_library(context, entry, &archive_path)?;

    Ok(DownloadedLibrary {
        dll_path,
        artifact_id,
    })
}

pub(super) fn local_archive_path(entry: &LibraryManifestEntry) -> Result<PathBuf, ServiceError> {
    storage::local_archive_path(
        library_id_to_group_key(&entry.library.id),
        &entry.archive_file_name(),
    )
}

fn local_dll_path(entry: &LibraryManifestEntry) -> Result<PathBuf, ServiceError> {
    storage::local_dll_path(
        library_id_to_group_key(&entry.library.id),
        &entry.entry_id,
        &entry.library.file_name,
    )
}

async fn ensure_local_archive(
    entry: &LibraryManifestEntry,
    archive_path: &Path,
) -> Result<(), ServiceError> {
    if archive_path.exists() && archive_is_valid(archive_path, entry)? {
        return Ok(());
    }

    if archive_path.exists() {
        storage::remove_file_if_exists(archive_path)?;
    }

    let payload = download_archive(entry).await?;
    validate::validate_archive_payload(entry, &payload)?;
    storage::write_file_atomically(archive_path, &payload)
}

fn materialize_local_library(
    context: &crate::Context,
    entry: &LibraryManifestEntry,
    archive_path: &Path,
) -> Result<String, ServiceError> {
    let dll_path = local_dll_path(entry)?;

    if let Some(artifact_id) = reuse_local_library(context, entry, &dll_path)? {
        return Ok(artifact_id);
    }

    extract_and_register_library(context, entry, archive_path, &dll_path)
}

fn reuse_local_library(
    context: &crate::Context,
    entry: &LibraryManifestEntry,
    dll_path: &Path,
) -> Result<Option<String>, ServiceError> {
    if !dll_path.exists() {
        return Ok(None);
    }

    let metadata = std::fs::metadata(dll_path)
        .map_err(|error| library_error(format!("failed to read dll metadata: {error}")))?;

    if metadata.len() != entry.files.dll.size_bytes {
        return Ok(None);
    }

    let dll_sha256 = read_or_compute_dll_sha256(dll_path)?;
    register_local_artifact(context, entry, dll_path, &dll_sha256).map(Some)
}

fn extract_and_register_library(
    context: &crate::Context,
    entry: &LibraryManifestEntry,
    archive_path: &Path,
    dll_path: &Path,
) -> Result<String, ServiceError> {
    let payload = storage::read_file(archive_path)?;
    let dll_bytes = extract_dll_from_archive(entry, &payload)?;
    let sha256 = hex::encode(Sha256::digest(&dll_bytes));

    storage::write_file_atomically(dll_path, &dll_bytes)?;
    storage::write_sha256_cache(dll_path, &sha256)?;

    // Free up space by deleting the downloaded ZIP now that we have the DLL
    let _ = storage::remove_file_if_exists(archive_path);

    register_local_artifact(context, entry, dll_path, &sha256)
}

fn read_or_compute_dll_sha256(dll_path: &Path) -> Result<String, ServiceError> {
    match storage::read_sha256_cache(dll_path)? {
        Some(cached) => Ok(cached),
        None => {
            let computed = hex::encode(Sha256::digest(&storage::read_file(dll_path)?));
            storage::write_sha256_cache(dll_path, &computed)?;
            Ok(computed)
        }
    }
}

fn register_local_artifact(
    context: &crate::Context,
    entry: &LibraryManifestEntry,
    dll_path: &Path,
    sha256: &str,
) -> Result<String, ServiceError> {
    let artifact = artifact_builder::build_manifest_artifact(entry, dll_path, sha256)?;
    let artifact_id = artifact.id().as_str().to_owned();

    context
        .storage()
        .upsert_artifact(&artifact)
        .map_err(ServiceError::from)?;

    Ok(artifact_id)
}

pub(super) fn delete_local_library(
    context: &crate::Context,
    entry: &LibraryManifestEntry,
) -> Result<(), ServiceError> {
    // Remove the cached ZIP archive (may already be absent after a prior extraction).
    let archive_path = local_archive_path(entry)?;
    storage::remove_file_if_exists(&archive_path)?;

    let dll_path = local_dll_path(entry)?;
    if dll_path.exists() {
        // Best-effort: unregister the artifact from the catalog before deleting the file.
        if let Ok(sha256) = read_or_compute_dll_sha256(&dll_path) {
            if let Ok(artifact) =
                artifact_builder::build_manifest_artifact(entry, &dll_path, &sha256)
            {
                let _ = context.storage().delete_artifact(artifact.id());
            }
        }
        // Remove the DLL and its sidecar SHA-256 cache.
        storage::remove_file_if_exists(&dll_path)?;
        let _ = storage::remove_file_if_exists(&sha256_cache_path(&dll_path));
    }

    Ok(())
}

/// Returns the sidecar SHA-256 cache path for a given file, mirroring the
/// convention used by [`storage::write_sha256_cache`].
fn sha256_cache_path(path: &Path) -> PathBuf {
    path.with_extension(format!(
        "{}.sha256",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ))
}

async fn download_archive(entry: &LibraryManifestEntry) -> Result<Vec<u8>, ServiceError> {
    let client = http::http_client();
    http::download_exact_bytes(
        client,
        &entry.files.zip.download_url,
        entry.files.zip.size_bytes,
        "library download",
    )
    .await
}

fn archive_is_valid(path: &Path, entry: &LibraryManifestEntry) -> Result<bool, ServiceError> {
    if !path.exists() {
        return Ok(false);
    }

    let metadata = std::fs::metadata(path)
        .map_err(|error| library_error(format!("failed to read archive metadata: {error}")))?;
    Ok(metadata.len() == entry.files.zip.size_bytes)
}

fn extract_dll_from_archive(
    entry: &LibraryManifestEntry,
    payload: &[u8],
) -> Result<Vec<u8>, ServiceError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(payload)).map_err(|error| {
        library_error(format!(
            "invalid ZIP archive for `{}`: {error}",
            entry.entry_id
        ))
    })?;

    let dll_file_name = &entry.library.file_name;
    let mut dll_file = archive.by_name(dll_file_name).map_err(|error| {
        library_error(format!(
            "DLL `{dll_file_name}` not found in archive for `{}`: {error}",
            entry.entry_id
        ))
    })?;

    let mut dll_bytes = Vec::with_capacity(dll_file.size() as usize);
    dll_file.read_to_end(&mut dll_bytes).map_err(|error| {
        library_error(format!(
            "failed to read DLL `{dll_file_name}` from archive for `{}`: {error}",
            entry.entry_id
        ))
    })?;

    Ok(dll_bytes)
}

// ---------------------------------------------------------------------------
// FSR package materialization
// ---------------------------------------------------------------------------

/// Downloads and extracts a single member DLL to local storage **without**
/// registering a single-file artifact (FSR members are only offered as packages).
async fn ensure_member_dll(entry: &LibraryManifestEntry) -> Result<PathBuf, ServiceError> {
    let archive_path = local_archive_path(entry)?;
    ensure_local_archive(entry, &archive_path).await?;

    let dll_path = local_dll_path(entry)?;
    if dll_path.exists() {
        if let Ok(metadata) = std::fs::metadata(&dll_path) {
            if metadata.len() == entry.files.dll.size_bytes {
                return Ok(dll_path);
            }
        }
    }

    let payload = storage::read_file(&archive_path)?;
    let dll_bytes = extract_dll_from_archive(entry, &payload)?;
    let sha256 = hex::encode(Sha256::digest(&dll_bytes));
    storage::write_file_atomically(&dll_path, &dll_bytes)?;
    storage::write_sha256_cache(&dll_path, &sha256)?;

    // Free up space by deleting the downloaded ZIP now that we have the DLL
    let _ = storage::remove_file_if_exists(&archive_path);

    Ok(dll_path)
}

/// Materializes every member of an FSR package, then registers a single composed
/// `LibraryArtifact` whose files point at the local DLLs and carry their install
/// targets. Returns the registered package artifact id. All-or-nothing: the
/// package artifact is registered only after every member is on disk.
pub(super) async fn ensure_package_downloaded(
    context: &crate::Context,
    manifest: &LibraryManifest,
    package: &FsrPackage,
) -> Result<String, ServiceError> {
    let mut local_files = Vec::with_capacity(package.member_entry_ids.len());

    for (index, entry_id) in package.member_entry_ids.iter().enumerate() {
        let entry = super::manifest::require_entry(manifest, entry_id)?;
        let dll_path = ensure_member_dll(entry).await?;

        // Mirror the virtual package file (sha / version / install_as) but point
        // it at the local DLL.
        let template = &package.artifact.files()[index];
        let path = PathRef::new(dll_path.to_string_lossy().as_ref())
            .map_err(|error| library_error(format!("invalid member dll path: {error}")))?;
        let mut file = ComponentFile::new(path);
        if let Some(sha256) = template.sha256() {
            file = file.with_sha256(sha256.clone());
        }
        if let Some(version) = template.version() {
            file = file.with_version(version.clone());
        }
        if let Some(install_as) = template.install_as() {
            file = file.with_install_as(install_as);
        }
        local_files.push(file);
    }

    let artifact = LibraryArtifact::new(
        package.artifact.id().clone(),
        GraphicsTechnology::AmdFsr,
        package.artifact.file_name(),
        local_files,
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .map_err(|error| library_error(format!("failed to build FSR package artifact: {error}")))?
    .with_source("manifest-download")
    .map_err(|error| library_error(format!("failed to attach package source: {error}")))?;

    let artifact_id = artifact.id().as_str().to_owned();
    context
        .storage()
        .upsert_artifact(&artifact)
        .map_err(ServiceError::from)?;

    Ok(artifact_id)
}

// ---------------------------------------------------------------------------
// Group-key mapping
// ---------------------------------------------------------------------------

pub(crate) fn library_id_to_group_key(library_id: &str) -> &'static str {
    match library_id {
        "nvngx_dlss" => "dlss",
        "nvngx_dlssg" => "dlss_g",
        "nvngx_dlssd" => "dlss_d",
        "amd_fidelityfx_dx12" => "fsr_31_dx12",
        "amd_fidelityfx_vk" => "fsr_31_vk",
        "amd_fidelityfx_loader_dx12" => "fsr_loader_dx12",
        "amd_fidelityfx_upscaler_dx12" => "fsr_upscaler_dx12",
        "amd_fidelityfx_framegeneration_dx12" => "fsr_framegeneration_dx12",
        "amd_fidelityfx_denoiser_dx12" => "fsr_denoiser_dx12",
        "amd_fidelityfx_radiancecache_dx12" => "fsr_radiancecache_dx12",
        "libxell" => "xell",
        "libxess" => "xess",
        "libxess_dx11" => "xess_dx11",
        "libxess_fg" => "xess_fg",
        _ => "other",
    }
}

// ---------------------------------------------------------------------------
// Library state helpers
// ---------------------------------------------------------------------------

pub(super) fn library_state(
    entry: &LibraryManifestEntry,
    is_downloaded: bool,
    local_path: Option<&Path>,
    artifact_id: Option<String>,
) -> LibraryState {
    LibraryState {
        id: entry.entry_id.clone(),
        version: entry.version.value.clone(),
        is_downloaded,
        local_path: local_path.map(|path| path.to_string_lossy().into_owned()),
        artifact_id,
    }
}

pub(super) fn library_states(
    entries: &[LibraryManifestEntry],
) -> Result<Vec<LibraryState>, ServiceError> {
    let mut states = Vec::with_capacity(entries.len());

    for entry in entries {
        let dll_path = local_dll_path(entry)?;
        let is_downloaded = dll_path.exists();
        let local_path = is_downloaded.then_some(dll_path.as_path());

        states.push(library_state(entry, is_downloaded, local_path, None));
    }

    Ok(states)
}
