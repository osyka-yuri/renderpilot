use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use renderpilot_application::ArtifactRepository;
use renderpilot_domain::{Architecture, ArtifactMetadata, PackageVersion, PeExportSet};
use sha2::{Digest, Sha256};

use super::artifact_builder::{build_catalog_artifact, package_is_supported};
use super::types::{
    LibraryArtifactRecord, LibraryCatalog, LibraryContent, LibraryLegalDocument,
    LibraryLegalDocumentFormat, LibraryLegalDocumentKind, LibraryPackage, LibraryPackageMember,
    LibraryProvenance, LibraryRelease, LibraryReleaseChannel, LibraryTarget, LibraryTransport,
    LibraryVendor, LibraryVendorCatalog, LibraryVendorReference, LibraryVendorSnapshot,
    SignatureInfo,
};

mod legal;

fn physical_artifact(digest: char, file_name: &str) -> LibraryArtifactRecord {
    let sha256 = digest.to_string().repeat(64);
    LibraryArtifactRecord {
        artifact_id: format!("sha256:{sha256}"),
        library_id: file_name.trim_end_matches(".dll").replace('.', "_"),
        file_name: file_name.to_owned(),
        file_version: Some("1.2.3.4".to_owned()),
        pe_named_exports: None,
        architecture: Architecture::X64,
        dll: LibraryContent {
            sha256,
            size_bytes: 16,
        },
        transport: LibraryTransport {
            compression: "zstd".to_owned(),
            object_key: format!(
                "libraries/blobs/sha256/{}.dll.zst",
                digest.to_string().repeat(64)
            ),
            sha256: digest.to_string().repeat(64),
            size_bytes: 8,
        },
        signature: SignatureInfo::Unsigned,
        extensions: None,
    }
}

fn compressed_artifact(dll: &[u8]) -> (LibraryArtifactRecord, Vec<u8>) {
    let payload = zstd::stream::encode_all(dll, 1).expect("zstd fixture");
    let dll_sha256 = hex::encode(Sha256::digest(dll));
    let transport_sha256 = hex::encode(Sha256::digest(&payload));
    let mut artifact = physical_artifact('a', "fixture.dll");
    artifact.artifact_id = format!("sha256:{dll_sha256}");
    artifact.dll.sha256 = dll_sha256;
    artifact.dll.size_bytes = dll.len() as u64;
    artifact.transport.sha256 = transport_sha256.clone();
    artifact.transport.size_bytes = payload.len() as u64;
    artifact.transport.object_key = format!("libraries/blobs/sha256/{transport_sha256}.dll.zst");
    (artifact, payload)
}

fn package(technology: &str) -> LibraryPackage {
    LibraryPackage {
        package_id: "test_package.1.2.3".to_owned(),
        revision_sha256: "c".repeat(64),
        technology: technology.to_owned(),
        variant: "runtime_bundle".to_owned(),
        display_name: "Test Package".to_owned(),
        release: LibraryRelease::new(
            PackageVersion::parse("1.2.3").expect("package version"),
            LibraryReleaseChannel::Stable,
            None,
        ),
        target: LibraryTarget {
            os: "windows".to_owned(),
            architecture: Architecture::X64,
            compatibility: None,
        },
        provenance: None,
        legal_document_ids: Vec::new(),
        members: vec![
            LibraryPackageMember {
                artifact_id: format!("sha256:{}", "a".repeat(64)),
                role: "primary".to_owned(),
                install_as: "primary.dll".to_owned(),
            },
            LibraryPackageMember {
                artifact_id: format!("sha256:{}", "b".repeat(64)),
                role: "support".to_owned(),
                install_as: "support.dll".to_owned(),
            },
        ],
        extensions: None,
    }
}

fn package_revision(package: &LibraryPackage) -> String {
    super::revision::package_revision_sha256(package).expect("canonical package revision")
}

fn vendor() -> LibraryVendorCatalog {
    LibraryVendorCatalog {
        vendor: LibraryVendor {
            id: "nvidia".to_owned(),
            display_name: "NVIDIA".to_owned(),
        },
        generated_at: "2026-07-22T00:00:00Z".to_owned(),
        legal_documents: Vec::new(),
        artifacts: vec![
            physical_artifact('a', "primary.dll"),
            physical_artifact('b', "support.dll"),
        ],
        packages: Vec::new(),
    }
}

fn complete_catalog(mut package: LibraryPackage) -> LibraryCatalog {
    package.revision_sha256 = package_revision(&package);
    let mut nvidia = vendor();
    nvidia.packages = vec![package];
    catalog_with_nvidia(nvidia)
}

fn catalog_with_nvidia(nvidia: LibraryVendorCatalog) -> LibraryCatalog {
    let empty_vendor = |id: &str, display_name: &str| LibraryVendorCatalog {
        vendor: LibraryVendor {
            id: id.to_owned(),
            display_name: display_name.to_owned(),
        },
        generated_at: "2026-07-22T00:00:00Z".to_owned(),
        legal_documents: Vec::new(),
        artifacts: Vec::new(),
        packages: Vec::new(),
    };
    LibraryCatalog {
        schema_version: 1,
        generated_at: "2026-07-22T00:00:00Z".to_owned(),
        vendors: vec![
            empty_vendor("amd", "AMD"),
            empty_vendor("intel", "Intel"),
            empty_vendor("microsoft", "Microsoft"),
            nvidia,
            empty_vendor("valve", "Valve"),
        ],
    }
}

