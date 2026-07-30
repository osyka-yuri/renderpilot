//! Canonical package identity shared with the external catalog producer.

use std::collections::BTreeMap;

use renderpilot_domain::PackageVersion;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::ServiceError;

use super::library_error;
use super::types::{
    LibraryPackage, LibraryPackageMember, LibraryProvenance, LibraryReleaseChannel, LibraryTarget,
};

#[derive(Serialize)]
struct RevisionReleaseV1<'a> {
    version: &'a str,
    channel: &'a LibraryReleaseChannel,
}

#[derive(Serialize)]
struct RevisionInputV1<'a> {
    schema_version: u32,
    package_id: &'a str,
    technology: &'a str,
    variant: &'a str,
    release: RevisionReleaseV1<'a>,
    target: &'a LibraryTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: &'a Option<LibraryProvenance>,
    members: &'a [LibraryPackageMember],
}

#[derive(Serialize)]
struct RevisionReleaseV2<'a> {
    version: &'a str,
    channel: &'a LibraryReleaseChannel,
    components: &'a BTreeMap<String, PackageVersion>,
}

#[derive(Serialize)]
struct RevisionInputV2<'a> {
    schema_version: u32,
    package_id: &'a str,
    technology: &'a str,
    variant: &'a str,
    release: RevisionReleaseV2<'a>,
    target: &'a LibraryTarget,
    provenance: &'a LibraryProvenance,
    members: &'a [LibraryPackageMember],
}

pub(super) fn package_revision_sha256(package: &LibraryPackage) -> Result<String, ServiceError> {
    let value = package_revision_input(package)?;
    let canonical = canonical_json(&value)?;
    Ok(hex::encode(Sha256::digest(canonical.as_bytes())))
}

fn package_revision_input(package: &LibraryPackage) -> Result<serde_json::Value, ServiceError> {
    let value = if package.release.components.is_empty() {
        serde_json::to_value(RevisionInputV1 {
            schema_version: 1,
            package_id: &package.package_id,
            technology: &package.technology,
            variant: &package.variant,
            release: RevisionReleaseV1 {
                version: package.release.revision_version(),
                channel: &package.release.channel,
            },
            target: &package.target,
            provenance: &package.provenance,
            members: &package.members,
        })
    } else {
        let provenance = package.provenance.as_ref().ok_or_else(|| {
            library_error(format!(
                "composite package `{}` requires provenance",
                package.package_id
            ))
        })?;
        serde_json::to_value(RevisionInputV2 {
            schema_version: 2,
            package_id: &package.package_id,
            technology: &package.technology,
            variant: &package.variant,
            release: RevisionReleaseV2 {
                version: package.release.revision_version(),
                channel: &package.release.channel,
                components: &package.release.components,
            },
            target: &package.target,
            provenance,
            members: &package.members,
        })
    };
    let value = value
        .map_err(|error| library_error(format!("failed to encode package revision: {error}")))?;
    Ok(value)
}

fn canonical_json(value: &serde_json::Value) -> Result<String, ServiceError> {
    match value {
        serde_json::Value::Array(items) => Ok(format!(
            "[{}]",
            items
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        )),
        serde_json::Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort_unstable();
            let fields = keys
                .into_iter()
                .map(|key| {
                    let encoded_key = serde_json::to_string(key).map_err(|error| {
                        library_error(format!("failed to encode revision key: {error}"))
                    })?;
                    Ok(format!("{encoded_key}:{}", canonical_json(&object[key])?))
                })
                .collect::<Result<Vec<_>, ServiceError>>()?;
            Ok(format!("{{{}}}", fields.join(",")))
        }
        scalar => serde_json::to_string(scalar)
            .map_err(|error| library_error(format!("failed to encode package revision: {error}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn legacy_v1_wire_version_authenticates_without_leaking_into_new_output() {
        let legacy: LibraryPackage = serde_json::from_value(serde_json::json!({
            "package_id": "legacy.package",
            "revision_sha256": "0".repeat(64),
            "technology": "test",
            "variant": "runtime",
            "display_name": "Legacy",
            "release": {
                "version": "1.2.3.0",
                "channel": "stable",
                "label": null
            },
            "target": {
                "os": "windows",
                "architecture": "X64"
            },
            "members": []
        }))
        .expect("legacy package");
        assert_eq!(legacy.release.version.as_str(), "1.2.3");
        assert_eq!(legacy.release.revision_version(), "1.2.3.0");

        let canonical: LibraryPackage =
            serde_json::from_value(serde_json::to_value(&legacy).expect("serialize"))
                .expect("canonical package");
        assert_eq!(canonical.release.revision_version(), "1.2.3");
        assert_ne!(
            package_revision_sha256(&legacy).unwrap(),
            package_revision_sha256(&canonical).unwrap()
        );
    }

    #[test]
    #[ignore = "requires a pinned renderpilot-libraries producer checkout"]
    fn producer_v2_golden_fixture_matches_rust_projection() {
        let fixture_path = std::env::var_os("RENDERPILOT_LIBRARIES_FIXTURE")
            .map(PathBuf::from)
            .expect("RENDERPILOT_LIBRARIES_FIXTURE must point to the pinned producer fixture");
        let fixture: serde_json::Value =
            serde_json::from_slice(&fs::read(&fixture_path).unwrap_or_else(|error| {
                panic!("failed to read {}: {error}", fixture_path.display())
            }))
            .expect("producer fixture JSON");
        let package: LibraryPackage =
            serde_json::from_value(fixture["package"].clone()).expect("fixture package");
        let input = package_revision_input(&package).expect("Rust revision projection");
        assert_eq!(input, fixture["canonical_input"]);
        let canonical = canonical_json(&input).expect("canonical revision JSON");
        assert_eq!(canonical, fixture["canonical_json"]);
        let revision = hex::encode(Sha256::digest(canonical.as_bytes()));
        assert_eq!(revision, fixture["revision_sha256"]);
        assert_eq!(package.revision_sha256, revision);
    }
}
