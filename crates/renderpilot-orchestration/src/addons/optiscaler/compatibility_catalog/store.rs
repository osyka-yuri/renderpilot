//! Admission for revisable compatibility knowledge.
//!
//! This cache is deliberately independent from the immutable release cache.
//! A compatibility correction may be replaced, but never silently downgrades
//! the last admitted document or rewrites an equal revision with different
//! content.

use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use crate::cdn;
use crate::fs::{CacheObservation, CachePublication, MatchingCurrentPolicy};
use crate::{ServiceError, failed};

use super::model::{OptiScalerCompatibilityCatalog, WireCompatibilityCatalog, invalid_catalog};

const BUNDLED_CATALOG: &[u8] =
    include_bytes!("../../../../assets/optiscaler-compatibility-fallback.json");
const CACHE_FILE: &str = "optiscaler_compatibility_catalog_v1.json";
const MAX_CATALOG_BYTES: u64 = 2 * 1024 * 1024;
const CACHE_TTL: Duration = Duration::from_hours(24);

#[derive(Debug)]
struct CachedCatalog {
    catalog: OptiScalerCompatibilityCatalog,
    is_fresh: bool,
    bytes: Vec<u8>,
}

fn catalog_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Parses one compatibility document against the schema and validation rules.
pub(crate) fn parse_catalog(bytes: &[u8]) -> Result<OptiScalerCompatibilityCatalog, ServiceError> {
    let wire: WireCompatibilityCatalog =
        serde_json::from_slice(crate::fs::strip_utf8_bom(bytes))
            .map_err(|error| invalid_catalog(format!("invalid JSON: {error}")))?;
    wire.try_into()
}

/// Returns the bundled fallback compatibility catalog.
#[cfg(test)]
pub(crate) fn bundled_catalog() -> Result<OptiScalerCompatibilityCatalog, ServiceError> {
    parse_catalog(BUNDLED_CATALOG)
}

/// Returns the best independently admitted compatibility document.
pub(crate) async fn get_or_fetch_catalog() -> Result<OptiScalerCompatibilityCatalog, ServiceError> {
    let _single_flight = catalog_lock().lock().await;
    let path = crate::app_dir::app_dir()?.join(CACHE_FILE);
    get_or_fetch_catalog_from(&path, || async {
        crate::net::download_limited_bytes(
            &cdn::cdn_url("addons/v1/optiscaler-compatibility.json"),
            MAX_CATALOG_BYTES,
            "OptiScaler compatibility catalog fetch",
        )
        .await
    })
    .await
}

async fn get_or_fetch_catalog_from<F, Fut>(
    path: &Path,
    fetch: F,
) -> Result<OptiScalerCompatibilityCatalog, ServiceError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, ServiceError>>,
{
    let bundled = parse_catalog(BUNDLED_CATALOG)?;
    let observed = observe_cache(path)?;
    let (cached, admitted_bytes) = match &observed {
        CacheObservation::Valid { value, .. }
            if compare_admission(&value.catalog, &bundled).is_admissible() =>
        {
            if value.is_fresh {
                return Ok(value.catalog.clone());
            }
            (Some(&value.catalog), value.bytes.as_slice())
        }
        _ => (None, BUNDLED_CATALOG),
    };
    let admitted = cached.unwrap_or(&bundled);

    let candidate_bytes = match fetch().await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!("OptiScaler compatibility catalog refresh failed: {error}");
            return Ok(admitted.clone());
        }
    };
    let candidate = match parse_catalog(&candidate_bytes) {
        Ok(candidate) => candidate,
        Err(error) => {
            tracing::warn!("OptiScaler compatibility catalog rejected: {error}");
            return Ok(admitted.clone());
        }
    };

    match compare_admission(&candidate, admitted) {
        AdmissionOrder::Older => {
            if let Err(error) = refresh_cache_freshness(path, observed.generation(), admitted_bytes)
            {
                tracing::warn!(
                    "could not refresh OptiScaler compatibility catalog freshness: {error}"
                );
            }
            Ok(admitted.clone())
        }
        AdmissionOrder::Equal | AdmissionOrder::Newer => publish_candidate(
            path,
            observed.generation(),
            &candidate_bytes,
            &candidate,
            &bundled,
            admitted,
        ),
        AdmissionOrder::Divergent => {
            tracing::warn!(
                "OptiScaler compatibility catalog rejected: equal revision changes content"
            );
            Ok(admitted.clone())
        }
    }
}