pub(super) fn openvr_catalog(repository: &str) -> LibraryCatalog {
    let dll_bytes = b"openvr-api-fixture";
    let dll_sha256 = hex::encode(Sha256::digest(dll_bytes));
    let mut artifact = physical_artifact('d', renderpilot_domain::openvr::DLL_NAME);
    artifact.artifact_id = format!("sha256:{dll_sha256}");
    artifact.dll.sha256 = dll_sha256;
    artifact.dll.size_bytes = dll_bytes.len() as u64;
    artifact.file_version = None;
    artifact.pe_named_exports = Some(
        PeExportSet::from_canonical_names(vec!["VR Init".to_owned(), "VR_InitInternal".to_owned()])
            .expect("exports"),
    );
    let legal_sha256 = "f".repeat(64);
    let legal_document_id = format!("license.{legal_sha256}");
    let legal_document = LibraryLegalDocument {
        legal_document_id: legal_document_id.clone(),
        kind: LibraryLegalDocumentKind::License,
        title: "OpenVR SDK License".to_owned(),
        format: LibraryLegalDocumentFormat::Text,
        file_name: "LICENSE.txt".to_owned(),
        content: LibraryContent {
            sha256: legal_sha256.clone(),
            size_bytes: 42,
        },
        object_key: format!("libraries/legal/sha256/{legal_sha256}.txt"),
    };

    let mut openvr_package = LibraryPackage {
        package_id: "openvr.1.1.3b.x64".to_owned(),
        revision_sha256: String::new(),
        technology: "openvr".to_owned(),
        variant: "runtime".to_owned(),
        display_name: "OpenVR SDK".to_owned(),
        release: LibraryRelease::new(
            PackageVersion::parse("1.1.3").expect("package version"),
            LibraryReleaseChannel::Stable,
            Some("revision b".to_owned()),
        ),
        target: LibraryTarget {
            os: "windows".to_owned(),
            architecture: Architecture::X64,
            compatibility: None,
        },
        provenance: Some(LibraryProvenance::GithubRelease {
            repository: repository.to_owned(),
            tag: "v1.1.3b".to_owned(),
            commit_sha: "e".repeat(40),
        }),
        legal_document_ids: vec![legal_document_id],
        members: vec![LibraryPackageMember {
            artifact_id: artifact.artifact_id.clone(),
            role: "primary".to_owned(),
            install_as: renderpilot_domain::openvr::DLL_NAME.to_owned(),
        }],
        extensions: None,
    };
    openvr_package.revision_sha256 = package_revision(&openvr_package);

    let mut catalog = complete_catalog(package("nvidia_streamline"));
    let valve = catalog
        .vendors
        .iter_mut()
        .find(|vendor| vendor.vendor.id == "valve")
        .expect("valve");
    valve.legal_documents = vec![legal_document];
    valve.artifacts = vec![artifact];
    valve.packages = vec![openvr_package];
    catalog
}

#[test]
fn explicit_package_contract_builds_one_domain_artifact() {
    let catalog =
        super::resolved::ValidatedCatalog::new(complete_catalog(package("nvidia_streamline")))
            .expect("validated catalog");
    let resolved = catalog.packages().next().expect("resolved package");
    let artifact = build_catalog_artifact(&resolved, None)
        .expect("valid adapter")
        .expect("known technology");

    assert_eq!(artifact.id(), resolved.artifact_id());
    assert_eq!(artifact.files().len(), 2);
    assert_eq!(artifact.files()[0].install_as(), Some("primary.dll"));
    assert_eq!(artifact.files()[1].install_as(), Some("support.dll"));
    assert_eq!(artifact.release_version().unwrap().as_str(), "1.2.3");
}

#[test]
fn openvr_catalog_builds_nullable_version_exports_provenance_and_label() {
    let catalog = openvr_catalog(renderpilot_domain::openvr::UPSTREAM_REPOSITORY);

    let wire = serde_json::to_value(&catalog).expect("catalog json");
    let valve_wire = wire["vendors"]
        .as_array()
        .expect("vendors")
        .iter()
        .find(|vendor| vendor["vendor"]["id"] == "valve")
        .expect("valve wire");
    assert!(valve_wire["artifacts"][0]["file_version"].is_null());
    assert_eq!(
        valve_wire["artifacts"][0]["pe_named_exports"],
        serde_json::json!(["VR Init", "VR_InitInternal"])
    );
    assert_eq!(valve_wire["packages"][0]["release"]["label"], "revision b");
    assert_eq!(
        valve_wire["packages"][0]["legal_document_ids"][0],
        valve_wire["legal_documents"][0]["legal_document_id"]
    );
    assert_eq!(
        valve_wire["packages"][0]["provenance"]["repository"],
        renderpilot_domain::openvr::UPSTREAM_REPOSITORY
    );

    let round_tripped = serde_json::from_value::<LibraryCatalog>(wire).expect("catalog round trip");
    let catalog = super::resolved::ValidatedCatalog::new(round_tripped).expect("catalog");
    let resolved = catalog
        .packages()
        .find(|resolved| resolved.package().technology == "openvr")
        .expect("OpenVR package");
    let built = build_catalog_artifact(&resolved, None)
        .expect("adapter")
        .expect("known technology");

    assert_eq!(
        built.technology(),
        renderpilot_domain::GraphicsTechnology::OpenVr
    );
    assert_eq!(built.files()[0].version(), None);
    assert_eq!(
        built.files()[0]
            .pe_compatibility()
            .expect("profile")
            .architecture(),
        Architecture::X64
    );
    assert_eq!(
        built.files()[0]
            .pe_compatibility()
            .expect("profile")
            .named_exports()
            .names(),
        &["VR Init".to_owned(), "VR_InitInternal".to_owned()]
    );
    assert_eq!(built.metadata().release_label(), Some("revision b"));
    assert_eq!(
        built
            .metadata()
            .upstream_package()
            .expect("provenance")
            .provider(),
        renderpilot_domain::UpstreamPackageProvider::GitHub
    );
}

