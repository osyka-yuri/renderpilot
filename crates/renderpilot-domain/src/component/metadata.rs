//! Package provenance and runtime compatibility metadata for library artifacts.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    Architecture, CatalogPackageReceiptV1, PackageVersion, Version, text::normalize_required_text,
};

use super::ComponentError;

/// Provenance of a release obtained from an upstream package registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpstreamPackage {
    provider: UpstreamPackageProvider,
    id: String,
    version: PackageVersion,
}

impl<'de> Deserialize<'de> for UpstreamPackage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WirePackage {
            provider: UpstreamPackageProvider,
            id: String,
            version: String,
        }

        let wire = WirePackage::deserialize(deserializer)?;
        Self::new(wire.provider, wire.id, wire.version).map_err(serde::de::Error::custom)
    }
}

impl UpstreamPackage {
    /// Creates normalized and validated upstream package metadata.
    pub fn new(
        provider: UpstreamPackageProvider,
        id: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self, ComponentError> {
        Ok(Self {
            provider,
            id: normalize_required_text("upstream_package_id", id)?,
            version: PackageVersion::parse(version)
                .map_err(ComponentError::InvalidUpstreamPackageVersion)?,
        })
    }

    /// Returns the upstream registry provider.
    pub const fn provider(&self) -> UpstreamPackageProvider {
        self.provider
    }

    /// Returns the upstream package identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the validated upstream package release version.
    pub const fn version(&self) -> &PackageVersion {
        &self.version
    }
}

/// Supported upstream package registries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpstreamPackageProvider {
    /// NuGet.org package registry.
    #[serde(rename = "nuget")]
    NuGet,
    /// GitHub release in an upstream repository.
    #[serde(rename = "github")]
    GitHub,
}

/// Runtime constraints carried by an artifact as a whole.
///
/// Architecture is interpreted by the technology policy: executable-context
/// runtimes target the selected EXE, while OpenVR targets the installed loader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTarget {
    architecture: Architecture,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compatibility: Option<RuntimeCompatibility>,
}

impl RuntimeTarget {
    /// Creates an architecture-only target.
    #[must_use]
    pub const fn new(architecture: Architecture) -> Self {
        Self {
            architecture,
            compatibility: None,
        }
    }

    /// Adds a typed runtime compatibility constraint.
    #[must_use]
    pub fn with_compatibility(mut self, compatibility: RuntimeCompatibility) -> Self {
        self.compatibility = Some(compatibility);
        self
    }

    /// Returns the architecture interpreted by the technology policy.
    pub const fn architecture(&self) -> Architecture {
        self.architecture
    }

    /// Returns the optional runtime compatibility constraint.
    pub const fn compatibility(&self) -> Option<&RuntimeCompatibility> {
        self.compatibility.as_ref()
    }
}

/// Technology-specific runtime compatibility constraint for an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeCompatibility {
    /// Exact DirectX 12 Agility SDK line requested by `D3D12SDKVersion`.
    D3d12Sdk {
        /// Numeric Agility SDK line, for example `618`.
        version: u32,
    },
}

impl RuntimeCompatibility {
    /// Returns the D3D12 SDK line for that compatibility variant.
    #[must_use]
    pub const fn as_d3d12_sdk_version(&self) -> Option<u32> {
        match self {
            Self::D3d12Sdk { version } => Some(*version),
        }
    }
}

/// Atomic package release identity and optional presentation annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseMetadata {
    version: Version,
    label: Option<String>,
}

impl ReleaseMetadata {
    /// Creates release metadata with an optional normalized annotation.
    pub fn new(version: Version, label: Option<String>) -> Result<Self, ComponentError> {
        Ok(Self {
            version,
            label: label
                .map(|value| normalize_required_text("release_label", value))
                .transpose()?,
        })
    }

    /// Returns the canonical release version.
    pub const fn version(&self) -> &Version {
        &self.version
    }

