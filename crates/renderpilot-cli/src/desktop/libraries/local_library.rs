use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{catalog, CliError};
use renderpilot_application::ArtifactRepository;

use super::{
    artifact_builder, http, library_error, storage, types::LibraryManifestEntry, validate,
};

pub(super) struct DownloadedLibrary {
    pub(super) archive_path: PathBuf,
    pub(super) artifact_id: String,
}

pub(super) async fn ensure_downloaded_and_registered(
    entry: &LibraryManifestEntry,
) -> Result<DownloadedLibrary, CliError> {
    let archive_path = local_archive_path(entry)?;

    ensure_local_archive(entry, &archive_path).await?;

    let artifact_id = materialize_local_library(entry, &archive_path)?;

    Ok(DownloadedLibrary {
        archive_path,
        artifact_id,
    })
}

pub(super) fn local_archive_path(entry: &LibraryManifestEntry) -> Result<PathBuf, CliError> {
    storage::local_archive_path(
        library_id_to_group_key(&entry.library.id),
        &entry.archive_file_name(),
    )
}

fn local_dll_path(entry: &LibraryManifestEntry) -> Result<PathBuf, CliError> {
    storage::local_dll_path(
        library_id_to_group_key(&entry.library.id),
        &entry.entry_id,
        &entry.library.file_name,
    )
}

async fn ensure_local_archive(
    entry: &LibraryManifestEntry,
    archive_path: &Path,
) -> Result<(), CliError> {
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
    entry: &LibraryManifestEntry,
    archive_path: &Path,
) -> Result<String, CliError> {
    let dll_path = local_dll_path(entry)?;

    if let Some(artifact_id) = reuse_local_library(entry, &dll_path)? {
        return Ok(artifact_id);
    }

    extract_and_register_library(entry, archive_path, &dll_path)
}

fn reuse_local_library(
    entry: &LibraryManifestEntry,
    dll_path: &Path,
) -> Result<Option<String>, CliError> {
    if !dll_path.exists() {
        return Ok(None);
    }

    let metadata = std::fs::metadata(dll_path)
        .map_err(|error| library_error(format!("failed to read dll metadata: {error}")))?;

    if metadata.len() != entry.files.dll.size_bytes {
        return Ok(None);
    }

    let dll_sha256 = read_or_compute_dll_sha256(dll_path)?;
    register_local_artifact(entry, dll_path, &dll_sha256).map(Some)
}

fn extract_and_register_library(
    entry: &LibraryManifestEntry,
    archive_path: &Path,
    dll_path: &Path,
) -> Result<String, CliError> {
    let payload = storage::read_file(archive_path)?;
    let dll_bytes = extract_dll_from_archive(entry, &payload)?;
    let sha256 = hex::encode(Sha256::digest(&dll_bytes));

    storage::write_file_atomically(dll_path, &dll_bytes)?;
    storage::write_sha256_cache(dll_path, &sha256)?;

    register_local_artifact(entry, dll_path, &sha256)
}

fn read_or_compute_dll_sha256(dll_path: &Path) -> Result<String, CliError> {
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
    entry: &LibraryManifestEntry,
    dll_path: &Path,
    sha256: &str,
) -> Result<String, CliError> {
    let artifact = artifact_builder::build_manifest_artifact(entry, dll_path, sha256)?;
    let artifact_id = artifact.id().as_str().to_owned();
    let storage = catalog::open_catalog_storage()?;

    storage.upsert_artifact(&artifact).map_err(CliError::from)?;

    Ok(artifact_id)
}

async fn download_archive(entry: &LibraryManifestEntry) -> Result<Vec<u8>, CliError> {
    let client = http::http_client();
    http::download_exact_bytes(
        client,
        &entry.files.zip.download_url,
        entry.files.zip.size_bytes,
        "library download",
    )
    .await
}

fn archive_is_valid(path: &Path, entry: &LibraryManifestEntry) -> Result<bool, CliError> {
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
) -> Result<Vec<u8>, CliError> {
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

use super::types::LibraryState;

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
) -> Result<Vec<LibraryState>, CliError> {
    let mut states = Vec::with_capacity(entries.len());

    for entry in entries {
        let archive_path = local_archive_path(entry)?;
        let is_downloaded = archive_path.exists();
        let local_path = is_downloaded.then_some(archive_path.as_path());

        states.push(library_state(entry, is_downloaded, local_path, None));
    }

    Ok(states)
}