#[test]
fn unknown_technology_is_skipped_without_guessing_from_file_names() {
    let catalog =
        super::resolved::ValidatedCatalog::new(complete_catalog(package("future_vendor_feature")))
            .expect("validated catalog");
    let resolved = catalog.packages().next().expect("resolved package");
    assert!(
        build_catalog_artifact(&resolved, None)
            .expect("unknown semantic family is not malformed")
            .is_none()
    );
}

#[test]
fn validated_catalog_resolves_package_members_once() {
    let catalog =
        super::resolved::ValidatedCatalog::new(complete_catalog(package("nvidia_streamline")))
            .expect("validated catalog");
    let packages = catalog.packages().collect::<Vec<_>>();

    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package().package_id, "test_package.1.2.3");
    assert_eq!(packages[0].members().len(), 2);
}

#[test]
fn package_summary_uses_primary_integrity_and_total_member_size() {
    let catalog =
        super::resolved::ValidatedCatalog::new(complete_catalog(package("nvidia_streamline")))
            .expect("validated catalog");
    let resolved = catalog.packages().next().expect("resolved package");
    let artifact = build_catalog_artifact(&resolved, None)
        .expect("valid adapter")
        .expect("known technology");

    let entry = super::inventory::InventoryEntry {
        package_id: resolved.package().package_id.clone(),
        active: Some(artifact),
        local: None,
        local_state: super::types::LibraryLocalState::Absent,
    };
    let summary = super::projection::package_summary(&entry).expect("package summary");

    assert_eq!(summary.primary_file_name, "primary.dll");
    assert_eq!(summary.primary_sha256, "a".repeat(64));
    assert_eq!(summary.size_bytes, 32);
    assert_eq!(summary.local_state, super::types::LibraryLocalState::Absent);
}

#[test]
fn active_catalog_backfills_a_legacy_download_without_a_receipt() {
    let catalog =
        super::resolved::ValidatedCatalog::new(complete_catalog(package("nvidia_streamline")))
            .expect("validated catalog");
    let resolved = catalog.packages().next().expect("resolved package");
    let active = build_catalog_artifact(&resolved, None)
        .expect("valid adapter")
        .expect("known technology");
    let mut legacy_metadata = ArtifactMetadata::default();
    if let Some(release) = active.metadata().release() {
        legacy_metadata = legacy_metadata
            .with_release(
                release.version().clone(),
                release.label().map(str::to_owned),
            )
            .expect("legacy release metadata");
    }
    if let Some(upstream) = active.metadata().upstream_package() {
        legacy_metadata = legacy_metadata.with_upstream_package(upstream.clone());
    }
    if let Some(target) = active.metadata().runtime_target() {
        legacy_metadata = legacy_metadata.with_runtime_target(target.clone());
    }
    let legacy = active.with_metadata(legacy_metadata);
    let context = crate::Context::from_storage(
        renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("storage"),
    );
    context
        .storage()
        .upsert_artifact(&legacy)
        .expect("legacy registration");

    super::inventory::Inventory::load(
        &context,
        Some(&catalog),
        super::types::LibraryCatalogStatus::Active,
    )
    .expect("inventory reconciliation");

    let stored = context
        .storage()
        .list_artifacts()
        .expect("stored artifacts");
    let receipt = stored[0]
        .metadata()
        .catalog_package_receipt()
        .expect("receipt was backfilled");
    assert_eq!(receipt.package_id, resolved.package().package_id);
    assert_eq!(receipt.release.version, resolved.package().release.version);
}

#[test]
fn validated_catalog_keeps_unknown_future_technology_structurally_valid() {
    let catalog =
        super::resolved::ValidatedCatalog::new(complete_catalog(package("future_vendor_feature")))
            .expect("future technology is a runtime policy concern");

    assert_eq!(catalog.packages().len(), 1);
}

#[test]
fn pre_openvr_catalog_cache_remains_valid_for_offline_upgrade() {
    let mut legacy = complete_catalog(package("nvidia_streamline"));
    legacy
        .vendors
        .retain(|vendor| vendor.vendor.id.as_str() != "valve");

    assert!(
        super::resolved::ValidatedCatalog::new(legacy).is_ok(),
        "adding a supported vendor must not invalidate a v1 last-known-good cache"
    );
}

