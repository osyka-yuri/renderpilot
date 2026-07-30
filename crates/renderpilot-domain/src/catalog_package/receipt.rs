//! Versioned catalog package receipts and fail-closed projections.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Architecture, ArtifactId, PeExportSet, PeImportProfile, Sha256Hash};

use super::{
    provenance::{
        CatalogLegalDocumentReceipt, CatalogPackageProvenanceReceipt, CatalogProvenanceReceipt,
        CatalogSignatureReceipt, CatalogTargetReceipt,
    },
    release::PackageRelease,
};

/// Wire-schema marker for [`CatalogPackageReceiptV1`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CatalogReceiptSchemaV1;

impl Serialize for CatalogReceiptSchemaV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(1)
    }
}

impl<'de> Deserialize<'de> for CatalogReceiptSchemaV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_schema_version(deserializer, 1).map(|()| Self)
    }
}

/// V1 receipt persisted with a downloaded single-member artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPackageReceiptV1 {
    /// Constant V1 wire-schema marker.
    pub schema_version: CatalogReceiptSchemaV1,
    /// Stable catalog package identifier.
    pub package_id: String,
    /// Catalog vendor identifier.
    pub vendor: String,
    /// Library technology slug.
    pub technology: String,
    /// Package variant.
    pub variant: String,
    /// User-facing package name.
    pub display_name: String,
    /// Exact package release identity.
    #[serde(
        serialize_with = "serialize_v1_release",
        deserialize_with = "deserialize_v1_release"
    )]
    pub release: PackageRelease,
    /// Runtime target snapshot.
    pub target: CatalogTargetReceipt,
    /// Optional immutable upstream provenance.
    pub provenance: Option<CatalogProvenanceReceipt>,
    /// Canonical package revision digest.
    pub revision_sha256: Sha256Hash,
    /// Primary member installation name.
    pub primary_file_name: String,
    /// Primary member DLL digest.
    pub primary_sha256: Sha256Hash,
    /// Primary member signature snapshot.
    pub primary_signature: CatalogSignatureReceipt,
    /// Legal-document links applicable at download time.
    pub legal_documents: Vec<CatalogLegalDocumentReceipt>,
    /// Total uncompressed package size.
    pub size_bytes: u64,
}

impl CatalogPackageReceiptV1 {
    /// Returns the only valid local artifact identity for this package revision.
    #[must_use]
    pub fn artifact_id(&self) -> ArtifactId {
        ArtifactId::for_package_revision(&self.revision_sha256)
    }
}

/// Wire-schema marker for [`CatalogPackageReceiptV2`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CatalogReceiptSchemaV2;

impl Serialize for CatalogReceiptSchemaV2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(2)
    }
}

impl<'de> Deserialize<'de> for CatalogReceiptSchemaV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_schema_version(deserializer, 2).map(|()| Self)
    }
}

/// One member of an immutable composite package receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPackageMemberReceipt {
    /// Semantic component name, such as `vorbis` or `ogg`.
    pub component: String,
    /// Semantic package role.
    pub role: String,
    /// Target DLL basename.
    pub install_as: String,
    /// Uncompressed DLL digest.
    pub sha256: Sha256Hash,
    /// PE architecture.
    pub architecture: Architecture,
    /// Verified named export surface.
    pub named_exports: PeExportSet,
    /// Separately verified regular and delay-load imports.
    pub imports: PeImportProfile,
    /// Authenticode status of this exact member DLL.
    pub signature: CatalogSignatureReceipt,
}

/// V2 receipt for composite packages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPackageReceiptV2 {
    /// Constant V2 wire-schema marker.
    pub schema_version: CatalogReceiptSchemaV2,
    /// Stable catalog package identifier.
    pub package_id: String,
    /// Catalog vendor identifier.
    pub vendor: String,
    /// Library technology slug.
    pub technology: String,
    /// Package variant.
    pub variant: String,
    /// User-facing package name.
    pub display_name: String,
    /// Composite release identity.
    pub release: PackageRelease,
    /// Runtime target snapshot.
    pub target: CatalogTargetReceipt,
    /// Immutable provenance for the composite package.
    pub provenance: CatalogPackageProvenanceReceipt,
    /// Canonical package revision digest.
    pub revision_sha256: Sha256Hash,
    /// Ordered complete package membership. The primary member is first.
    pub members: Vec<CatalogPackageMemberReceipt>,
    /// Legal-document links applicable at download time.
    pub legal_documents: Vec<CatalogLegalDocumentReceipt>,
    /// Total uncompressed package size.
    pub size_bytes: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogPackageReceiptV2Wire {
    schema_version: CatalogReceiptSchemaV2,
    package_id: String,
    vendor: String,
    technology: String,
    variant: String,
    display_name: String,
    release: PackageRelease,
    target: CatalogTargetReceipt,
    provenance: CatalogPackageProvenanceReceipt,
    revision_sha256: Sha256Hash,
    members: Vec<CatalogPackageMemberReceipt>,
    legal_documents: Vec<CatalogLegalDocumentReceipt>,
    size_bytes: u64,
}

