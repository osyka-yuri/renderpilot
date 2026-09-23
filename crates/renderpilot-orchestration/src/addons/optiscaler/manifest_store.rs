//! Metadata CDN/cache resolution with a release-pinned stable fallback.
//!
//! The JSON catalogue is RenderPilot-owned metadata. OptiScaler archives are
//! never mirrored here; each release names a typed official upstream source
//! which is validated by the private upstream-source policy module.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};
use std::sync::OnceLock;
use std::time::Duration;

use crate::cdn::{self, CdnManifestSpec};
use crate::{ServiceError, failed};

use super::types::{OptiScalerManifest, WireOptiScalerManifest};

pub(super) mod validation;

const BUNDLED: &[u8] = include_bytes!("../../../assets/optiscaler-fallback.json");
const MAX_MANIFEST_ARCHIVE_SIZE: u64 = 128 * 1024 * 1024;
const MAX_MODULE_ARTIFACT_SIZE: u64 = 32 * 1024 * 1024;

fn manifest_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn spec() -> CdnManifestSpec {
    CdnManifestSpec {
        file_name: "optiscaler_manifest.json",
        url: cdn::cdn_url("addons/v1/optiscaler.json"),
        max_size_bytes: 8 * 1024 * 1024,
        ttl: Some(Duration::from_hours(24)),
    }
}

/// Parses and semantically validates a strict wire-v1 manifest.
pub fn parse_manifest(bytes: &[u8]) -> Result<OptiScalerManifest, ServiceError> {
    let wire: WireOptiScalerManifest = serde_json::from_slice(bytes)
        .map_err(|error| failed(format!("invalid OptiScaler manifest JSON: {error}")))?;
    wire.try_into()
}

/// Returns a validated remote manifest or the bundled stable fallback.
pub async fn get_or_fetch_manifest() -> Result<OptiScalerManifest, ServiceError> {
    let _single_flight = manifest_lock().lock().await;
    let bundled = parse_manifest(BUNDLED)?;
    match cdn::get_or_fetch(&spec(), parse_manifest).await {
        Ok(manifest) if validate_manifest_update(&manifest, &bundled).is_ok() => Ok(manifest),
        Ok(_) | Err(_) => {
            tracing::warn!(
                "OptiScaler manifest failed immutable-history validation; using bundled snapshot"
            );
            if let Err(cache_error) = cache_bundled_fallback().await {
                tracing::warn!(
                    "could not cache bundled OptiScaler manifest fallback: {cache_error}"
                );
            }
            Ok(bundled)
        }
    }
}

async fn cache_bundled_fallback() -> Result<(), ServiceError> {
    tokio::task::spawn_blocking(|| {
        let path = crate::app_dir::app_dir()?.join(spec().file_name);
        crate::fs::write_file_atomically(&path, BUNDLED)
    })
    .await
    .map_err(|error| {
        failed(format!(
            "OptiScaler manifest cache write task failed: {error}"
        ))
    })?
}

fn validate_manifest_update(
    candidate: &OptiScalerManifest,
    baseline: &OptiScalerManifest,
) -> Result<(), ServiceError> {
    let candidate_revision = revision_key(&candidate.revision)?;
    let baseline_revision = revision_key(&baseline.revision)?;
    if candidate_revision < baseline_revision {
        return Err(failed(format!(
            "OptiScaler manifest revision {} is older than accepted revision {}",
            candidate.revision, baseline.revision
        )));
    }
    if !preserves_release_history(candidate, baseline) {
        return Err(failed(
            "OptiScaler metadata removes or rewrites accepted immutable release history",
        ));
    }
    Ok(())
}

fn preserves_release_history(
    candidate: &OptiScalerManifest,
    baseline: &OptiScalerManifest,
) -> bool {
    baseline.releases.iter().all(|expected| {
        candidate
            .releases
            .iter()
            .find(|release| release.id == expected.id)
            == Some(expected)
    })
}