#[test]
fn pre_openvr_client_filters_valve_before_snapshot_fetch_selection() {
    let reference = |vendor_id: &str| LibraryVendorReference {
        vendor_id: vendor_id.to_owned(),
        display_name: vendor_id.to_owned(),
        snapshot_key: format!("libraries/v1/vendors/{vendor_id}/{}.json", "a".repeat(64)),
        snapshot_sha256: "a".repeat(64),
        snapshot_size_bytes: 1,
    };
    let index = super::types::LibraryIndex {
        schema_version: 1,
        generated_at: "2026-07-23T00:00:00Z".to_owned(),
        vendors: ["amd", "intel", "microsoft", "nvidia", "valve"]
            .map(reference)
            .into(),
    };
    let old_supported = ["amd", "intel", "microsoft", "nvidia"];
    let (selected, skipped) = super::catalog::partition_vendor_references(&index, |vendor| {
        old_supported.contains(&vendor)
    });

    assert_eq!(selected.len(), 4);
    assert_eq!(
        skipped
            .iter()
            .map(|reference| reference.vendor_id.as_str())
            .collect::<Vec<_>>(),
        ["valve"]
    );
}

#[test]
fn current_client_filters_unknown_vendor_before_snapshot_fetch_selection() {
    let reference = |vendor_id: &str| LibraryVendorReference {
        vendor_id: vendor_id.to_owned(),
        display_name: vendor_id.to_owned(),
        snapshot_key: format!("libraries/v1/vendors/{vendor_id}/{}.json", "a".repeat(64)),
        snapshot_sha256: "a".repeat(64),
        snapshot_size_bytes: 1,
    };
    let index = super::types::LibraryIndex {
        schema_version: 1,
        generated_at: "2026-07-23T00:00:00Z".to_owned(),
        vendors: ["amd", "intel", "microsoft", "nvidia", "valve", "future"]
            .map(reference)
            .into(),
    };
    let (selected, skipped) =
        super::catalog::partition_vendor_references(&index, super::validation::is_supported_vendor);

    assert_eq!(
        selected
            .iter()
            .map(|reference| reference.vendor_id.as_str())
            .collect::<Vec<_>>(),
        ["amd", "intel", "microsoft", "nvidia", "valve"]
    );
    assert_eq!(
        skipped
            .iter()
            .map(|reference| reference.vendor_id.as_str())
            .collect::<Vec<_>>(),
        ["future"]
    );
}

#[test]
fn github_provenance_syntax_is_generic_but_openvr_repository_is_exact() {
    let mut generic = package("nvidia_streamline");
    generic.provenance = Some(LibraryProvenance::GithubRelease {
        repository: "example/runtime".to_owned(),
        tag: "v1.2.3".to_owned(),
        commit_sha: "d".repeat(40),
    });
    generic.revision_sha256 = package_revision(&generic);
    assert!(
        super::resolved::ValidatedCatalog::new(complete_catalog(generic)).is_ok(),
        "generic GitHub provenance must not be coupled to OpenVR"
    );

    assert!(super::resolved::ValidatedCatalog::new(openvr_catalog("example/openvr")).is_err());
}

#[test]
fn catalog_rejects_a_blank_release_label() {
    let mut catalog = openvr_catalog(renderpilot_domain::openvr::UPSTREAM_REPOSITORY);
    let package = &mut catalog
        .vendors
        .iter_mut()
        .find(|vendor| vendor.vendor.id == "valve")
        .expect("Valve vendor")
        .packages[0];
    package.release.label = Some("  ".to_owned());

    assert!(super::resolved::ValidatedCatalog::new(catalog).is_err());
}

#[test]
fn refresh_fetch_parse_and_activation_failures_return_the_validated_last_known_good_catalog() {
    let directory = tempfile::tempdir().expect("catalog storage");
    let storage = super::storage::LibraryStorage::from_root(directory.path().join("libraries"));
    let cached =
        super::resolved::ValidatedCatalog::new(complete_catalog(package("nvidia_streamline")))
            .expect("validated cache");
    super::catalog::save_catalog(&storage, &cached).expect("cache last-known-good catalog");

    let selected = super::catalog::complete_refresh(
        &storage,
        Err(crate::ServiceError::command_failed("remote unavailable")),
        super::catalog::save_catalog,
    )
    .expect("fetch failure should select last-known-good");
    assert!(!selected.activated);

    let parse_failure = super::catalog::parse_catalog(b"{not valid catalog json");
    let selected =
        super::catalog::complete_refresh(&storage, parse_failure, super::catalog::save_catalog)
            .expect("parse failure should select last-known-good");
    assert!(!selected.activated);
    assert_eq!(
        selected
            .catalog
            .packages()
            .next()
            .expect("cached package")
            .package()
            .package_id,
        "test_package.1.2.3"
    );

    let mut fresh_catalog = complete_catalog(package("nvidia_streamline"));
    fresh_catalog.generated_at = "2026-07-23T00:00:00Z".to_owned();
    let fresh = super::resolved::ValidatedCatalog::new(fresh_catalog).expect("fresh catalog");
    let selected = super::catalog::complete_refresh(&storage, Ok(fresh), |_storage, _catalog| {
        Err(crate::ServiceError::command_failed(
            "activation unavailable",
        ))
    })
    .expect("activation failure should select last-known-good");

    assert!(!selected.activated);
    assert_eq!(
        selected.catalog.as_catalog().generated_at,
        cached.as_catalog().generated_at
    );
}