fn refresh_cache_freshness(
    path: &Path,
    observed: &crate::fs::CacheGeneration,
    bytes: &[u8],
) -> Result<(), ServiceError> {
    crate::fs::commit_cache_candidate(
        path,
        observed,
        bytes,
        MatchingCurrentPolicy::RefreshCandidate,
        parse_catalog,
    )
    .map(|_| ())
}

fn observe_cache(path: &Path) -> Result<CacheObservation<CachedCatalog>, ServiceError> {
    crate::fs::observe_cache_file(path, |bytes, metadata| {
        if metadata.len() > MAX_CATALOG_BYTES {
            return Err(failed(
                "OptiScaler compatibility cache exceeds its size limit",
            ));
        }
        let catalog = parse_catalog(bytes)?;
        let is_fresh = metadata
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_none_or(|age| age <= CACHE_TTL);
        Ok(CachedCatalog {
            catalog,
            is_fresh,
            bytes: bytes.to_vec(),
        })
    })
}

fn publish_candidate(
    path: &Path,
    observed: &crate::fs::CacheGeneration,
    bytes: &[u8],
    candidate: &OptiScalerCompatibilityCatalog,
    bundled: &OptiScalerCompatibilityCatalog,
    fallback: &OptiScalerCompatibilityCatalog,
) -> Result<OptiScalerCompatibilityCatalog, ServiceError> {
    let publication = match crate::fs::commit_cache_candidate(
        path,
        observed,
        bytes,
        MatchingCurrentPolicy::RefreshCandidate,
        parse_catalog,
    ) {
        Ok(publication) => publication,
        Err(error) => {
            tracing::warn!("OptiScaler compatibility catalog publication failed: {error}");
            return Ok(fallback.clone());
        }
    };
    match publication {
        CachePublication::Published => Ok(candidate.clone()),
        CachePublication::Current(_) | CachePublication::PreservedUnclassified => {
            latest_admitted(path, bundled, fallback)
        }
    }
}

