//! Canonical package identity shared with the external catalog producer.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::ServiceError;

use super::library_error;
use super::types::{
    LibraryPackage, LibraryPackageMember, LibraryProvenance, LibraryReleaseChannel, LibraryTarget,
};

const PACKAGE_REVISION_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
struct RevisionRelease<'a> {
    version: &'a str,
    channel: &'a LibraryReleaseChannel,
}

#[derive(Serialize)]
struct RevisionInput<'a> {
    schema_version: u32,
    package_id: &'a str,
    technology: &'a str,
    variant: &'a str,
    release: RevisionRelease<'a>,
    target: &'a LibraryTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: &'a Option<LibraryProvenance>,
    members: &'a [LibraryPackageMember],
}

pub(super) fn package_revision_sha256(package: &LibraryPackage) -> Result<String, ServiceError> {
    let input = RevisionInput {
        schema_version: PACKAGE_REVISION_SCHEMA_VERSION,
        package_id: &package.package_id,
        technology: &package.technology,
        variant: &package.variant,
        release: RevisionRelease {
            version: &package.release.version,
            channel: &package.release.channel,
        },
        target: &package.target,
        provenance: &package.provenance,
        members: &package.members,
    };
    let value = serde_json::to_value(input)
        .map_err(|error| library_error(format!("failed to encode package revision: {error}")))?;
    let canonical = canonical_json(&value)?;
    Ok(hex::encode(Sha256::digest(canonical.as_bytes())))
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