    /// Returns the optional supplemental release annotation.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

/// Optional package provenance and runtime constraints for an artifact.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtifactMetadata {
    release: Option<ReleaseMetadata>,
    upstream_package: Option<UpstreamPackage>,
    runtime_target: Option<RuntimeTarget>,
    catalog_package_receipt: Option<CatalogPackageReceiptV1>,
}

impl ArtifactMetadata {
    /// Returns the atomic release version and annotation, when declared.
    pub const fn release(&self) -> Option<&ReleaseMetadata> {
        self.release.as_ref()
    }

    /// Returns the release version declared by the curated package.
    pub const fn release_version(&self) -> Option<&Version> {
        match self.release() {
            Some(release) => Some(release.version()),
            None => None,
        }
    }

    /// Returns the optional supplemental release annotation.
    pub fn release_label(&self) -> Option<&str> {
        self.release.as_ref().and_then(ReleaseMetadata::label)
    }

    /// Returns upstream package provenance, when known.
    pub const fn upstream_package(&self) -> Option<&UpstreamPackage> {
        self.upstream_package.as_ref()
    }

    /// Returns runtime target constraints, when known.
    pub const fn runtime_target(&self) -> Option<&RuntimeTarget> {
        self.runtime_target.as_ref()
    }

    /// Returns the immutable catalog receipt attached at download time.
    pub const fn catalog_package_receipt(&self) -> Option<&CatalogPackageReceiptV1> {
        self.catalog_package_receipt.as_ref()
    }

    /// Sets upstream package provenance.
    #[must_use]
    pub fn with_upstream_package(mut self, package: UpstreamPackage) -> Self {
        self.upstream_package = Some(package);
        self
    }

    /// Sets the release version declared by the curated package.
    #[must_use]
    pub fn with_release_version(mut self, version: Version) -> Self {
        self.release = Some(ReleaseMetadata {
            version,
            label: None,
        });
        self
    }

    /// Sets an atomic release version and optional supplemental annotation.
    pub fn with_release(
        mut self,
        version: Version,
        label: Option<String>,
    ) -> Result<Self, ComponentError> {
        self.release = Some(ReleaseMetadata::new(version, label)?);
        Ok(self)
    }

    /// Sets runtime target constraints.
    #[must_use]
    pub fn with_runtime_target(mut self, target: RuntimeTarget) -> Self {
        self.runtime_target = Some(target);
        self
    }

    /// Attaches the immutable catalog receipt used for local package lifecycle.
    #[must_use]
    pub fn with_catalog_package_receipt(mut self, receipt: CatalogPackageReceiptV1) -> Self {
        self.catalog_package_receipt = Some(receipt);
        self
    }
}