impl<'de> Deserialize<'de> for CatalogPackageReceiptV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CatalogPackageReceiptV2Wire::deserialize(deserializer)?;
        if wire.members.is_empty() {
            return Err(serde::de::Error::custom(
                "catalog package receipt V2 requires at least one member",
            ));
        }
        Ok(Self {
            schema_version: wire.schema_version,
            package_id: wire.package_id,
            vendor: wire.vendor,
            technology: wire.technology,
            variant: wire.variant,
            display_name: wire.display_name,
            release: wire.release,
            target: wire.target,
            provenance: wire.provenance,
            revision_sha256: wire.revision_sha256,
            members: wire.members,
            legal_documents: wire.legal_documents,
            size_bytes: wire.size_bytes,
        })
    }
}

impl CatalogPackageReceiptV2 {
    /// Returns the only valid local artifact identity for this package revision.
    #[must_use]
    pub fn artifact_id(&self) -> ArtifactId {
        ArtifactId::for_package_revision(&self.revision_sha256)
    }
}

/// Version-selected immutable catalog receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CatalogPackageReceipt {
    /// Existing single-version package receipt.
    V1(CatalogPackageReceiptV1),
    /// Composite package receipt.
    V2(CatalogPackageReceiptV2),
}

impl From<CatalogPackageReceiptV1> for CatalogPackageReceipt {
    fn from(value: CatalogPackageReceiptV1) -> Self {
        Self::V1(value)
    }
}

impl From<CatalogPackageReceiptV2> for CatalogPackageReceipt {
    fn from(value: CatalogPackageReceiptV2) -> Self {
        Self::V2(value)
    }
}

impl CatalogPackageReceipt {
    /// Returns whether this in-memory receipt still satisfies its wire-schema
    /// invariants. Storage deserialization enforces the same conditions; this
    /// guard protects candidate eligibility from manually-constructed values.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::V1(value) => value.release.components.is_empty(),
            Self::V2(value) => !value.members.is_empty(),
        }
    }

    /// Stable package identifier.
    #[must_use]
    pub fn package_id(&self) -> &str {
        match self {
            Self::V1(value) => &value.package_id,
            Self::V2(value) => &value.package_id,
        }
    }

    /// Stable vendor identifier.
    #[must_use]
    pub fn vendor(&self) -> &str {
        match self {
            Self::V1(value) => &value.vendor,
            Self::V2(value) => &value.vendor,
        }
    }

    /// Stable technology slug.
    #[must_use]
    pub fn technology(&self) -> &str {
        match self {
            Self::V1(value) => &value.technology,
            Self::V2(value) => &value.technology,
        }
    }

    /// Package variant.
    #[must_use]
    pub fn variant(&self) -> &str {
        match self {
            Self::V1(value) => &value.variant,
            Self::V2(value) => &value.variant,
        }
    }

    /// User-facing package name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        match self {
            Self::V1(value) => &value.display_name,
            Self::V2(value) => &value.display_name,
        }
    }

    /// Exact release identity.
    #[must_use]
    pub fn release(&self) -> &PackageRelease {
        match self {
            Self::V1(value) => &value.release,
            Self::V2(value) => &value.release,
        }
    }

    /// Canonical revision digest.
    #[must_use]
    pub fn revision_sha256(&self) -> &Sha256Hash {
        match self {
            Self::V1(value) => &value.revision_sha256,
            Self::V2(value) => &value.revision_sha256,
        }
    }

    /// Runtime target snapshot.
    #[must_use]
    pub fn target(&self) -> &CatalogTargetReceipt {
        match self {
            Self::V1(value) => &value.target,
            Self::V2(value) => &value.target,
        }
    }

    /// Applicable legal documents.
    #[must_use]
    pub fn legal_documents(&self) -> &[CatalogLegalDocumentReceipt] {
        match self {
            Self::V1(value) => &value.legal_documents,
            Self::V2(value) => &value.legal_documents,
        }
    }

    /// Total uncompressed package size.
    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        match self {
            Self::V1(value) => value.size_bytes,
            Self::V2(value) => value.size_bytes,
        }
    }

    /// Primary installation basename.
    #[must_use]
    pub fn primary_file_name(&self) -> Option<&str> {
        match self {
            Self::V1(value) => Some(&value.primary_file_name),
            Self::V2(value) => value
                .members
                .first()
                .map(|member| member.install_as.as_str()),
        }
    }

    /// Primary uncompressed DLL digest.
    #[must_use]
    pub fn primary_sha256(&self) -> Option<&Sha256Hash> {
        match self {
            Self::V1(value) => Some(&value.primary_sha256),
            Self::V2(value) => value.members.first().map(|member| &member.sha256),
        }
    }

    /// Primary member signature snapshot.
    #[must_use]
    pub fn primary_signature(&self) -> Option<&CatalogSignatureReceipt> {
        match self {
            Self::V1(value) => Some(&value.primary_signature),
            Self::V2(value) => value.members.first().map(|member| &member.signature),
        }
    }

    /// Whether any package member is unsigned.
    #[must_use]
    pub fn has_unsigned_members(&self) -> bool {
        match self {
            Self::V1(value) => matches!(value.primary_signature, CatalogSignatureReceipt::Unsigned),
            Self::V2(value) => value
                .members
                .iter()
                .any(|member| matches!(member.signature, CatalogSignatureReceipt::Unsigned)),
        }
    }

    /// Local artifact identity derived from the revision.
    #[must_use]
    pub fn artifact_id(&self) -> ArtifactId {
        ArtifactId::for_package_revision(self.revision_sha256())
    }

    /// Returns composite provenance for a V2 receipt.
    #[must_use]
    pub fn composite_provenance(&self) -> Option<&CatalogPackageProvenanceReceipt> {
        match self {
            Self::V1(_) => None,
            Self::V2(value) => Some(&value.provenance),
        }
    }
}

