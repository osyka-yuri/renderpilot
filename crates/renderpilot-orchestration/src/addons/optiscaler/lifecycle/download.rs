//! Transport, cache, and archive staging for lifecycle commands.

use super::*;

pub(super) async fn download_and_stage(
    release: &OptiScalerRelease,
    modules: &HashSet<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedArchive, ServiceError> {
    let descriptor =
        super::super::artifact_descriptor::VerifiedArtifactDescriptor::release(release)?;
    let cache_path = descriptor.cache_path;
    crate::addons::progress::emit_indeterminate(progress, super::super::PHASE_VERIFYING);
    match load_cached_archive_off_runtime(cache_path.clone(), release, modules.clone()).await {
        Ok(Some(archive)) => return Ok(archive),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                "discarding invalid OptiScaler archive cache {}: {error}",
                cache_path.display()
            );
            remove_cached_file_off_runtime(cache_path.clone()).await?;
        }
    }
    let validated = crate::net::download_exact_with_url_chain(
        descriptor.source_url.as_str(),
        descriptor.size,
        "OptiScaler archive",
        progress,
    )
    .await?;
    super::super::source::validate_download_chain(&descriptor.source_url, &validated.url_chain)?;
    let bytes = validated.bytes;
    crate::addons::progress::emit_indeterminate(progress, super::super::PHASE_VERIFYING);
    let (bytes, archive) =
        validate_downloaded_archive_off_runtime(bytes, release, modules.clone()).await?;
    write_cached_file_off_runtime(cache_path, bytes).await?;
    Ok(archive)
}

/// Loads only the previous release's immutable INI base. The compressed archive
/// digest and member digest are still verified, while the unrelated solid DLL
/// block is not inflated a second time during a release transition.
pub(super) async fn download_config_base(
    release: &OptiScalerRelease,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Vec<u8>, ServiceError> {
    let archive_path = release
        .members
        .iter()
        .find(|member| member.target == "OptiScaler.ini")
        .map(|member| member.archive_path.clone())
        .ok_or_else(|| failed("installed release has no OptiScaler.ini"))?;
    let descriptor =
        super::super::artifact_descriptor::VerifiedArtifactDescriptor::release(release)?;
    let cache_path = descriptor.cache_path;
    crate::addons::progress::emit_indeterminate(progress, super::super::PHASE_VERIFYING);
    match load_cached_config_off_runtime(cache_path.clone(), release, archive_path.clone()).await {
        Ok(Some(config)) => return Ok(config),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                "discarding invalid OptiScaler archive cache {}: {error}",
                cache_path.display()
            );
            remove_cached_file_off_runtime(cache_path.clone()).await?;
        }
    }

    let validated = crate::net::download_exact_with_url_chain(
        descriptor.source_url.as_str(),
        descriptor.size,
        "OptiScaler archive",
        progress,
    )
    .await?;
    super::super::source::validate_download_chain(&descriptor.source_url, &validated.url_chain)?;
    crate::addons::progress::emit_indeterminate(progress, super::super::PHASE_VERIFYING);
    let (bytes, config) =
        read_downloaded_config_off_runtime(validated.bytes, release, archive_path).await?;
    write_cached_file_off_runtime(cache_path, bytes).await?;
    Ok(config)
}

async fn load_cached_archive_off_runtime(
    cache_path: PathBuf,
    release: &OptiScalerRelease,
    modules: HashSet<String>,
) -> Result<Option<PreparedArchive>, ServiceError> {
    let release = release.clone();
    tokio::task::spawn_blocking(move || match std::fs::read(&cache_path) {
        Ok(bytes) => validate_and_stage(&bytes, &release, &modules).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failed(format!(
            "failed to read OptiScaler archive cache {}: {error}",
            cache_path.display()
        ))),
    })
    .await
    .map_err(|error| failed(format!("OptiScaler archive cache task failed: {error}")))?
}

async fn load_cached_config_off_runtime(
    cache_path: PathBuf,
    release: &OptiScalerRelease,
    archive_path: String,
) -> Result<Option<Vec<u8>>, ServiceError> {
    let release = release.clone();
    tokio::task::spawn_blocking(move || match std::fs::read(&cache_path) {
        Ok(bytes) => read_verified_member(&bytes, &release, &archive_path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failed(format!(
            "failed to read OptiScaler archive cache {}: {error}",
            cache_path.display()
        ))),
    })
    .await
    .map_err(|error| failed(format!("OptiScaler config cache task failed: {error}")))?
}

async fn write_cached_file_off_runtime(
    cache_path: PathBuf,
    bytes: Vec<u8>,
) -> Result<(), ServiceError> {
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                failed(format!(
                    "failed to create OptiScaler archive cache {}: {error}",
                    parent.display()
                ))
            })?;
        }
        crate::fs::write_file_atomically(&cache_path, &bytes)
    })
    .await
    .map_err(|error| {
        failed(format!(
            "OptiScaler archive cache write task failed: {error}"
        ))
    })?
}

async fn remove_cached_file_off_runtime(cache_path: PathBuf) -> Result<(), ServiceError> {
    tokio::task::spawn_blocking(move || match std::fs::remove_file(&cache_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failed(format!(
            "failed to remove invalid OptiScaler cache {}: {error}",
            cache_path.display()
        ))),
    })
    .await
    .map_err(|error| failed(format!("OptiScaler cache cleanup task failed: {error}")))?
}

async fn validate_downloaded_archive_off_runtime(
    bytes: Vec<u8>,
    release: &OptiScalerRelease,
    modules: HashSet<String>,
) -> Result<(Vec<u8>, PreparedArchive), ServiceError> {
    let release = release.clone();
    tokio::task::spawn_blocking(move || {
        let archive = validate_and_stage(&bytes, &release, &modules)?;
        Ok((bytes, archive))
    })
    .await
    .map_err(|error| {
        failed(format!(
            "OptiScaler archive validation task failed: {error}"
        ))
    })?
}

async fn read_downloaded_config_off_runtime(
    bytes: Vec<u8>,
    release: &OptiScalerRelease,
    archive_path: String,
) -> Result<(Vec<u8>, Vec<u8>), ServiceError> {
    let release = release.clone();
    tokio::task::spawn_blocking(move || {
        let config = read_verified_member(&bytes, &release, &archive_path)?;
        Ok((bytes, config))
    })
    .await
    .map_err(|error| failed(format!("OptiScaler config validation task failed: {error}")))?
}
