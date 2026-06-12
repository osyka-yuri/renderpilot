use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::ServiceError;
use renderpilot_application::ArtifactRepository;
use renderpilot_domain::{
    ArtifactTrustLevel, ComponentFile, GraphicsTechnology, LibraryArtifact, PathRef,
};

use super::{
    artifact_builder, compression,
    fsr_packages::FsrPackage,
    http, library_error, storage,
    types::{LibraryManifest, LibraryManifestEntry, LibraryState},
    validate,
};

pub(super) struct DecompressedArtifact {
    pub(super) bytes: Vec<u8>,
    pub(super) sha256: String,
}

impl DecompressedArtifact {
    /// Decompresses the archive payload and verifies the DLL bytes against
    /// the SHA-256 declared in the manifest.
    ///
    /// This is the only way to obtain a [`DecompressedArtifact`], so every
    /// DLL — freshly downloaded or read back from the archive cache — is
    /// hash-verified before it is written anywhere. `sha256` is the verified
    /// manifest hash (already lowercase), so no extra hashing pass is needed.
    pub(super) fn decompress_and_verify(
        entry: &LibraryManifestEntry,
        payload: &[u8],
    ) -> Result<Self, ServiceError> {
        let bytes = compression::decompress_library(entry, payload)?;
        validate::validate_dll_hash(&entry.entry_id, &entry.files.dll.hashes.sha256, &bytes)?;

        Ok(Self {
            bytes,
            sha256: entry.files.dll.hashes.sha256.clone(),
        })
    }
}

pub(super) struct DownloadedLibrary {
    pub(super) dll_path: PathBuf,
    pub(super) artifact_id: String,
}

