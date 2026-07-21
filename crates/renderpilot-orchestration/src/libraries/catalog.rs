//! Atomic activation and last-known-good caching of catalog v1 snapshots.

use renderpilot_domain::ArtifactId;

use crate::ServiceError;
use crate::cdn;

use super::resolved::{ResolvedPackage, ValidatedCatalog};
use super::storage::LibraryStorage;
use super::types::{LibraryCatalog, LibraryIndex, LibraryVendorCatalog, LibraryVendorSnapshot};
use super::{library_error, validate};

const INDEX_KEY: &str = "libraries/v1/index.json";
const MAX_PRESET_SIZE: u64 = 2 * 1024 * 1024;

fn preset_urls() -> [String; 3] {
    [
        cdn::cdn_url("dlss_presets.json"),
        cdn::cdn_url("dlss_g_presets.json"),
        cdn::cdn_url("dlss_d_presets.json"),
    ]
}

fn parse_index(bytes: &[u8]) -> Result<LibraryIndex, ServiceError> {
    let index = serde_json::from_slice::<LibraryIndex>(crate::fs::strip_utf8_bom(bytes))
        .map_err(|error| library_error(format!("failed to parse library index: {error}")))?;
    validate::validate_index(&index)?;
    Ok(index)
}

fn parse_vendor_snapshot(
    bytes: &[u8],
    reference: &super::types::LibraryVendorReference,
) -> Result<LibraryVendorSnapshot, ServiceError> {
    validate::validate_exact_document(
        &format!("vendor snapshot `{}`", reference.vendor_id),
        reference.snapshot_size_bytes,
        &reference.snapshot_sha256,
        bytes,
    )?;
    let snapshot = serde_json::from_slice::<LibraryVendorSnapshot>(bytes).map_err(|error| {
        library_error(format!(
            "failed to parse vendor snapshot `{}`: {error}",
            reference.vendor_id
        ))
    })?;
    validate::validate_vendor_snapshot_envelope(&snapshot, reference)?;
    Ok(snapshot)
}

pub(super) fn parse_catalog(bytes: &[u8]) -> Result<ValidatedCatalog, ServiceError> {
    let catalog = serde_json::from_slice::<LibraryCatalog>(crate::fs::strip_utf8_bom(bytes))
        .map_err(|error| {
            library_error(format!("failed to parse cached library catalog: {error}"))
        })?;
    ValidatedCatalog::new(catalog)
}

async fn fetch_remote_catalog() -> Result<ValidatedCatalog, ServiceError> {
    let index_bytes = crate::net::download_limited_bytes(
        &cdn::cdn_url(INDEX_KEY),
        validate::MAX_INDEX_SIZE,
        "library index fetch",
    )
    .await?;
    let index = parse_index(&index_bytes)?;
    let mut vendors = Vec::new();

    let (supported, unsupported) =
        partition_vendor_references(&index, validate::is_supported_vendor);
    for reference in unsupported {
        log::warn!(
            "library index vendor `{}` is not supported by this client; skipping it",
            reference.vendor_id
        );
    }
    for reference in supported {
        let bytes = crate::net::download_exact_bytes(
            &cdn::cdn_url(&reference.snapshot_key),
            reference.snapshot_size_bytes,
            "vendor snapshot fetch",
            None,
        )
        .await?;
        let snapshot = parse_vendor_snapshot(&bytes, reference)?;
        vendors.push(LibraryVendorCatalog {
            vendor: snapshot.vendor,
            generated_at: snapshot.generated_at,
            artifacts: snapshot.artifacts,
            packages: snapshot.packages,
        });
    }

    let catalog = LibraryCatalog {
        schema_version: index.schema_version,
        generated_at: index.generated_at,
        vendors,
    };
    ValidatedCatalog::new(catalog)
}

pub(super) fn partition_vendor_references<F>(
    index: &LibraryIndex,
    mut is_supported: F,
) -> (
    Vec<&super::types::LibraryVendorReference>,
    Vec<&super::types::LibraryVendorReference>,
)
where
    F: FnMut(&str) -> bool,
{
    index
        .vendors
        .iter()
        .partition(|reference| is_supported(&reference.vendor_id))
}

pub(super) fn save_catalog(
    storage: &LibraryStorage,
    catalog: &ValidatedCatalog,
) -> Result<(), ServiceError> {
    let mut bytes = serde_json::to_vec_pretty(catalog.as_catalog())
        .map_err(|error| library_error(format!("failed to serialize library catalog: {error}")))?;
    bytes.push(b'\n');
    crate::fs::write_file_atomically(&storage.catalog_cache_path(), &bytes)
}

#[derive(Debug)]
pub(super) struct CatalogRefresh {
    pub(super) catalog: ValidatedCatalog,
    pub(super) activated: bool,
}