#[test]
fn invalid_last_known_good_is_quarantined_after_refresh_failure() {
    let directory = tempfile::tempdir().expect("catalog storage");
    let storage = super::storage::LibraryStorage::from_root(directory.path().join("libraries"));
    let path = storage.catalog_cache_path();
    std::fs::create_dir_all(path.parent().expect("catalog parent")).expect("catalog parent");
    std::fs::write(&path, b"{invalid cache").expect("invalid cache");

    let error = super::catalog::complete_refresh(
        &storage,
        Err(crate::ServiceError::command_failed("remote unavailable")),
        super::catalog::save_catalog,
    )
    .expect_err("invalid cache cannot be used as last-known-good");

    assert!(error.to_string().contains("remote unavailable"));
    assert!(!path.exists());
    assert!(
        crate::fs::with_added_extension(&path, "corrupt")
            .expect("quarantine path")
            .is_file()
    );
}

#[test]
fn catalog_validation_rejects_duplicate_package_ids_within_and_across_vendors() {
    let mut within_vendor = complete_catalog(package("nvidia_streamline"));
    let nvidia = within_vendor
        .vendors
        .iter_mut()
        .find(|vendor| vendor.vendor.id == "nvidia")
        .expect("NVIDIA vendor");
    nvidia.packages.push(nvidia.packages[0].clone());
    assert!(
        super::resolved::ValidatedCatalog::new(within_vendor)
            .expect_err("duplicate package within a vendor")
            .to_string()
            .contains("duplicate package")
    );

    let mut across_vendors = complete_catalog(package("nvidia_streamline"));
    let nvidia = across_vendors
        .vendors
        .iter()
        .find(|vendor| vendor.vendor.id == "nvidia")
        .expect("NVIDIA vendor")
        .clone();
    let amd = across_vendors
        .vendors
        .iter_mut()
        .find(|vendor| vendor.vendor.id == "amd")
        .expect("AMD vendor");
    amd.artifacts = nvidia.artifacts;
    amd.packages = nvidia.packages;
    assert!(
        super::resolved::ValidatedCatalog::new(across_vendors)
            .expect_err("duplicate package across vendors")
            .to_string()
            .contains("duplicate package")
    );
}

#[test]
fn vendor_validation_rejects_duplicate_broken_and_orphan_artifacts() {
    let mut duplicate = vendor();
    duplicate.artifacts.push(duplicate.artifacts[0].clone());
    assert!(
        super::resolved::ValidatedCatalog::new(catalog_with_nvidia(duplicate))
            .expect_err("duplicate artifact")
            .to_string()
            .contains("duplicate artifact")
    );

    let mut broken_package = package("nvidia_streamline");
    broken_package.members[1].artifact_id = format!("sha256:{}", "d".repeat(64));
    broken_package.revision_sha256 = package_revision(&broken_package);
    let mut broken = vendor();
    broken.packages = vec![broken_package];
    assert!(
        super::resolved::ValidatedCatalog::new(catalog_with_nvidia(broken))
            .expect_err("broken reference")
            .to_string()
            .contains("missing artifact")
    );

    let mut orphan_vendor = vendor();
    orphan_vendor
        .artifacts
        .push(physical_artifact('c', "orphan.dll"));
    let mut valid_package = package("nvidia_streamline");
    valid_package.revision_sha256 = package_revision(&valid_package);
    orphan_vendor.packages = vec![valid_package];
    assert!(
        super::resolved::ValidatedCatalog::new(catalog_with_nvidia(orphan_vendor))
            .expect_err("orphan artifact")
            .to_string()
            .contains("unreferenced artifact")
    );
}

#[test]
fn vendor_validation_rejects_package_member_architecture_drift() {
    let mut package = package("nvidia_streamline");
    package.target.architecture = Architecture::X86;
    package.revision_sha256 = package_revision(&package);
    let mut vendor_catalog = vendor();
    vendor_catalog.packages = vec![package];

    let error = super::resolved::ValidatedCatalog::new(catalog_with_nvidia(vendor_catalog))
        .expect_err("mixed target architectures must be rejected");
    assert!(error.to_string().contains("mixes target architectures"));
}

#[test]
fn vendor_validation_binds_microsoft_runtime_semantics() {
    let mut dxc = package("microsoft_dxc");
    dxc.revision_sha256 = package_revision(&dxc);
    let mut dxc_vendor = vendor();
    dxc_vendor.packages = vec![dxc];

    let error = super::resolved::ValidatedCatalog::new(catalog_with_nvidia(dxc_vendor))
        .expect_err("Microsoft runtime provenance must be explicit");
    assert!(error.to_string().contains("provenance"));

    let mut generic = package("nvidia_streamline");
    generic.target.compatibility =
        Some(renderpilot_domain::RuntimeCompatibility::D3d12Sdk { version: 2 });
    generic.revision_sha256 = package_revision(&generic);
    let mut generic_vendor = vendor();
    generic_vendor.packages = vec![generic];
    let error = super::resolved::ValidatedCatalog::new(catalog_with_nvidia(generic_vendor))
        .expect_err("compatibility cannot leak to another technology");
    assert!(error.to_string().contains("non-D3D12"));
}

