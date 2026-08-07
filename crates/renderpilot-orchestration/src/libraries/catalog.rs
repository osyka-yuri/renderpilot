//! Atomic activation and last-known-good caching of catalog v1 snapshots.

use renderpilot_domain::ArtifactId;
use std::time::UNIX_EPOCH;

use crate::ServiceError;
use crate::cdn;

use super::library_error;
use super::resolved::{ResolvedPackage, ValidatedCatalog};
use super::storage::LibraryStorage;
use super::types::{LibraryCatalog, LibraryIndex, LibraryVendorCatalog, LibraryVendorSnapshot};

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
    super::validation::validate_index(&index)?;
    Ok(index)
}

fn parse_vendor_snapshot(
    bytes: &[u8],
    reference: &super::types::LibraryVendorReference,
) -> Result<LibraryVendorSnapshot, ServiceError> {
    super::validation::validate_exact_document(
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
    super::validation::validate_vendor_snapshot_envelope(&snapshot, reference)?;
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
        super::validation::MAX_INDEX_SIZE,
        "library index fetch",
    )
    .await?;
    let index = parse_index(&index_bytes)?;
    let mut vendors = Vec::new();

    let (supported, unsupported) =
        partition_vendor_references(&index, super::validation::is_supported_vendor);
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
            legal_documents: snapshot.legal_documents,
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

#[cfg(test)]
pub(super) fn save_catalog(
    storage: &LibraryStorage,
    catalog: &ValidatedCatalog,
) -> Result<bool, ServiceError> {
    let observed = observe_catalog(storage)?;
    Ok(commit_catalog(storage, observed.generation(), catalog)?.published())
}

pub(super) fn observe_catalog(
    storage: &LibraryStorage,
) -> Result<crate::fs::CacheObservation<ValidatedCatalog>, ServiceError> {
    let path = storage.catalog_cache_path();
    crate::fs::observe_cache_file(&path, |bytes, _metadata| parse_catalog(bytes))
}

pub(super) fn commit_catalog(
    storage: &LibraryStorage,
    observed_generation: &crate::fs::CacheGeneration,
    catalog: &ValidatedCatalog,
) -> Result<crate::fs::CachePublication<ValidatedCatalog>, ServiceError> {
    let mut bytes = serde_json::to_vec_pretty(catalog.as_catalog())
        .map_err(|error| library_error(format!("failed to serialize library catalog: {error}")))?;
    bytes.push(b'\n');
    let path = storage.catalog_cache_path();
    crate::fs::commit_cache_candidate(
        &path,
        observed_generation,
        &bytes,
        crate::fs::MatchingCurrentPolicy::PreserveCurrent,
        parse_catalog,
    )
}

#[derive(Debug)]
pub(super) struct CatalogRefresh {
    pub(super) catalog: ValidatedCatalog,
    pub(super) activated: bool,
    pub(super) changed: bool,
}

#[cfg(test)]
pub(super) fn complete_refresh<F>(
    storage: &LibraryStorage,
    remote: Result<ValidatedCatalog, ServiceError>,
    activate: F,
) -> Result<CatalogRefresh, ServiceError>
where
    F: FnOnce(&LibraryStorage, &ValidatedCatalog) -> Result<bool, ServiceError>,
{
    match remote {
        Ok(catalog) => match activate(storage, &catalog) {
            Ok(changed) => Ok(CatalogRefresh {
                catalog,
                activated: true,
                changed,
            }),
            Err(error) => last_known_good_after_failure(storage, error),
        },
        Err(error) => last_known_good_after_failure(storage, error),
    }
}

pub(super) async fn fetch_validated_catalog_refresh() -> Result<CatalogRefresh, ServiceError> {
    let storage = LibraryStorage::discover()?;
    storage.ensure_content_layout_v1()?;
    let observed = observe_catalog(&storage)?;
    fetch_catalog_after_observation(&storage, observed.generation().clone()).await
}

async fn fetch_catalog_after_observation(
    storage: &LibraryStorage,
    observed_generation: crate::fs::CacheGeneration,
) -> Result<CatalogRefresh, ServiceError> {
    let refresh =
        complete_observed_refresh(storage, &observed_generation, fetch_remote_catalog().await)?;
    if refresh.activated != refresh.changed {
        return Err(library_error(
            "catalog refresh activation state did not match its publication result",
        ));
    }
    if refresh.changed {
        download_presets_best_effort(storage).await;
    }
    Ok(refresh)
}

fn complete_observed_refresh(
    storage: &LibraryStorage,
    observed_generation: &crate::fs::CacheGeneration,
    remote: Result<ValidatedCatalog, ServiceError>,
) -> Result<CatalogRefresh, ServiceError> {
    match remote {
        Ok(catalog) => match commit_catalog(storage, observed_generation, &catalog) {
            Ok(publication) => catalog_refresh_from_publication(storage, catalog, publication),
            Err(error) => last_known_good_after_failure(storage, error),
        },
        Err(error) => last_known_good_after_failure(storage, error),
    }
}

fn catalog_refresh_from_publication(
    storage: &LibraryStorage,
    catalog: ValidatedCatalog,
    publication: crate::fs::CachePublication<ValidatedCatalog>,
) -> Result<CatalogRefresh, ServiceError> {
    match publication {
        crate::fs::CachePublication::Published => Ok(CatalogRefresh {
            catalog,
            activated: true,
            changed: true,
        }),
        crate::fs::CachePublication::Current(winner) => Ok(CatalogRefresh {
            catalog: winner,
            activated: false,
            changed: false,
        }),
        crate::fs::CachePublication::PreservedUnclassified => reobserve_preserved_catalog(storage),
    }
}

fn reobserve_preserved_catalog(storage: &LibraryStorage) -> Result<CatalogRefresh, ServiceError> {
    match observe_catalog(storage)? {
        crate::fs::CacheObservation::Valid { value: catalog, .. } => Ok(CatalogRefresh {
            catalog,
            activated: false,
            changed: false,
        }),
        crate::fs::CacheObservation::Absent { .. } => Err(library_error(
            "library catalog disappeared before a preserved refresh could be reobserved",
        )),
        crate::fs::CacheObservation::Invalid { error, .. } => Err(error),
    }
}

fn last_known_good_after_failure(
    storage: &LibraryStorage,
    refresh_error: ServiceError,
) -> Result<CatalogRefresh, ServiceError> {
    match observe_catalog(storage) {
        Ok(crate::fs::CacheObservation::Valid { value: catalog, .. }) => {
            log::warn!(
                "library catalog refresh failed ({refresh_error}); using last-known-good snapshot"
            );
            Ok(CatalogRefresh {
                catalog,
                activated: false,
                changed: false,
            })
        }
        Ok(crate::fs::CacheObservation::Absent { .. }) => Err(refresh_error),
        Ok(crate::fs::CacheObservation::Invalid {
            error: cache_error, ..
        })
        | Err(cache_error) => {
            log::warn!(
                "library catalog refresh failed ({refresh_error}) and cached snapshot is invalid ({cache_error})"
            );
            Err(refresh_error)
        }
    }
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
    let observed = observe_catalog(&storage)?;
    match observed {
        crate::fs::CacheObservation::Valid { value: catalog, .. } => Ok(catalog),
        crate::fs::CacheObservation::Absent { generation } => {
            Ok(fetch_catalog_after_observation(&storage, generation)
                .await?
                .catalog)
        }
        crate::fs::CacheObservation::Invalid { generation, error } => {
            log::warn!("cached library catalog is invalid ({error}); refreshing it");
            Ok(fetch_catalog_after_observation(&storage, generation)
                .await?
                .catalog)
        }
    }
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

pub(super) fn local_catalog_revision() -> Result<Option<(u64, u128)>, ServiceError> {
    let storage = LibraryStorage::discover()?;
    let path = storage.catalog_cache_path();
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(library_error(format!(
                "failed to inspect cached library catalog: {error}"
            )));
        }
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    Ok(Some((metadata.len(), modified)))
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
#[cfg(test)]
mod cache_publication_tests {
    use super::super::types::LibraryVendor;
    use super::*;

    fn catalog(generated_at: &str) -> ValidatedCatalog {
        let vendor = |id: &str, display_name: &str| LibraryVendorCatalog {
            vendor: LibraryVendor {
                id: id.to_owned(),
                display_name: display_name.to_owned(),
            },
            generated_at: generated_at.to_owned(),
            legal_documents: Vec::new(),
            artifacts: Vec::new(),
            packages: Vec::new(),
        };
        ValidatedCatalog::new(LibraryCatalog {
            schema_version: 1,
            generated_at: generated_at.to_owned(),
            vendors: vec![
                vendor("amd", "AMD"),
                vendor("intel", "Intel"),
                vendor("microsoft", "Microsoft"),
                vendor("nvidia", "NVIDIA"),
            ],
        })
        .expect("minimal supported-vendor catalog is valid")
    }

    fn storage() -> (tempfile::TempDir, LibraryStorage) {
        let directory = tempfile::tempdir().expect("catalog test storage");
        let storage = LibraryStorage::from_root(directory.path().join("libraries"));
        (directory, storage)
    }

    fn catalog_bytes(catalog: &ValidatedCatalog) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(catalog.as_catalog())
            .expect("serialize valid catalog fixture");
        bytes.push(b'\n');
        bytes
    }

    fn write_catalog(storage: &LibraryStorage, catalog: &ValidatedCatalog) {
        let bytes = catalog_bytes(catalog);
        crate::fs::write_file_atomically(&storage.catalog_cache_path(), &bytes)
            .expect("atomically install concurrent catalog fixture");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_catalog_refresh_reobserves_durable_current_with_honest_flags() {
        let (_directory, storage) = storage();
        let durable_a = catalog("2026-08-01T00:00:00Z");
        assert!(save_catalog(&storage, &durable_a).expect("publish durable catalog A"));
        let observed = observe_catalog(&storage).expect("observe durable catalog A");
        let remote_c = catalog("2026-08-03T00:00:00Z");

        let refresh = complete_observed_refresh(&storage, observed.generation(), Ok(remote_c))
            .expect("reobserve durable catalog A after unpublished C");

        assert!(!refresh.activated);
        assert!(!refresh.changed);
        assert_eq!(
            refresh.catalog.as_catalog().generated_at,
            "2026-08-01T00:00:00Z"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_catalog_refresh_reobserves_late_current_with_honest_flags() {
        let (_directory, storage) = storage();
        let durable_a = catalog("2026-08-01T00:00:00Z");
        assert!(save_catalog(&storage, &durable_a).expect("publish durable catalog A"));
        let observed = observe_catalog(&storage).expect("observe durable catalog A");
        let late_b = catalog("2026-08-02T00:00:00Z");
        write_catalog(&storage, &late_b);
        let remote_c = catalog("2026-08-03T00:00:00Z");

        let refresh = complete_observed_refresh(&storage, observed.generation(), Ok(remote_c))
            .expect("reobserve late durable catalog B after unpublished C");

        assert!(!refresh.activated);
        assert!(!refresh.changed);
        assert_eq!(
            refresh.catalog.as_catalog().generated_at,
            "2026-08-02T00:00:00Z"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_catalog_refresh_reobserve_rejects_absent_invalid_and_unreadable_cache() {
        let (_directory, storage) = storage();
        assert!(reobserve_preserved_catalog(&storage).is_err());

        let path = storage.catalog_cache_path();
        std::fs::create_dir_all(path.parent().expect("catalog cache parent"))
            .expect("create catalog cache parent");
        std::fs::write(&path, b"{invalid catalog").expect("write invalid catalog cache");
        assert!(reobserve_preserved_catalog(&storage).is_err());

        let _ = std::fs::remove_file(&path);
        std::fs::create_dir(&path).expect("create unreadable non-file cache path");
        assert!(reobserve_preserved_catalog(&storage).is_err());
    }

    #[test]
    fn catalog_refresh_published_candidate_reports_true_true() {
        let (_directory, storage) = storage();
        let observed = observe_catalog(&storage).expect("observe absent catalog cache");
        let remote = catalog("2026-08-03T00:00:00Z");
        let expected_bytes = catalog_bytes(&remote);

        let refresh = complete_observed_refresh(&storage, observed.generation(), Ok(remote))
            .expect("publish remote catalog");

        assert!(refresh.activated);
        assert!(refresh.changed);
        assert_eq!(
            refresh.catalog.as_catalog().generated_at,
            "2026-08-03T00:00:00Z"
        );
        let published_path = storage.catalog_cache_path();
        assert!(
            published_path.is_file(),
            "a Published catalog result must materialize the exact catalog path"
        );
        assert_eq!(
            std::fs::read(&published_path).expect("read published catalog bytes"),
            expected_bytes,
            "a Published catalog result must write the exact serialized candidate bytes"
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn catalog_refresh_current_winner_reports_false_false() {
        let (_directory, storage) = storage();
        let durable_a = catalog("2026-08-01T00:00:00Z");
        assert!(save_catalog(&storage, &durable_a).expect("publish durable catalog A"));
        let observed = observe_catalog(&storage).expect("observe durable catalog A");
        let current_b = catalog("2026-08-02T00:00:00Z");
        write_catalog(&storage, &current_b);
        let remote_c = catalog("2026-08-03T00:00:00Z");

        let refresh = complete_observed_refresh(&storage, observed.generation(), Ok(remote_c))
            .expect("retain current durable catalog B");

        assert!(!refresh.activated);
        assert!(!refresh.changed);
        assert_eq!(
            refresh.catalog.as_catalog().generated_at,
            "2026-08-02T00:00:00Z"
        );
    }
}