pub(super) async fn ensure_downloaded_and_registered(
    context: &crate::Context,
    entry: &LibraryManifestEntry,
    progress: Option<&super::ProgressObserver<'_>>,
) -> Result<DownloadedLibrary, ServiceError> {
    let archive_path = local_archive_path(entry)?;
    let dll_path = local_dll_path(entry)?;

    let maybe_artifact = ensure_local_archive(entry, &archive_path, progress).await?;

    let artifact_id = materialize_local_library(context, entry, &archive_path, maybe_artifact)?;

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

/// Ensures the compressed archive exists on disk and is valid.
///
/// Returns `None` if the cached archive is already present and valid (no
/// download needed).  Returns `Some(artifact)` when a fresh download was
/// required — the caller can use these pre-decompressed bytes directly,
/// avoiding a second decompression pass.
async fn ensure_local_archive(
    entry: &LibraryManifestEntry,
    archive_path: &Path,
    progress: Option<&super::ProgressObserver<'_>>,
) -> Result<Option<DecompressedArtifact>, ServiceError> {
    if archive_is_valid(archive_path, entry)? {
        if let Some(cb) = progress {
            let size = entry.files.zst.size_bytes;
            cb(super::DownloadProgress {
                downloaded_bytes: size,
                total_bytes: size,
            });
        }
        return Ok(None);
    }

    storage::remove_file_if_exists(archive_path)?;

    let payload = download_archive(entry, progress).await?;

    // Validate before writing to disk so we never persist a corrupted archive.
    validate::validate_compressed_size(entry, &payload)?;
    let artifact = DecompressedArtifact::decompress_and_verify(entry, &payload)?;

    storage::write_file_atomically(archive_path, &payload)?;

    Ok(Some(artifact))
}

fn materialize_local_library(
    context: &crate::Context,
    entry: &LibraryManifestEntry,
    archive_path: &Path,
    maybe_artifact: Option<DecompressedArtifact>,
) -> Result<String, ServiceError> {
    let dll_path = local_dll_path(entry)?;

    if let Some(artifact_id) = reuse_local_library(context, entry, &dll_path)? {
        return Ok(artifact_id);
    }

    extract_and_register_library(context, entry, archive_path, &dll_path, maybe_artifact)
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

/// Returns the artifact from pre-decompressed bytes (fresh download) or reads
/// and decompresses the cached archive from disk. Either way the DLL bytes
/// are hash-verified against the manifest.
fn decompress_or_reuse(
    entry: &LibraryManifestEntry,
    archive_path: &Path,
    maybe_artifact: Option<DecompressedArtifact>,
) -> Result<DecompressedArtifact, ServiceError> {
    match maybe_artifact {
        Some(artifact) => Ok(artifact),
        None => {
            let payload = storage::read_file(archive_path)?;
            DecompressedArtifact::decompress_and_verify(entry, &payload)
        }
    }
}

/// Writes the DLL and its SHA-256 sidecar cache, then frees up space by
/// deleting the cached archive (best-effort) now that we have the DLL.
fn persist_dll(
    artifact: &DecompressedArtifact,
    dll_path: &Path,
    archive_path: &Path,
) -> Result<(), ServiceError> {
    storage::write_file_atomically(dll_path, &artifact.bytes)?;
    storage::write_sha256_cache(dll_path, &artifact.sha256)?;

    let _ = storage::remove_file_if_exists(archive_path);

    Ok(())
}

fn extract_and_register_library(
    context: &crate::Context,
    entry: &LibraryManifestEntry,
    archive_path: &Path,
    dll_path: &Path,
    maybe_artifact: Option<DecompressedArtifact>,
) -> Result<String, ServiceError> {
    let artifact = decompress_or_reuse(entry, archive_path, maybe_artifact)?;
    persist_dll(&artifact, dll_path, archive_path)?;

    register_local_artifact(context, entry, dll_path, &artifact.sha256)
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
    // Remove the cached archive (may already be absent after a prior extraction).
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

async fn download_archive(
    entry: &LibraryManifestEntry,
    progress: Option<&super::ProgressObserver<'_>>,
) -> Result<Vec<u8>, ServiceError> {
    let client = http::http_client();
    http::download_exact_bytes(
        client,
        &entry.files.zst.download_url,
        entry.files.zst.size_bytes,
        "library download",
        progress,
    )
    .await
}

/// Returns `true` when the cached archive has the expected file size.
///
/// We check only the file size, not the content hash.  This relies on the
/// invariant that a content change is always accompanied by a size change
/// in the manifest.  If that invariant is ever violated, remove the cached
/// archive manually.
fn archive_is_valid(path: &Path, entry: &LibraryManifestEntry) -> Result<bool, ServiceError> {
    if !path.exists() {
        return Ok(false);
    }

    let metadata = std::fs::metadata(path)
        .map_err(|error| library_error(format!("failed to read archive metadata: {error}")))?;
    Ok(metadata.len() == entry.files.zst.size_bytes)
}

// ---------------------------------------------------------------------------
// FSR package materialization
// ---------------------------------------------------------------------------

/// Downloads and extracts a single member DLL to local storage **without**
/// registering a single-file artifact (FSR members are only offered as packages).
async fn ensure_member_dll(
    entry: &LibraryManifestEntry,
    progress: Option<&super::ProgressObserver<'_>>,
) -> Result<PathBuf, ServiceError> {
    let archive_path = local_archive_path(entry)?;
    let maybe_artifact = ensure_local_archive(entry, &archive_path, progress).await?;

    let dll_path = local_dll_path(entry)?;
    if dll_path.exists() {
        if let Ok(metadata) = std::fs::metadata(&dll_path) {
            if metadata.len() == entry.files.dll.size_bytes {
                return Ok(dll_path);
            }
        }
    }

    let artifact = decompress_or_reuse(entry, &archive_path, maybe_artifact)?;
    persist_dll(&artifact, &dll_path, &archive_path)?;

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
    progress: Option<&super::ProgressObserver<'_>>,
) -> Result<String, ServiceError> {
    // Collect members and total byte count in a single pass — no duplicate
    // `require_entry` calls, no extra allocations.
    let members: Vec<&LibraryManifestEntry> = package
        .member_entry_ids
        .iter()
        .map(|id| super::manifest::require_entry(manifest, id))
        .collect::<Result<_, _>>()?;

    let total_bytes: u64 = members.iter().map(|e| e.files.zst.size_bytes).sum();

    let mut local_files = Vec::with_capacity(members.len());
    let mut cumulative_downloaded: u64 = 0;

    for (index, entry) in members.iter().enumerate() {
        let offset = cumulative_downloaded;

        // Build a per-member closure that translates member-local bytes into
        // cumulative package bytes — no `Box` allocation needed.
        let member_progress_fn = progress.map(|cb| {
            move |p: super::DownloadProgress| {
                cb(super::DownloadProgress {
                    downloaded_bytes: offset + p.downloaded_bytes,
                    total_bytes,
                });
            }
        });
        let member_progress: Option<&super::ProgressObserver<'_>> = member_progress_fn
            .as_ref()
            .map(|f| f as &super::ProgressObserver<'_>);

        let dll_path = ensure_member_dll(entry, member_progress).await?;
        cumulative_downloaded += entry.files.zst.size_bytes;

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