#[test]
fn unsupported_dxc_package_is_not_exposed_as_a_catalog_artifact() {
    let mut dxc = package("microsoft_dxc");
    assert!(!package_is_supported(&dxc));

    dxc.members[0].install_as = "dxcompiler.dll".to_owned();
    dxc.members[1].install_as = "dxil.dll".to_owned();
    assert!(package_is_supported(&dxc));
}

#[test]
fn vendor_snapshot_envelope_binds_the_index_identity() {
    let reference = LibraryVendorReference {
        vendor_id: "nvidia".to_owned(),
        display_name: "NVIDIA".to_owned(),
        snapshot_key: format!("libraries/v1/vendors/nvidia/{}.json", "d".repeat(64)),
        snapshot_sha256: "d".repeat(64),
        snapshot_size_bytes: 1,
    };
    let vendor_catalog = vendor();
    let mut snapshot = LibraryVendorSnapshot {
        schema_version: 1,
        vendor: vendor_catalog.vendor,
        generated_at: vendor_catalog.generated_at,
        legal_documents: vendor_catalog.legal_documents,
        artifacts: vendor_catalog.artifacts,
        packages: Vec::new(),
    };

    super::validation::validate_vendor_snapshot_envelope(&snapshot, &reference)
        .expect("matching snapshot envelope");
    snapshot.vendor.display_name = "Different".to_owned();
    assert!(
        super::validation::validate_vendor_snapshot_envelope(&snapshot, &reference)
            .expect_err("snapshot identity drift")
            .to_string()
            .contains("identity")
    );
}

#[test]
fn package_revision_matches_the_producer_canonical_json_contract() {
    let package: LibraryPackage = serde_json::from_str(
        r#"{
          "package_id":"amd_fidelityfx_dx12_1.0.0.36208",
          "revision_sha256":"448c967868c2a24ea56bef3d89da9eb43cb819a120deb7d57e71338320a5fd61",
          "technology":"amd_fsr",
          "variant":"dx12_runtime",
          "display_name":"AMD FidelityFX Super Resolution",
          "release":{"version":"1.0.0.36208","channel":"beta","label":"FSR 3.1.0"},
          "target":{"os":"windows","architecture":"X64"},
          "members":[{
            "artifact_id":"sha256:602d24510583b74e6a660241009ebe702f0578ada8e8cc76243b3b2d6cb51b79",
            "role":"primary",
            "install_as":"amd_fidelityfx_dx12.dll"
          }]
        }"#,
    )
    .expect("producer package fixture");

    assert_eq!(package_revision(&package), package.revision_sha256);
}

#[test]
fn package_revision_ignores_presentation_metadata_but_binds_channel() {
    let mut package = package("nvidia_streamline");
    package.release.label = Some("Original annotation".to_owned());
    let original = package_revision(&package);

    package.display_name = "Renamed package".to_owned();
    package.release.label = Some("Updated annotation".to_owned());
    let reworded = package_revision(&package);
    assert_eq!(original, reworded);

    package.release.channel = LibraryReleaseChannel::Beta;
    let beta = package_revision(&package);
    assert_ne!(original, beta);
}

#[test]
fn nuget_package_revision_matches_the_producer_contract() {
    let package: LibraryPackage = serde_json::from_str(
        r#"{
          "package_id":"d3d12_agility.1.4.9.x64",
          "revision_sha256":"419f94933726423b0a2e0ca5c3ddfd64930fe3d461c382df6e41b90810423b67",
          "technology":"d3d12_agility",
          "variant":"runtime",
          "display_name":"Microsoft D3D12 Agility SDK",
          "release":{"version":"1.4.9","channel":"stable","label":null},
          "target":{"os":"windows","architecture":"X64","compatibility":{"kind":"d3d12_sdk","version":4}},
          "provenance":{
            "kind":"nuget",
            "package_id":"Microsoft.Direct3D.D3D12",
            "version":"1.4.9",
            "package_sha512":"37hQ83k/y2vu2TUhGSD0Uqlm1rvIHHxwSRVFsoM05WZ3G2Rh/JboSY1mi7op1sa6KyMXNjLpVN95y1yASJqGbg=="
          },
          "members":[{
            "artifact_id":"sha256:2ad0f827d9ecbf3b4cf7d2e0016c4dd2c5560496739f1daed4746261f876be2d",
            "role":"primary",
            "install_as":"D3D12Core.dll"
          }]
        }"#,
    )
    .expect("producer NuGet package fixture");

    assert_eq!(package_revision(&package), package.revision_sha256);
}