fn deserialize_schema_version<'de, D>(deserializer: D, expected: u32) -> Result<(), D::Error>
where
    D: Deserializer<'de>,
{
    let actual = u32::deserialize(deserializer)?;
    if actual != expected {
        return Err(serde::de::Error::custom(format!(
            "unsupported catalog package receipt schema {actual}"
        )));
    }
    Ok(())
}

fn deserialize_v1_release<'de, D>(deserializer: D) -> Result<PackageRelease, D::Error>
where
    D: Deserializer<'de>,
{
    let release = PackageRelease::deserialize(deserializer)?;
    if !release.components.is_empty() {
        return Err(serde::de::Error::custom(
            "catalog package receipt V1 does not support composite release components",
        ));
    }
    Ok(release)
}

fn serialize_v1_release<S>(release: &PackageRelease, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if !release.components.is_empty() {
        return Err(serde::ser::Error::custom(
            "catalog package receipt V1 does not support composite release components",
        ));
    }
    release.serialize(serializer)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    // These fixtures deliberately assert the complete serialized byte stream.
    // V1 and V2 retain separate wire DTOs, so changing either field order or
    // V1's special release handling is a contract change rather than a DRY
    // refactor opportunity.
    const V1_GOLDEN: &[u8] = br#"{"schema_version":1,"package_id":"nvngx","vendor":"nvidia","technology":"dlss_super_resolution","variant":"standard","display_name":"DLSS","release":{"version":"3.7.0","channel":"stable","label":null},"target":{"os":"windows","architecture":"X64","compatibility":null},"provenance":null,"revision_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","primary_file_name":"nvngx_dlss.dll","primary_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","primary_signature":{"status":"unsigned"},"legal_documents":[],"size_bytes":1024}"#;

    const V2_GOLDEN: &[u8] = br#"{"schema_version":2,"package_id":"xiph","vendor":"xiph","technology":"xiph_vorbis","variant":"shared.lib","display_name":"Xiph","release":{"version":"1.0.0","channel":"stable","label":null,"components":{"ogg":"1.3.5","vorbis":"1.3.7"}},"target":{"os":"windows","architecture":"X64","compatibility":null},"provenance":{"kind":"github_release","repository":"xiph/ogg","tag":"v1.3.5","commit_sha":"0123456789abcdef0123456789abcdef01234567"},"revision_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","members":[{"component":"ogg","role":"container","install_as":"libogg.dll","sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","architecture":"X64","named_exports":["ogg_sync_init"],"imports":{"regular":["kernel32.dll"],"delay":[]},"signature":{"status":"signed","subject":"Xiph.Org","thumbprint":null,"signed_at":null}}],"legal_documents":[],"size_bytes":1024}"#;

    #[test]
    fn v1_wire_fixture_is_byte_stable() {
        let receipt =
            serde_json::from_slice::<CatalogPackageReceiptV1>(V1_GOLDEN).expect("valid V1 fixture");

        assert_eq!(
            serde_json::to_vec(&receipt).expect("serialize V1 fixture"),
            V1_GOLDEN
        );
    }

    #[test]
    fn v2_wire_fixture_is_byte_stable() {
        let receipt =
            serde_json::from_slice::<CatalogPackageReceiptV2>(V2_GOLDEN).expect("valid V2 fixture");

        assert_eq!(
            serde_json::to_vec(&receipt).expect("serialize V2 fixture"),
            V2_GOLDEN
        );
    }

    fn valid_v2_value() -> serde_json::Value {
        serde_json::from_slice(V2_GOLDEN).expect("valid V2 JSON")
    }

    #[test]
    fn receipt_wire_dtos_reject_unknown_fields_and_wrong_schema_versions() {
        let mut v1: serde_json::Value = serde_json::from_slice(V1_GOLDEN).expect("valid V1 JSON");
        v1["unexpected"] = json!(true);
        assert!(serde_json::from_value::<CatalogPackageReceiptV1>(v1).is_err());

        let mut v2 = valid_v2_value();
        v2["unexpected"] = json!(true);
        assert!(serde_json::from_value::<CatalogPackageReceiptV2>(v2).is_err());

        let mut wrong_v1: serde_json::Value =
            serde_json::from_slice(V1_GOLDEN).expect("valid V1 JSON");
        wrong_v1["schema_version"] = json!(2);
        assert!(serde_json::from_value::<CatalogPackageReceiptV1>(wrong_v1).is_err());

        let mut wrong_v2 = valid_v2_value();
        wrong_v2["schema_version"] = json!(1);
        assert!(serde_json::from_value::<CatalogPackageReceiptV2>(wrong_v2).is_err());
    }

    #[test]
    fn v1_rejects_composite_release_components_on_both_wire_paths() {
        let mut receipt: serde_json::Value =
            serde_json::from_slice(V1_GOLDEN).expect("valid V1 JSON");
        receipt["release"]["components"] = json!({ "ogg": "1.3.5" });
        assert!(serde_json::from_value::<CatalogPackageReceiptV1>(receipt).is_err());
    }

    #[test]
    fn v2_deserialization_rejects_an_empty_member_list() {
        let mut value = valid_v2_value();
        value["members"] = json!([]);

        let error = serde_json::from_value::<CatalogPackageReceiptV2>(value)
            .expect_err("empty V2 receipt must be rejected");
        assert!(error.to_string().contains("requires at least one member"));
    }

    #[test]
    fn v2_roundtrip_preserves_members_and_mixed_signatures() {
        let mut value = valid_v2_value();
        let mut unsigned = value["members"][0].clone();
        unsigned["component"] = json!("vorbis");
        unsigned["role"] = json!("codec");
        unsigned["install_as"] = json!("libvorbis.dll");
        unsigned["signature"] = json!({ "status": "unsigned" });
        value["members"]
            .as_array_mut()
            .expect("members")
            .push(unsigned);

        let receipt: CatalogPackageReceiptV2 =
            serde_json::from_value(value).expect("valid V2 receipt");
        let receipt = CatalogPackageReceipt::V2(receipt);
        assert!(receipt.has_unsigned_members());
        assert_eq!(receipt.primary_file_name(), Some("libogg.dll"));
        assert!(matches!(
            receipt.primary_signature(),
            Some(CatalogSignatureReceipt::Signed { .. })
        ));

        let roundtrip = serde_json::to_value(&receipt).expect("serialize receipt");
        let decoded: CatalogPackageReceipt =
            serde_json::from_value(roundtrip).expect("deserialize receipt");
        assert_eq!(decoded, receipt);
    }

    #[test]
    fn v2_accessors_fail_closed_for_an_invalid_in_memory_empty_receipt() {
        let mut receipt: CatalogPackageReceiptV2 =
            serde_json::from_value(valid_v2_value()).expect("valid V2 receipt");
        receipt.members.clear();
        let receipt = CatalogPackageReceipt::V2(receipt);

        assert_eq!(receipt.primary_file_name(), None);
        assert_eq!(receipt.primary_sha256(), None);
        assert_eq!(receipt.primary_signature(), None);
        assert!(!receipt.has_unsigned_members());
    }
}