pub(super) fn complete_refresh<F>(
    storage: &LibraryStorage,
    remote: Result<ValidatedCatalog, ServiceError>,
    activate: F,
) -> Result<CatalogRefresh, ServiceError>
where
    F: FnOnce(&LibraryStorage, &ValidatedCatalog) -> Result<(), ServiceError>,
{
    match remote {
        Ok(catalog) => match activate(storage, &catalog) {
            Ok(()) => Ok(CatalogRefresh {
                catalog,
                activated: true,
            }),
            Err(error) => last_known_good_after_failure(storage, error),
        },
        Err(error) => last_known_good_after_failure(storage, error),
    }
}

/// Fetches and atomically activates a complete catalog. On remote failure, a
/// fully validated last-known-good snapshot is returned without mutation.
pub(super) async fn fetch_validated_catalog() -> Result<ValidatedCatalog, ServiceError> {
    let storage = LibraryStorage::discover()?;
    storage.ensure_content_layout_v1()?;
    let refresh = complete_refresh(&storage, fetch_remote_catalog().await, save_catalog)?;
    if refresh.activated {
        download_presets_best_effort(&storage).await;
    }
    Ok(refresh.catalog)
}

fn last_known_good_after_failure(
    storage: &LibraryStorage,
    refresh_error: ServiceError,
) -> Result<CatalogRefresh, ServiceError> {
    let cached = load_local_catalog_from(storage);
    let cache_is_invalid = cached.is_err();
    let result = match cached {
        Ok(Some(catalog)) => {
            log::warn!(
                "library catalog refresh failed ({refresh_error}); using last-known-good snapshot"
            );
            Ok(CatalogRefresh {
                catalog,
                activated: false,
            })
        }
        Ok(None) => Err(refresh_error),
        Err(cache_error) => {
            log::warn!(
                "library catalog refresh failed ({refresh_error}) and cached snapshot is invalid ({cache_error})"
            );
            Err(refresh_error)
        }
    };
    if cache_is_invalid {
        quarantine_cached_catalog(storage);
    }
    result
}

async fn download_presets_best_effort(storage: &LibraryStorage) {
    for url in preset_urls() {
        if let Err(error) = download_and_save_preset(storage, &url).await {
            log::warn!("failed to download preset manifest {url}: {error}");
        }
    }
}

async fn download_and_save_preset(storage: &LibraryStorage, url: &str) -> Result<(), ServiceError> {
    let bytes = crate::net::download_limited_bytes(url, MAX_PRESET_SIZE, "preset fetch").await?;
    if let Some(file_name) = url.split('/').next_back() {
        crate::fs::write_file_atomically(
            &storage.local_dlss_document_path(file_name)?,
            crate::fs::strip_utf8_bom(&bytes),
        )?;
    }
    Ok(())
}

pub(super) async fn get_or_fetch_validated_catalog() -> Result<ValidatedCatalog, ServiceError> {
    let storage = LibraryStorage::discover()?;
    storage.ensure_content_layout_v1()?;
    match load_local_catalog_from(&storage) {
        Ok(Some(catalog)) => return Ok(catalog),
        Ok(None) => {}
        Err(error) => {
            log::warn!("cached library catalog is invalid ({error}); refreshing it");
            quarantine_cached_catalog(&storage);
        }
    }
    fetch_validated_catalog().await
}

fn quarantine_cached_catalog(storage: &LibraryStorage) {
    crate::cdn::quarantine_at(&storage.catalog_cache_path());
}

pub(super) fn require_local_catalog() -> Result<ValidatedCatalog, ServiceError> {
    let storage = LibraryStorage::discover()?;
    load_local_catalog_from(&storage)?.ok_or_else(|| {
        library_error("library catalog is not loaded; fetch the catalog before using packages")
    })
}

pub(super) fn load_local_catalog() -> Result<Option<ValidatedCatalog>, ServiceError> {
    let storage = LibraryStorage::discover()?;
    load_local_catalog_from(&storage)
}

fn load_local_catalog_from(
    storage: &LibraryStorage,
) -> Result<Option<ValidatedCatalog>, ServiceError> {
    let path = storage.catalog_cache_path();
    match crate::fs::read_file(&path) {
        Ok(bytes) => parse_catalog(&bytes).map(Some),
        Err(_error) if !path.exists() => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn require_package<'a>(
    catalog: &'a ValidatedCatalog,
    package_id: &str,
) -> Result<ResolvedPackage<'a>, ServiceError> {
    catalog.package(package_id)
}

pub(super) fn require_package_by_artifact_id<'a>(
    catalog: &'a ValidatedCatalog,
    artifact_id: &ArtifactId,
) -> Result<ResolvedPackage<'a>, ServiceError> {
    catalog.package_by_artifact_id(artifact_id)
}