#[test]
fn microsoft_preview_package_preserves_exact_identity_and_sdk_line() {
    let package: LibraryPackage = serde_json::from_str(
        r#"{
          "package_id":"d3d12_agility.1.721.2-preview.x64",
          "revision_sha256":"b998fa89635f393cb58534a12a1c70edc9739b304d8c4a3cd64d112bd6e36a87",
          "technology":"d3d12_agility",
          "variant":"runtime",
          "display_name":"Microsoft D3D12 Agility SDK",
          "release":{"version":"1.721.2-preview","channel":"preview","label":null},
          "target":{"os":"windows","architecture":"X64","compatibility":{"kind":"d3d12_sdk","version":721}},
          "provenance":{
            "kind":"nuget",
            "package_id":"Microsoft.Direct3D.D3D12",
            "version":"1.721.2-preview",
            "package_sha512":"p3O3y+3WsciKgZ9MCqi3je/e+i0f0Vo/zx4T+1cdbmr4ph7XsWFRMP5f73fEVdhdorlA2ZQFouxmIfc9DoIw7w=="
          },
          "legal_document_ids":["license.b79425b6d54d8dc63971f2b2291441bcbcba75d878d8bdd4fd1d43046c05e0c0"],
          "members":[{
            "artifact_id":"sha256:8eef346be7f070cdc5804316acd3395151c6567d03be8aeca971171271e82d8e",
            "role":"primary",
            "install_as":"D3D12Core.dll"
          }]
        }"#,
    )
    .expect("generated Microsoft preview package");

    assert_eq!(package.release.version.as_str(), "1.721.2-preview");
    assert_eq!(package.release.version.numeric_core().as_str(), "1.721.2");
    assert_eq!(package.release.channel, LibraryReleaseChannel::Preview);
    assert_eq!(
        package.target.compatibility,
        Some(renderpilot_domain::RuntimeCompatibility::D3d12Sdk { version: 721 })
    );
    assert!(matches!(
        &package.provenance,
        Some(LibraryProvenance::Nuget { version, .. })
            if version.as_str() == "1.721.2-preview"
    ));
    assert_eq!(package_revision(&package), package.revision_sha256);
}

#[test]
fn transport_hash_and_compression_round_trip_exact_bytes() {
    let dll = b"exact graphics runtime bytes";
    let (artifact, payload) = compressed_artifact(dll);

    super::validation::validate_transport(&artifact, &payload).expect("valid transport");
    let decoded =
        super::compression::decompress_library(&artifact, &payload).expect("valid compressed DLL");
    super::validation::validate_dll_hash(&artifact, &decoded).expect("valid DLL digest");
    assert_eq!(decoded, dll);
}

#[test]
fn transport_rejects_size_and_sha256_mismatches() {
    let (mut artifact, payload) = compressed_artifact(b"transport bytes");
    artifact.transport.size_bytes += 1;
    assert!(
        super::validation::validate_transport(&artifact, &payload)
            .expect_err("wrong size")
            .to_string()
            .contains("size mismatch")
    );

    artifact.transport.size_bytes = payload.len() as u64;
    artifact.transport.sha256 = "0".repeat(64);
    assert!(
        super::validation::validate_transport(&artifact, &payload)
            .expect_err("wrong digest")
            .to_string()
            .contains("hash mismatch")
    );
}

#[test]
fn decompression_rejects_corrupt_zstd_and_wrong_output_size() {
    let (artifact, payload) = compressed_artifact(b"compressed bytes");
    assert!(super::compression::decompress_library(&artifact, b"not-zstd").is_err());

    let mut wrong_size = artifact;
    wrong_size.dll.size_bytes += 1;
    assert!(
        super::compression::decompress_library(&wrong_size, &payload)
            .expect_err("wrong decompressed size")
            .to_string()
            .contains("decompressed size mismatch")
    );
}

#[test]
fn valid_cached_transport_is_reused_and_invalid_cache_is_ignored() {
    let (artifact, payload) = compressed_artifact(b"cached bytes");
    let directory = tempfile::tempdir().expect("temp directory");
    let path = directory.path().join("artifact.dll.zst");
    std::fs::write(&path, &payload).expect("cache fixture");

    assert_eq!(
        super::packages::read_valid_archive(&path, &artifact).expect("valid cache"),
        Some(payload)
    );
    std::fs::write(&path, b"broken").expect("invalid cache fixture");
    assert!(
        super::packages::read_valid_archive(&path, &artifact)
            .expect("invalid cache should be recoverable")
            .is_none()
    );
}