impl Serialize for ArtifactMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct WireMetadata<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            release_version: Option<&'a Version>,
            #[serde(skip_serializing_if = "Option::is_none")]
            release_label: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            upstream_package: Option<&'a UpstreamPackage>,
            #[serde(skip_serializing_if = "Option::is_none")]
            runtime_target: Option<&'a RuntimeTarget>,
            #[serde(skip_serializing_if = "Option::is_none")]
            catalog_package_receipt: Option<&'a CatalogPackageReceiptV1>,
        }

        WireMetadata {
            release_version: self.release_version(),
            release_label: self.release_label(),
            upstream_package: self.upstream_package(),
            runtime_target: self.runtime_target(),
            catalog_package_receipt: self.catalog_package_receipt(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ArtifactMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireMetadata {
            #[serde(default)]
            release_version: Option<Version>,
            #[serde(default)]
            release_label: Option<String>,
            #[serde(default)]
            upstream_package: Option<UpstreamPackage>,
            #[serde(default)]
            runtime_target: Option<RuntimeTarget>,
            #[serde(default)]
            catalog_package_receipt: Option<CatalogPackageReceiptV1>,
        }

        let wire = WireMetadata::deserialize(deserializer)?;
        let release = match (wire.release_version, wire.release_label) {
            (Some(version), label) => {
                Some(ReleaseMetadata::new(version, label).map_err(serde::de::Error::custom)?)
            }
            (None, None) => None,
            (None, Some(_)) => {
                return Err(serde::de::Error::custom(
                    "release_label requires release_version",
                ));
            }
        };
        Ok(Self {
            release,
            upstream_package: wire.upstream_package,
            runtime_target: wire.runtime_target,
            catalog_package_receipt: wire.catalog_package_receipt,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CatalogReceiptSchemaV1, PackageVersionParseError, VersionParseError};

    #[test]
    fn upstream_package_normalizes_and_round_trips_as_string_json() {
        let package = UpstreamPackage::new(
            UpstreamPackageProvider::NuGet,
            "  Microsoft.Direct3D.DXC  ",
            " 01.009.2602.024 ",
        )
        .expect("valid package");

        assert_eq!(package.id(), "Microsoft.Direct3D.DXC");
        assert_eq!(package.version().as_str(), "1.9.2602.24");

        let json = serde_json::to_string(&package).expect("serialize package");
        assert!(json.contains(r#""version":"1.9.2602.24""#));
        assert_eq!(
            serde_json::from_str::<UpstreamPackage>(&json).expect("deserialize package"),
            package
        );
    }

    #[test]
    fn upstream_package_rejects_invalid_release_version() {
        let error = UpstreamPackage::new(
            UpstreamPackageProvider::NuGet,
            "Microsoft.Direct3D.DXC",
            "1.beta",
        )
        .expect_err("invalid version");

        assert_eq!(
            error,
            ComponentError::InvalidUpstreamPackageVersion(
                PackageVersionParseError::InvalidNumericCore(VersionParseError::InvalidSegment)
            )
        );
    }

    #[test]
    fn upstream_package_deserialization_uses_constructor_validation() {
        let error = serde_json::from_str::<UpstreamPackage>(
            r#"{"provider":"nuget","id":" ","version":"1.0"}"#,
        )
        .expect_err("blank id");

        assert!(
            error
                .to_string()
                .contains("upstream_package_id cannot be empty")
        );
    }

    #[test]
    fn empty_metadata_remains_an_empty_json_object() {
        assert_eq!(
            serde_json::to_string(&ArtifactMetadata::default()).expect("serialize metadata"),
            "{}"
        );
    }

    #[test]
    fn catalog_receipt_schema_is_a_closed_v1_wire_type() {
        assert_eq!(
            serde_json::from_str::<CatalogReceiptSchemaV1>("1").expect("v1"),
            CatalogReceiptSchemaV1
        );
        let error = serde_json::from_str::<CatalogReceiptSchemaV1>("2").expect_err("future schema");
        assert!(
            error
                .to_string()
                .contains("unsupported catalog package receipt schema 2")
        );
        assert_eq!(
            serde_json::to_string(&CatalogReceiptSchemaV1).expect("serialize v1"),
            "1"
        );
    }

    #[test]
    fn explicit_release_version_round_trips_without_registry_provenance() {
        let metadata = ArtifactMetadata::default()
            .with_release_version(Version::parse("4.1.1.2740").expect("valid curated release"));
        let json = serde_json::to_string(&metadata).expect("serialize metadata");
        let restored: ArtifactMetadata = serde_json::from_str(&json).expect("deserialize metadata");

        assert_eq!(
            restored.release_version().map(Version::as_str),
            Some("4.1.1.2740")
        );
        assert!(restored.upstream_package().is_none());
    }

    #[test]
    fn release_metadata_preserves_flat_wire_shape_and_rejects_orphan_label() {
        let metadata = ArtifactMetadata::default()
            .with_release(
                Version::parse("1.1.3").expect("version"),
                Some("revision b".to_owned()),
            )
            .expect("release");
        let json = serde_json::to_value(&metadata).expect("serialize");
        assert_eq!(json["release_version"], "1.1.3");
        assert_eq!(json["release_label"], "revision b");

        let error = serde_json::from_value::<ArtifactMetadata>(serde_json::json!({
            "release_label": "revision b"
        }))
        .expect_err("orphan label");
        assert!(error.to_string().contains("requires release_version"));
    }
}
