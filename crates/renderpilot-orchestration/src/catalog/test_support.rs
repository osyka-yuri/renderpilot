//! Catalog-level fixtures shared by otherwise independent test modules.

use std::{fs, path::Path};

use renderpilot_domain::{
    Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, CatalogPackageReceiptV1,
    CatalogReceiptSchemaV1, CatalogSignatureReceipt, CatalogTargetReceipt, ComponentFile,
    LibraryArtifact, LibraryTechnology, PackageRelease, PackageVersion, PathRef, ReleaseChannel,
    RuntimeCompatibility, RuntimeTarget, UpstreamPackage, UpstreamPackageProvider, Version,
};

pub(crate) fn d3d12_preview_artifact_at(source: &Path, sdk_line: u32) -> LibraryArtifact {
    let source_hash = renderpilot_detection::sha256_file(source).expect("hash file");
    let package_version = format!("1.{sdk_line}.1");
    let receipt = CatalogPackageReceiptV1 {
        schema_version: CatalogReceiptSchemaV1,
        package_id: "microsoft.d3d12-preview".to_owned(),
        vendor: "microsoft".to_owned(),
        technology: LibraryTechnology::D3D12Agility.as_slug().to_owned(),
        variant: "runtime".to_owned(),
        display_name: "Microsoft D3D12 Agility Preview".to_owned(),
        release: PackageRelease {
            version: PackageVersion::parse(&package_version).expect("package version"),
            channel: ReleaseChannel::Preview,
            label: None,
            components: Default::default(),
        },
        target: CatalogTargetReceipt {
            os: "windows".to_owned(),
            architecture: Architecture::X64,
            compatibility: None,
        },
        provenance: None,
        revision_sha256: source_hash.clone(),
        primary_file_name: "D3D12Core.dll".to_owned(),
        primary_sha256: source_hash.clone(),
        primary_signature: CatalogSignatureReceipt::Unsigned,
        legal_documents: Vec::new(),
        size_bytes: fs::metadata(source).expect("source metadata").len(),
    };

    LibraryArtifact::new(
        ArtifactId::for_package_revision(&source_hash),
        LibraryTechnology::D3D12Agility,
        "D3D12Core.dll",
        vec![
            ComponentFile::new(path_ref(source))
                .with_sha256(source_hash)
                .with_version(Version::parse(&package_version).expect("version")),
        ],
        ArtifactTrustLevel::CatalogDownloaded,
    )
    .expect("D3D12 preview artifact")
    .with_source("test-library")
    .expect("source")
    .with_metadata(
        ArtifactMetadata::default()
            .with_upstream_package(
                UpstreamPackage::new(
                    UpstreamPackageProvider::NuGet,
                    "Microsoft.Direct3D.D3D12",
                    &package_version,
                )
                .expect("package"),
            )
            .with_runtime_target(
                RuntimeTarget::new(Architecture::X64)
                    .with_compatibility(RuntimeCompatibility::D3d12Sdk { version: sdk_line }),
            )
            .with_catalog_package_receipt(receipt),
    )
}