#[test]
fn sqlite_registration_is_the_commit_point_and_delete_retains_shared_content() {
    let primary_bytes = b"shared primary runtime";
    let support_bytes = b"shared support runtime";
    let (mut primary, primary_archive) = compressed_artifact(primary_bytes);
    primary.file_name = "primary.dll".to_owned();
    primary.library_id = "primary".to_owned();
    let (mut support, support_archive) = compressed_artifact(support_bytes);
    support.file_name = "support.dll".to_owned();
    support.library_id = "support".to_owned();

    let mut first_package = package("nvidia_streamline");
    first_package.members[0].artifact_id = primary.artifact_id.clone();
    first_package.members[1].artifact_id = support.artifact_id.clone();
    let mut raw_catalog = complete_catalog(first_package);
    let nvidia = raw_catalog
        .vendors
        .iter_mut()
        .find(|vendor| vendor.vendor.id == "nvidia")
        .expect("NVIDIA vendor");
    nvidia.artifacts = vec![primary.clone(), support.clone()];
    let mut second_package = nvidia.packages[0].clone();
    second_package.package_id = "test_package.1.2.3.shared".to_owned();
    second_package.revision_sha256 = package_revision(&second_package);
    nvidia.packages.push(second_package);

    let catalog = super::resolved::ValidatedCatalog::new(raw_catalog).expect("validated catalog");
    let resolved = catalog.packages().collect::<Vec<_>>();
    let directory = tempfile::tempdir().expect("library storage");
    let storage = super::storage::LibraryStorage::from_root(directory.path().join("libraries"));
    let physical_members = [
        (&primary, primary_bytes.as_slice(), primary_archive),
        (&support, support_bytes.as_slice(), support_archive),
    ];
    let mut local_paths = Vec::with_capacity(physical_members.len());
    let mut archive_paths = Vec::with_capacity(physical_members.len());
    for (member, dll_bytes, archive_bytes) in &physical_members {
        let dll_path = storage.local_dll_path(&member.dll.sha256, &member.file_name);
        std::fs::create_dir_all(dll_path.parent().expect("DLL parent")).expect("DLL directory");
        std::fs::write(&dll_path, dll_bytes).expect("DLL content");
        local_paths.push(dll_path);

        let archive_path = storage.local_archive_path(&member.transport.sha256);
        std::fs::create_dir_all(archive_path.parent().expect("archive parent"))
            .expect("archive directory");
        std::fs::write(&archive_path, archive_bytes).expect("archive content");
        archive_paths.push(archive_path);
    }

    let first_artifact = build_catalog_artifact(&resolved[0], Some(&local_paths))
        .expect("valid first adapter")
        .expect("known technology");
    let second_artifact = build_catalog_artifact(&resolved[1], Some(&local_paths))
        .expect("valid second adapter")
        .expect("known technology");
    let context = crate::Context::from_storage(
        renderpilot_storage_sqlite::SqliteStorage::in_memory().expect("storage"),
    );

    let registered =
        super::packages::register_and_commit(&context, resolved[0].package(), &first_artifact)
            .expect("register package");
    assert!(registered.is_downloaded);
    super::packages::register_and_commit(&context, resolved[1].package(), &second_artifact)
        .expect("register package sharing content");
    assert_eq!(
        context.storage().list_artifacts().expect("artifacts").len(),
        2
    );
    let local_entry = super::inventory::InventoryEntry {
        package_id: resolved[0].package().package_id.clone(),
        active: None,
        local: Some(first_artifact.clone()),
        local_state: super::types::LibraryLocalState::Verified,
    };
    let local_only = super::projection::package_summary(&local_entry).expect("catalog receipt");
    assert_eq!(
        local_only.availability,
        super::types::LibraryPackageAvailability::LocalOnly
    );
    assert_eq!(local_only.release.version.as_str(), "1.2.3");

    let mut verifier = super::local_verifier::LocalArtifactVerifier::default();
    assert_eq!(
        verifier.artifact_state(&first_artifact),
        super::types::LibraryLocalState::Verified
    );
    assert_eq!(
        verifier.artifact_state(&second_artifact),
        super::types::LibraryLocalState::Verified
    );
    std::fs::write(&local_paths[0], b"corrupt").expect("corrupt local cache fixture");
    let mut verifier = super::local_verifier::LocalArtifactVerifier::default();
    assert_eq!(
        verifier.artifact_state(&first_artifact),
        super::types::LibraryLocalState::Corrupt,
        "a stale database registration must be classified as corrupt"
    );
    std::fs::write(&local_paths[0], primary_bytes).expect("restore shared content");

    context
        .storage()
        .delete_catalog_package_artifacts(&resolved[0].package().package_id)
        .expect("delete local-only package");
    let remaining = context.storage().list_artifacts().expect("artifacts");
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        remaining[0].id(),
        second_artifact.id(),
        "the package sharing the blobs remains registered"
    );
    for path in local_paths.iter().chain(&archive_paths) {
        assert!(path.is_file(), "shared content survives delete: {path:?}");
    }
}

#[tokio::test]
async fn concurrent_materialization_downloads_a_shared_artifact_once() {
    let directory = tempfile::tempdir().expect("library storage");
    let storage = super::storage::LibraryStorage::from_root(directory.path().join("libraries"));
    let (artifact, payload) = compressed_artifact(b"shared runtime bytes");
    let downloads = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());

    let first = tokio::spawn({
        let storage = storage.clone();
        let artifact = artifact.clone();
        let payload = payload.clone();
        let downloads = Arc::clone(&downloads);
        let entered = Arc::clone(&entered);
        let release = Arc::clone(&release);
        async move {
            super::packages::ensure_artifact_with(&storage, &artifact, None, move || async move {
                downloads.fetch_add(1, Ordering::SeqCst);
                entered.notify_one();
                release.notified().await;
                Ok(payload)
            })
            .await
        }
    });
    entered.notified().await;

    let second = tokio::spawn({
        let storage = storage.clone();
        let artifact = artifact.clone();
        let downloads = Arc::clone(&downloads);
        async move {
            super::packages::ensure_artifact_with(&storage, &artifact, None, move || async move {
                downloads.fetch_add(1, Ordering::SeqCst);
                Ok(payload)
            })
            .await
        }
    });
    tokio::task::yield_now().await;
    release.notify_one();

    let first_path = first
        .await
        .expect("first task")
        .expect("first materialization");
    let second_path = second
        .await
        .expect("second task")
        .expect("second materialization");
    assert_eq!(first_path, second_path);
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read(first_path).expect("materialized DLL"),
        b"shared runtime bytes"
    );
}