fn latest_admitted(
    path: &Path,
    bundled: &OptiScalerCompatibilityCatalog,
    fallback: &OptiScalerCompatibilityCatalog,
) -> Result<OptiScalerCompatibilityCatalog, ServiceError> {
    match observe_cache(path) {
        Ok(CacheObservation::Valid { value, .. }) if accepts(&value.catalog, bundled) => {
            Ok(value.catalog)
        }
        Ok(_) => Ok(fallback.clone()),
        Err(error) => {
            tracing::warn!("OptiScaler compatibility catalog re-observation failed: {error}");
            Ok(fallback.clone())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdmissionOrder {
    Older,
    Equal,
    Newer,
    Divergent,
}

impl AdmissionOrder {
    fn is_admissible(self) -> bool {
        !matches!(self, Self::Older | Self::Divergent)
    }
}

fn compare_admission(
    candidate: &OptiScalerCompatibilityCatalog,
    accepted: &OptiScalerCompatibilityCatalog,
) -> AdmissionOrder {
    let (Ok(candidate_key), Ok(accepted_key)) = (
        revision_key(&candidate.revision),
        revision_key(&accepted.revision),
    ) else {
        return AdmissionOrder::Divergent;
    };
    match candidate_key.cmp(&accepted_key) {
        std::cmp::Ordering::Less => AdmissionOrder::Older,
        std::cmp::Ordering::Greater => AdmissionOrder::Newer,
        std::cmp::Ordering::Equal if **candidate == **accepted => AdmissionOrder::Equal,
        std::cmp::Ordering::Equal => AdmissionOrder::Divergent,
    }
}

fn accepts(
    candidate: &OptiScalerCompatibilityCatalog,
    bundled: &OptiScalerCompatibilityCatalog,
) -> bool {
    compare_admission(candidate, bundled).is_admissible()
}

fn revision_key(value: &str) -> Result<Vec<u64>, ServiceError> {
    let normalized = value.replace('-', ".");
    let parts = normalized
        .split('.')
        .map(|part| {
            part.parse::<u64>()
                .map_err(|_| failed("invalid OptiScaler compatibility revision component"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parts.is_empty() {
        Err(failed("empty OptiScaler compatibility revision"))
    } else {
        Ok(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_parses_successfully() {
        let catalog = bundled_catalog().expect("bundled compatibility catalog");
        assert!(!catalog.entries().is_empty());
    }

    #[test]
    fn newer_revision_compares_as_newer() {
        let accepted = parse_catalog(BUNDLED_CATALOG).expect("catalog");
        let mut value: serde_json::Value =
            serde_json::from_slice(BUNDLED_CATALOG).expect("catalog JSON");
        value["revision"] = serde_json::Value::String("2026-09-16.1".to_owned());
        let candidate = parse_catalog(&serde_json::to_vec(&value).expect("serialize"))
            .expect("newer candidate");
        assert_eq!(
            compare_admission(&candidate, &accepted),
            AdmissionOrder::Newer
        );
    }

    #[test]
    fn catalog_accepts_new_game_with_new_message() {
        let mut value: serde_json::Value =
            serde_json::from_slice(BUNDLED_CATALOG).expect("catalog JSON");
        let mut new_entry = value["entries"][0].clone();
        new_entry["id"] = serde_json::Value::String("brand-new-game-2026".to_owned());
        new_entry["identities"] = serde_json::json!([
            { "kind": "steam_appid", "value": "9999999" }
        ]);
        new_entry["guidance"] = serde_json::json!([
            {
                "kind": "warning",
                "message": {
                    "id": "optiscaler-brand-new-message-unknown-to-binary",
                    "fallback_text": "A completely new reviewed message from CDN."
                }
            }
        ]);
        value["entries"].as_array_mut().unwrap().push(new_entry);
        let candidate = parse_catalog(&serde_json::to_vec(&value).expect("serialize"))
            .expect("catalog with new message");
        assert!(
            candidate
                .entries()
                .iter()
                .any(|e| e.id == "brand-new-game-2026")
        );
    }

    #[test]
    fn catalog_allows_shared_message_across_entries() {
        let mut value: serde_json::Value =
            serde_json::from_slice(BUNDLED_CATALOG).expect("catalog JSON");
        let shared_guidance = serde_json::json!([
            {
                "kind": "warning",
                "message": {
                    "id": "optiscaler-shared-universal-warning",
                    "fallback_text": "Universal warning shared across multiple games."
                }
            }
        ]);
        let mut entry_1 = value["entries"][0].clone();
        entry_1["id"] = serde_json::Value::String("game-one".to_owned());
        entry_1["identities"] = serde_json::json!([
            { "kind": "steam_appid", "value": "1111111" }
        ]);
        entry_1["guidance"] = shared_guidance.clone();

        let mut entry_2 = value["entries"][0].clone();
        entry_2["id"] = serde_json::Value::String("game-two".to_owned());
        entry_2["identities"] = serde_json::json!([
            { "kind": "steam_appid", "value": "2222222" }
        ]);
        entry_2["guidance"] = shared_guidance;

        value["entries"] = serde_json::json!([entry_1, entry_2]);
        let candidate = parse_catalog(&serde_json::to_vec(&value).expect("serialize"))
            .expect("catalog with shared guidance message");
        assert_eq!(candidate.entries().len(), 2);
    }

    #[test]
    fn conditional_variants_require_disjoint_exact_predicates() {
        let mut value: serde_json::Value =
            serde_json::from_slice(BUNDLED_CATALOG).expect("catalog");
        let default = value["entries"][0]["variants"][0].clone();
        let mut steam = default.clone();
        steam["when"] = serde_json::json!({ "launcher": "steam" });
        let mut executable = default;
        executable["when"] = serde_json::json!({ "executable": "Game.exe" });
        value["entries"][0]["variants"] = serde_json::json!([
            value["entries"][0]["variants"][0].clone(),
            steam,
            executable
        ]);
        assert!(parse_catalog(&serde_json::to_vec(&value).expect("serialize")).is_err());
    }

    #[test]
    fn producer_invalid_launcher_values_are_rejected() {
        for launcher in ["proton", "cross_over", "whisky"] {
            let mut value: serde_json::Value =
                serde_json::from_slice(BUNDLED_CATALOG).expect("catalog");
            let default = value["entries"][0]["variants"][0].clone();
            let mut conditional = default.clone();
            conditional["when"] = serde_json::json!({ "launcher": launcher });
            value["entries"][0]["variants"] = serde_json::json!([default, conditional]);

            assert!(
                parse_catalog(&serde_json::to_vec(&value).expect("serialize")).is_err(),
                "{launcher} must not be admitted as a compatibility launcher"
            );
        }
    }

    #[test]
    fn producer_nvngx_proxy_slot_is_admitted() {
        let mut value: serde_json::Value =
            serde_json::from_slice(BUNDLED_CATALOG).expect("catalog");
        value["entries"][0]["variants"][0]["proxy"] =
            serde_json::json!({ "kind": "exact", "slot": "nvngx.dll" });

        parse_catalog(&serde_json::to_vec(&value).expect("serialize"))
            .expect("nvngx.dll is an allowed exact proxy slot");
    }

    #[test]
    fn same_revision_content_change_is_not_admitted() {
        let accepted = parse_catalog(BUNDLED_CATALOG).expect("catalog");
        let mut value: serde_json::Value =
            serde_json::from_slice(BUNDLED_CATALOG).expect("catalog JSON");
        value["upstream"]["source"] =
            serde_json::Value::String("different-reviewed-source".to_owned());
        let candidate = parse_catalog(&serde_json::to_vec(&value).expect("serialize"))
            .expect("valid independently shaped candidate");
        assert_eq!(
            compare_admission(&candidate, &accepted),
            AdmissionOrder::Divergent
        );
    }

    #[cfg(not(target_os = "linux"))]
    fn age_file_for_test(path: &Path, age: Duration) {
        let modified = SystemTime::now()
            .checked_sub(age)
            .expect("representable timestamp");
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open cache")
            .set_modified(modified)
            .expect("set mtime");
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn stale_cache_equal_to_bundled_refreshes_newer_remote() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_file = temp.path().join(CACHE_FILE);

        let bundled = bundled_catalog().expect("bundled catalog");
        let r1_revision = bundled.revision.clone();

        std::fs::write(&cache_file, BUNDLED_CATALOG).expect("write initial cache");
        age_file_for_test(&cache_file, CACHE_TTL + Duration::from_hours(1));

        let mut r2_value: serde_json::Value =
            serde_json::from_slice(BUNDLED_CATALOG).expect("parse bundled json");
        let r2_revision = "2026-10-01.1";
        r2_value["revision"] = serde_json::Value::String(r2_revision.to_owned());
        let mut new_entry = r2_value["entries"][0].clone();
        new_entry["id"] = serde_json::Value::String("newly-supported-game".to_owned());
        new_entry["identities"] = serde_json::json!([
            { "kind": "steam_appid", "value": "12345678" }
        ]);
        r2_value["entries"].as_array_mut().unwrap().push(new_entry);
        let r2_bytes = serde_json::to_vec(&r2_value).expect("serialize r2");

        let result = get_or_fetch_catalog_from(&cache_file, || async { Ok(r2_bytes.clone()) })
            .await
            .expect("fetch catalog");

        assert_eq!(result.revision, r2_revision);
        assert_ne!(result.revision, r1_revision);
        assert!(
            result
                .entries()
                .iter()
                .any(|e| e.id == "newly-supported-game")
        );

        let cached_on_disk = std::fs::read(&cache_file).expect("read cache file");
        let parsed_cache = parse_catalog(&cached_on_disk).expect("parse cached catalog");
        assert_eq!(parsed_cache.revision, r2_revision);
        assert!(
            parsed_cache
                .entries()
                .iter()
                .any(|e| e.id == "newly-supported-game")
        );

        let second_result = get_or_fetch_catalog_from(&cache_file, || async {
            panic!("second call within TTL must not request remote");
        })
        .await
        .expect("second fetch");
        assert_eq!(second_result.revision, r2_revision);
    }

    #[tokio::test]
    async fn fresh_cache_equal_to_bundled_does_not_refetch_remote() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_file = temp.path().join(CACHE_FILE);

        let bundled = bundled_catalog().expect("bundled catalog");
        let r1_revision = bundled.revision.clone();

        std::fs::write(&cache_file, BUNDLED_CATALOG).expect("write initial cache");

        let result = get_or_fetch_catalog_from(&cache_file, || async {
            panic!("remote should not be called for a fresh valid cache");
        })
        .await
        .expect("fetch catalog");

        assert_eq!(result.revision, r1_revision);
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn remote_equal_to_admitted_marks_refresh_as_fresh() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_file = temp.path().join(CACHE_FILE);

        let bundled = bundled_catalog().expect("bundled catalog");
        let r1_revision = bundled.revision.clone();

        std::fs::write(&cache_file, BUNDLED_CATALOG).expect("write initial cache");
        age_file_for_test(&cache_file, CACHE_TTL + Duration::from_hours(1));

        let result1 =
            get_or_fetch_catalog_from(&cache_file, || async { Ok(BUNDLED_CATALOG.to_vec()) })
                .await
                .expect("first fetch");
        assert_eq!(result1.revision, r1_revision);

        let metadata = std::fs::metadata(&cache_file).expect("cache metadata");
        let age = SystemTime::now()
            .duration_since(metadata.modified().expect("modified"))
            .expect("duration");
        assert!(
            age <= Duration::from_secs(5),
            "cache file mtime must be updated to now"
        );

        let result2 = get_or_fetch_catalog_from(&cache_file, || async {
            panic!("second call must use fresh cache and not query remote");
        })
        .await
        .expect("second fetch");
        assert_eq!(result2.revision, r1_revision);
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn remote_older_than_admitted_does_not_refetch_until_refresh_ttl() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache_file = temp.path().join(CACHE_FILE);

        let mut r2_value: serde_json::Value =
            serde_json::from_slice(BUNDLED_CATALOG).expect("parse bundled json");
        let r2_revision = "2026-10-01.1";
        r2_value["revision"] = serde_json::Value::String(r2_revision.to_owned());
        let r2_bytes = serde_json::to_vec(&r2_value).expect("serialize r2");

        std::fs::write(&cache_file, &r2_bytes).expect("write r2 cache");
        age_file_for_test(&cache_file, CACHE_TTL + Duration::from_hours(1));

        let result1 =
            get_or_fetch_catalog_from(&cache_file, || async { Ok(BUNDLED_CATALOG.to_vec()) })
                .await
                .expect("first fetch");
        assert_eq!(
            result1.revision, r2_revision,
            "must preserve newer admitted revision"
        );

        let result2 = get_or_fetch_catalog_from(&cache_file, || async {
            panic!("second call must not query remote until TTL expires");
        })
        .await
        .expect("second fetch");
        assert_eq!(result2.revision, r2_revision);
    }
}