pub(crate) fn synthetic_versioned_d3d12_runtime(sdk_line: u32) -> Vec<u8> {
    const PE_OFFSET: usize = 0x80;
    const COFF_OFFSET: usize = PE_OFFSET + 4;
    const OPTIONAL_HEADER_OFFSET: usize = COFF_OFFSET + 20;
    const DATA_DIRECTORY_OFFSET: usize = OPTIONAL_HEADER_OFFSET + 112;
    const RESOURCE_DIRECTORY_INDEX: usize = 2;
    const SECTION_TABLE_OFFSET: usize = OPTIONAL_HEADER_OFFSET + 0xf0;
    const SECTION_HEADER_LEN: usize = 40;
    const RESOURCE_RVA: u32 = 0x2000;
    const RESOURCE_DATA_OFFSET: usize = 88;

    fn align4(bytes: &mut Vec<u8>) {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
    }

    let mut fixed = vec![0u8; 52];
    fixed[0..4].copy_from_slice(&0xfeef_04bdu32.to_le_bytes());
    fixed[8..12].copy_from_slice(&((1u32 << 16) | sdk_line).to_le_bytes());
    fixed[12..16].copy_from_slice(&(1u32 << 16).to_le_bytes());

    let mut version_blob = Vec::new();
    version_blob.extend_from_slice(&0u16.to_le_bytes());
    version_blob.extend_from_slice(&(fixed.len() as u16).to_le_bytes());
    version_blob.extend_from_slice(&0u16.to_le_bytes());
    version_blob.extend("VS_VERSION_INFO".encode_utf16().flat_map(u16::to_le_bytes));
    version_blob.extend_from_slice(&0u16.to_le_bytes());
    align4(&mut version_blob);
    version_blob.extend_from_slice(&fixed);
    let version_blob_len = version_blob.len() as u16;
    version_blob[0..2].copy_from_slice(&version_blob_len.to_le_bytes());

    let mut resource = vec![0u8; RESOURCE_DATA_OFFSET];
    resource.extend_from_slice(&version_blob);
    resource[14..16].copy_from_slice(&1u16.to_le_bytes());
    resource[16..20].copy_from_slice(&16u32.to_le_bytes());
    resource[20..24].copy_from_slice(&(0x8000_0000u32 | 24).to_le_bytes());
    resource[24 + 14..24 + 16].copy_from_slice(&1u16.to_le_bytes());
    resource[40..44].copy_from_slice(&1u32.to_le_bytes());
    resource[44..48].copy_from_slice(&(0x8000_0000u32 | 48).to_le_bytes());
    resource[48 + 14..48 + 16].copy_from_slice(&1u16.to_le_bytes());
    resource[64..68].copy_from_slice(&1033u32.to_le_bytes());
    resource[68..72].copy_from_slice(&72u32.to_le_bytes());
    resource[72..76].copy_from_slice(&(RESOURCE_RVA + RESOURCE_DATA_OFFSET as u32).to_le_bytes());
    resource[76..80].copy_from_slice(&(version_blob.len() as u32).to_le_bytes());

    let mut bytes = super::runtime_compatibility::synthetic_d3d12_executable(sdk_line);
    bytes[COFF_OFFSET + 2..COFF_OFFSET + 4].copy_from_slice(&2u16.to_le_bytes());
    let resource_directory =
        DATA_DIRECTORY_OFFSET + RESOURCE_DIRECTORY_INDEX * std::mem::size_of::<[u32; 2]>();
    bytes[resource_directory..resource_directory + 4].copy_from_slice(&RESOURCE_RVA.to_le_bytes());
    bytes[resource_directory + 4..resource_directory + 8]
        .copy_from_slice(&(resource.len() as u32).to_le_bytes());

    let resource_raw_pointer = bytes.len().div_ceil(0x200) * 0x200;
    let resource_header = SECTION_TABLE_OFFSET + SECTION_HEADER_LEN;
    bytes[resource_header..resource_header + 8].copy_from_slice(b".rsrc\0\0\0");
    bytes[resource_header + 8..resource_header + 12]
        .copy_from_slice(&(resource.len() as u32).to_le_bytes());
    bytes[resource_header + 12..resource_header + 16].copy_from_slice(&RESOURCE_RVA.to_le_bytes());
    bytes[resource_header + 16..resource_header + 20]
        .copy_from_slice(&(resource.len() as u32).to_le_bytes());
    bytes[resource_header + 20..resource_header + 24]
        .copy_from_slice(&(resource_raw_pointer as u32).to_le_bytes());
    bytes.resize(resource_raw_pointer + resource.len(), 0);
    bytes[resource_raw_pointer..].copy_from_slice(&resource);
    bytes
}

fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().as_ref()).expect("path ref")
}
