//! Package provenance and runtime compatibility metadata for library artifacts.

use serde::{Deserialize, Deserializer, Serialize};

use crate::{Architecture, Version, text::normalize_required_text};

use super::ComponentError;

/// Provenance of a release obtained from an upstream package registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpstreamPackage {
    provider: UpstreamPackageProvider,
    id: String,
    version: Version,
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
            version: Version::parse(version)
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
    pub const fn version(&self) -> &Version {
        &self.version
    }
}

/// Supported upstream package registries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpstreamPackageProvider {
    /// NuGet.org package registry.
    #[serde(rename = "nuget")]
    NuGet,
}

/// Runtime constraints carried by an artifact as a whole.
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

    /// Returns the target executable architecture.
    pub const fn architecture(&self) -> Architecture {
        self.architecture
    }

    /// Returns the optional runtime compatibility constraint.
    pub const fn compatibility(&self) -> Option<&RuntimeCompatibility> {
        self.compatibility.as_ref()
    }
}

/// Technology-specific ABI constraint for a runtime artifact.
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

/// Optional package provenance and runtime constraints for an artifact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    release_version: Option<Version>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upstream_package: Option<UpstreamPackage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    runtime_target: Option<RuntimeTarget>,
}

impl ArtifactMetadata {
    /// Returns the release version declared by the curated package.
    pub const fn release_version(&self) -> Option<&Version> {
        self.release_version.as_ref()
    }

    /// Returns upstream package provenance, when known.
    pub const fn upstream_package(&self) -> Option<&UpstreamPackage> {
        self.upstream_package.as_ref()
    }

    /// Returns runtime target constraints, when known.
    pub const fn runtime_target(&self) -> Option<&RuntimeTarget> {
        self.runtime_target.as_ref()
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
        self.release_version = Some(version);
        self
    }

    /// Sets runtime target constraints.
    #[must_use]
    pub fn with_runtime_target(mut self, target: RuntimeTarget) -> Self {
        self.runtime_target = Some(target);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VersionParseError;

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
            ComponentError::InvalidUpstreamPackageVersion(VersionParseError::InvalidSegment)
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
}