fn revision_key(value: &str) -> Result<Vec<u64>, ServiceError> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<u64>()
                .map_err(|_| failed(format!("manifest revision component is too large: {part}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled_value() -> serde_json::Value {
        serde_json::from_slice(BUNDLED).expect("bundled JSON")
    }

    fn rejects(value: &serde_json::Value) -> bool {
        parse_manifest(&serde_json::to_vec(value).expect("serialize test manifest")).is_err()
    }

    #[test]
    fn bundled_manifest_is_an_append_only_self_consistent_history() {
        let bundled = parse_manifest(BUNDLED).expect("bundled manifest");

        validate_manifest_update(&bundled, &bundled)
            .expect("bundled manifest must preserve immutable history");
    }

    #[test]
    fn bundled_manifest_is_strict_and_contains_the_supported_stable_history() {
        let manifest = parse_manifest(BUNDLED).expect("bundled manifest");
        assert!(
            manifest
                .modules
                .iter()
                .all(|module| module.id != "nvidia_rr")
        );
        assert_eq!(manifest.releases.len(), 5);
        assert!(
            manifest.releases[0]
                .members
                .iter()
                .any(|m| m.target == "libxess.dll")
        );
        for stable in ["v0.9.0", "v0.9.1", "v0.9.2", "v0.9.3", "v0.9.4"] {
            assert!(
                manifest.releases.iter().any(|release| release.id == stable),
                "the complete supported stable history must retain {stable}"
            );
        }
        assert_eq!(
            manifest.current_release().expect("current release").id,
            "v0.9.4"
        );
        assert!(
            manifest
                .modules
                .iter()
                .all(|module| module.id != "metadata"),
            "archive metadata must not be represented as an installable module"
        );
        let optipatcher = manifest
            .modules
            .iter()
            .find(|module| module.id == "optipatcher")
            .expect("pinned OptiPatcher module");
        let artifact = optipatcher.artifact.as_ref().expect("OptiPatcher artifact");
        assert_eq!(artifact.id, "v0.41");
        assert_eq!(artifact.source.tag, "v0.41");
        assert_ne!(artifact.source.tag, "rolling");
    }

    #[test]
    fn bundled_manifest_uses_only_the_current_public_module_contract() {
        let value = bundled_value();
        assert!(
            value["modules"]
                .as_array()
                .expect("modules")
                .iter()
                .all(|module| module.get("releases").is_none())
        );
        assert!(
            value["releases"]
                .as_array()
                .expect("releases")
                .iter()
                .flat_map(|release| release["members"].as_array().expect("members"))
                .all(|member| member["target"].is_string())
        );
    }

    #[test]
    fn parser_rejects_obsolete_or_schema_invalid_module_fields() {
        let mut obsolete_releases = bundled_value();
        obsolete_releases["modules"][0]["releases"] = serde_json::json!(["v0.9.4"]);
        assert!(rejects(&obsolete_releases));

        let mut empty_requires = bundled_value();
        empty_requires["modules"][0]["requires"] = serde_json::json!([]);
        assert!(!rejects(&empty_requires));

        let mut missing_artifact_architecture = bundled_value();
        let artifact_module = missing_artifact_architecture["modules"]
            .as_array_mut()
            .expect("modules")
            .iter_mut()
            .find(|module| module.get("artifact").is_some())
            .expect("artifact module");
        artifact_module["artifact"]
            .as_object_mut()
            .expect("artifact")
            .remove("pe_x64");
        assert!(rejects(&missing_artifact_architecture));

        let mut null_target = bundled_value();
        null_target["releases"][0]["members"][0]["target"] = serde_json::Value::Null;
        assert!(rejects(&null_target));
    }

    #[test]
    fn numeric_manifest_revision_order_does_not_use_lexicographic_patch_order() {
        assert!(
            revision_key("2026-07-19.10").expect("valid revision")
                > revision_key("2026-07-19.3").expect("valid revision")
        );
        assert!(
            revision_key("2026-07-20.1").expect("valid revision")
                > revision_key("2026-07-19.99").expect("valid revision")
        );
    }

    #[test]
    fn manifest_revision_component_overflow_fails_closed() {
        assert!(revision_key("2026-07-19.18446744073709551616").is_err());
    }

    #[test]
    fn remote_metadata_cannot_remove_or_rewrite_bundled_release_history() {
        let baseline = parse_manifest(BUNDLED).expect("bundled manifest");
        let mut removed_wire = baseline.wire_clone();
        removed_wire
            .releases
            .retain(|release| release.id != "v0.9.0");
        let removed = OptiScalerManifest::try_from(removed_wire).expect("valid removed history");
        assert!(!preserves_release_history(&removed, &baseline));

        let mut rewritten_wire = baseline.wire_clone();
        rewritten_wire.releases[0].archive_sha256 = "f".repeat(64);
        let rewritten =
            OptiScalerManifest::try_from(rewritten_wire).expect("valid rewritten history");
        assert!(!preserves_release_history(&rewritten, &baseline));

        let mut appended_wire = baseline.wire_clone();
        let mut future = baseline.current_release().expect("current release").clone();
        future.id = "v0.9.5".to_owned();
        future.source.tag = future.id.clone();
        appended_wire.releases.push(future);
        let appended = OptiScalerManifest::try_from(appended_wire).expect("valid appended history");
        assert!(preserves_release_history(&appended, &baseline));
    }

    #[test]
    fn remote_metadata_cannot_rewrite_history_first_accepted_remotely() {
        let bundled = parse_manifest(BUNDLED).expect("bundled manifest");
        let mut accepted_wire = bundled.wire_clone();
        accepted_wire.revision = "2099-01-01.1".to_owned();
        let mut future = bundled.current_release().expect("current release").clone();
        future.id = "v0.9.5".to_owned();
        future.source.tag = future.id.clone();
        accepted_wire.releases.push(future);
        let accepted = OptiScalerManifest::try_from(accepted_wire).expect("valid accepted history");

        let mut rewritten_wire = accepted.wire_clone();
        rewritten_wire.revision = "2099-01-01.2".to_owned();
        rewritten_wire.releases.last_mut().unwrap().archive_sha256 = "f".repeat(64);
        let rewritten =
            OptiScalerManifest::try_from(rewritten_wire).expect("valid rewritten history");
        assert!(validate_manifest_update(&rewritten, &accepted).is_err());

        let mut removed_wire = accepted.wire_clone();
        removed_wire.revision = "2099-01-01.2".to_owned();
        removed_wire.releases.pop();
        let removed = OptiScalerManifest::try_from(removed_wire).expect("valid removed history");
        assert!(validate_manifest_update(&removed, &accepted).is_err());

        let mut older_wire = accepted.wire_clone();
        older_wire.revision = bundled.revision.clone();
        let older = OptiScalerManifest::try_from(older_wire).expect("valid older revision");
        assert!(validate_manifest_update(&older, &accepted).is_err());
    }

    #[test]
    fn bundled_history_keeps_every_supported_release() {
        let manifest = parse_manifest(BUNDLED).expect("bundled manifest");
        let release_ids = manifest
            .releases
            .iter()
            .map(|release| release.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            release_ids,
            ["v0.9.0", "v0.9.1", "v0.9.2", "v0.9.3", "v0.9.4"]
        );
    }

    #[test]
    fn current_release_must_reference_known_history() {
        let mut value: serde_json::Value = serde_json::from_slice(BUNDLED).expect("json");
        value["current_release"] = serde_json::Value::String("v9.9.9".to_owned());
        assert!(parse_manifest(&serde_json::to_vec(&value).expect("serialize")).is_err());
    }

    #[test]
    fn archive_sources_must_be_the_official_optiscaler_repository() {
        let mut value: serde_json::Value = serde_json::from_slice(BUNDLED).expect("json");
        value["releases"][0]["source"]["repository"] =
            serde_json::Value::String("attacker/OptiScaler".to_owned());
        assert!(parse_manifest(&serde_json::to_vec(&value).expect("serialize")).is_err());
    }

    #[test]
    fn archive_urls_are_not_part_of_the_manifest_contract() {
        let mut value: serde_json::Value = serde_json::from_slice(BUNDLED).expect("json");
        value["releases"][0]["archive_url"] = serde_json::Value::String(
            "https://pub-48612a35034d40f88f42b4181547925a.r2.dev/archive.7z".to_owned(),
        );
        assert!(parse_manifest(&serde_json::to_vec(&value).expect("serialize")).is_err());
    }

    #[test]
    fn independent_modules_cannot_switch_to_an_unapproved_repository() {
        let mut value: serde_json::Value = serde_json::from_slice(BUNDLED).expect("json");
        let module = value["modules"]
            .as_array_mut()
            .expect("modules")
            .iter_mut()
            .find(|module| module["id"] == "optipatcher")
            .expect("OptiPatcher module");
        module["artifact"]["source"]["repository"] =
            serde_json::Value::String("attacker/OptiPatcher".to_owned());
        assert!(parse_manifest(&serde_json::to_vec(&value).expect("serialize")).is_err());
    }

    #[test]
    fn independent_modules_must_use_an_immutable_release_tag() {
        let mut value: serde_json::Value = serde_json::from_slice(BUNDLED).expect("json");
        let module = value["modules"]
            .as_array_mut()
            .expect("modules")
            .iter_mut()
            .find(|module| module["id"] == "optipatcher")
            .expect("OptiPatcher module");
        module["artifact"]["source"]["tag"] = serde_json::Value::String("rolling".to_owned());
        assert!(parse_manifest(&serde_json::to_vec(&value).expect("serialize")).is_err());
    }
}
