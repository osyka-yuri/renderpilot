//! Composes AMD FSR release packages from the manifest's individual split DLLs.
//!
//! FSR 3.1.4+ and FSR 4 ship as three DLLs that must run on one matched release:
//! the loader, the upscaler, and the frame-generation library. The loader is
//! installed as the FSR entry-point name the game loads (`amd_fidelityfx_dx12.dll`
//! for DX12 games, `amd_fidelityfx_vk.dll` for Vulkan games), so the upgrade
//! replaces the game's FSR 3.1 entry point and adds the other two. RenderPilot
//! offers the individual DLLs too for native FSR 4 games, while still composing
//! the matched cohesive package for entry-point-lineage upgrades.

use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, GraphicsTechnology, LibraryArtifact, Sha256Hash,
    fsr,
};
use std::collections::BTreeMap;

use crate::ServiceError;

use super::composed_package::{ComposedPackage, PACKAGE_SOURCE, manifest_member_file};
use super::library_error;
use super::types::LibraryManifestEntry;

const LOADER_ID_DX12: &str = "amd_fidelityfx_loader_dx12";
const UPSCALER_ID_DX12: &str = "amd_fidelityfx_upscaler_dx12";
const FRAMEGEN_ID_DX12: &str = "amd_fidelityfx_framegeneration_dx12";

const LOADER_ID_VK: &str = "amd_fidelityfx_loader_vk";
const UPSCALER_ID_VK: &str = "amd_fidelityfx_upscaler_vk";
const FRAMEGEN_ID_VK: &str = "amd_fidelityfx_framegeneration_vk";

/// Role of a split FSR DLL within one release package.
///
/// Built only from the closed set of known manifest library ids below — never
/// from free-form external strings — so assignment is exhaustive without
/// `unreachable!`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FsrMemberRole {
    Loader,
    Upscaler,
    Framegen,
}

#[derive(Default)]
struct ReleaseMembers<'a> {
    loader: Option<&'a LibraryManifestEntry>,
    upscaler: Option<&'a LibraryManifestEntry>,
    framegen: Option<&'a LibraryManifestEntry>,
}

/// Composes one FSR package per release and per API (DX12, Vulkan). Releases are grouped by their shared
/// build number (the last segment of `version.sort_key`); a release needs a loader
/// and an upscaler (frame generation is optional).
pub(super) fn compose_fsr_packages(
    entries: &[LibraryManifestEntry],
) -> Result<Vec<ComposedPackage>, ServiceError> {
    let mut by_release: BTreeMap<(String, &'static str), ReleaseMembers<'_>> = BTreeMap::new();

    for entry in entries {
        let (entry_point_file, role) = match entry.library.id.as_str() {
            LOADER_ID_DX12 => (fsr::ENTRY_POINT_FILE_DX12, FsrMemberRole::Loader),
            UPSCALER_ID_DX12 => (fsr::ENTRY_POINT_FILE_DX12, FsrMemberRole::Upscaler),
            FRAMEGEN_ID_DX12 => (fsr::ENTRY_POINT_FILE_DX12, FsrMemberRole::Framegen),
            LOADER_ID_VK => (fsr::ENTRY_POINT_FILE_VK, FsrMemberRole::Loader),
            UPSCALER_ID_VK => (fsr::ENTRY_POINT_FILE_VK, FsrMemberRole::Upscaler),
            FRAMEGEN_ID_VK => (fsr::ENTRY_POINT_FILE_VK, FsrMemberRole::Framegen),
            _ => continue,
        };

        let slot = by_release
            .entry((release_key(&entry.version.sort_key), entry_point_file))
            .or_default();

        let member = match role {
            FsrMemberRole::Loader => &mut slot.loader,
            FsrMemberRole::Upscaler => &mut slot.upscaler,
            FsrMemberRole::Framegen => &mut slot.framegen,
        };
        if member.is_some() {
            return Err(library_error(format!(
                "duplicate FSR package member for release {} ({})",
                entry.version.sort_key, entry.library.id
            )));
        }
        *member = Some(entry);
    }

    let mut packages = Vec::new();
    for ((_, entry_point_file), members) in by_release {
        if let (Some(upscaler), Some(loader)) = (members.upscaler, members.loader) {
            packages.push(build_package(
                upscaler,
                loader,
                members.framegen,
                entry_point_file,
            )?);
        }
    }
    Ok(packages)
}

/// The release key shared by a loader/upscaler/framegen of one SDK build: the last
/// dotted segment of the sort key (e.g. `0004.0000.0003.0604` → `0604`).
fn release_key(sort_key: &str) -> String {
    sort_key.rsplit('.').next().unwrap_or(sort_key).to_owned()
}

fn build_package(
    upscaler: &LibraryManifestEntry,
    loader: &LibraryManifestEntry,
    framegen: Option<&LibraryManifestEntry>,
    entry_point_file: &str,
) -> Result<ComposedPackage, ServiceError> {
    // The upscaler is the representative (its version is the FSR ML version, e.g.
    // 4.0.3), so it is files[0]. The loader installs as the FSR entry point.
    let mut files = Vec::new();
    let mut member_entry_ids = Vec::new();

    files.push(manifest_member_file(upscaler, None)?);
    member_entry_ids.push(upscaler.entry_id.clone());

    files.push(manifest_member_file(loader, Some(entry_point_file))?);
    member_entry_ids.push(loader.entry_id.clone());

    if let Some(framegen) = framegen {
        files.push(manifest_member_file(framegen, None)?);
        member_entry_ids.push(framegen.entry_id.clone());
    }

    let shas: Option<Vec<&Sha256Hash>> = files.iter().map(ComponentFile::sha256).collect();
    let shas = shas.ok_or_else(|| library_error("FSR package member lacks SHA-256"))?;
    let artifact = LibraryArtifact::new(
        ArtifactId::for_bundle(shas),
        GraphicsTechnology::AmdFsr,
        &upscaler.library.file_name,
        files,
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .map_err(|error| library_error(format!("failed to build FSR package: {error}")))?
    .with_source(PACKAGE_SOURCE)
    .map_err(|error| library_error(format!("failed to tag FSR package source: {error}")))?;

    ComposedPackage::new(artifact, member_entry_ids)
}

#[cfg(test)]
mod tests {
    use super::super::types::{
        BuildInfo, DllFileInfo, FilesInfo, HashesInfo, LibraryInfo, SignatureInfo, VersionInfo,
        ZstFileInfo,
    };
    use super::*;

    /// Builds a minimal manifest entry for one FSR split member.
    fn member_entry(
        entry_id: &str,
        library_id: &str,
        file_name: &str,
        version_value: &str,
        sort_key: &str,
        sha256: &str,
    ) -> LibraryManifestEntry {
        LibraryManifestEntry {
            entry_id: entry_id.to_owned(),
            library: LibraryInfo {
                id: library_id.to_owned(),
                file_name: file_name.to_owned(),
            },
            version: VersionInfo {
                value: version_value.to_owned(),
                sort_key: sort_key.to_owned(),
            },
            build: BuildInfo {
                build_type: "release".to_owned(),
                label: Some("FSR 4".to_owned()),
            },
            files: FilesInfo {
                dll: DllFileInfo {
                    size_bytes: 1,
                    hashes: HashesInfo {
                        sha256: sha256.to_owned(),
                    },
                },
                zst: ZstFileInfo {
                    size_bytes: 1,
                    download_url: "https://example.test/fsr.dll.zst".to_owned(),
                },
            },
            signature: SignatureInfo::Unsigned,
        }
    }

    #[test]
    fn composes_one_package_with_loader_install_as_and_upscaler_primary() {
        let sha_upscaler = "a".repeat(64);
        let sha_loader = "b".repeat(64);
        let sha_framegen = "c".repeat(64);
        // All three share build number `0604`, so they form one release.
        let entries = vec![
            member_entry(
                "e-loader",
                LOADER_ID_DX12,
                "amd_fidelityfx_loader_dx12.dll",
                "2.1.0.604",
                "0002.0001.0000.0604",
                &sha_loader,
            ),
            member_entry(
                "e-upscaler",
                UPSCALER_ID_DX12,
                "amd_fidelityfx_upscaler_dx12.dll",
                "4.0.3",
                "0004.0000.0003.0604",
                &sha_upscaler,
            ),
            member_entry(
                "e-framegen",
                FRAMEGEN_ID_DX12,
                "amd_fidelityfx_framegeneration_dx12.dll",
                "4.0.0",
                "0004.0000.0000.0604",
                &sha_framegen,
            ),
        ];

        let packages = compose_fsr_packages(&entries).expect("valid package composition");
        assert_eq!(packages.len(), 1, "one release composes one package");
        let package = &packages[0];

        // Members are ordered upscaler (primary), loader, framegen.
        assert_eq!(
            package.member_entry_ids,
            vec!["e-upscaler", "e-loader", "e-framegen"]
        );

        let artifact = &package.artifact;
        assert_eq!(artifact.technology(), GraphicsTechnology::AmdFsr);
        assert_eq!(
            artifact.file_name(),
            "amd_fidelityfx_upscaler_dx12.dll",
            "the displayed/primary file is the upscaler (the FSR ML version)"
        );

        let files = artifact.files();
        assert_eq!(files.len(), 3);
        // Upscaler is primary; install target is its real DLL basename.
        assert_eq!(files[0].path().as_str(), "manifest://e-upscaler");
        assert_eq!(
            files[0].install_as(),
            Some("amd_fidelityfx_upscaler_dx12.dll")
        );
        // Loader takes over the FSR entry point.
        assert_eq!(files[1].path().as_str(), "manifest://e-loader");
        assert_eq!(files[1].install_as(), Some(fsr::ENTRY_POINT_FILE_DX12));
        // Frame generation installs under its own DLL basename.
        assert_eq!(files[2].path().as_str(), "manifest://e-framegen");
        assert_eq!(
            files[2].install_as(),
            Some("amd_fidelityfx_framegeneration_dx12.dll")
        );

        // The id is content-stable over the member shas, in artifact file order.
        let hashes: Vec<Sha256Hash> = [
            sha_upscaler.as_str(),
            sha_loader.as_str(),
            sha_framegen.as_str(),
        ]
        .into_iter()
        .map(|hex| Sha256Hash::new(hex).expect("valid sha"))
        .collect();
        assert_eq!(artifact.id(), &ArtifactId::for_bundle(hashes.iter()));
    }

    #[test]
    fn composes_a_package_without_frame_generation() {
        let entries = vec![
            member_entry(
                "e-up",
                UPSCALER_ID_DX12,
                "amd_fidelityfx_upscaler_dx12.dll",
                "4.0.3",
                "0004.0000.0003.0604",
                &"a".repeat(64),
            ),
            member_entry(
                "e-ld",
                LOADER_ID_DX12,
                "amd_fidelityfx_loader_dx12.dll",
                "2.1.0.604",
                "0002.0001.0000.0604",
                &"b".repeat(64),
            ),
        ];

        let packages = compose_fsr_packages(&entries).expect("valid package composition");
        assert_eq!(packages.len(), 1);
        assert_eq!(
            packages[0].artifact.files().len(),
            2,
            "loader + upscaler is a valid package; frame generation is optional"
        );
        assert_eq!(packages[0].member_entry_ids, vec!["e-up", "e-ld"]);
    }

    #[test]
    fn skips_a_release_without_a_loader() {
        // An upscaler + framegen with no loader cannot replace the entry point.
        let entries = vec![
            member_entry(
                "e-up",
                UPSCALER_ID_DX12,
                "amd_fidelityfx_upscaler_dx12.dll",
                "4.0.3",
                "0004.0000.0003.0604",
                &"a".repeat(64),
            ),
            member_entry(
                "e-fg",
                FRAMEGEN_ID_DX12,
                "amd_fidelityfx_framegeneration_dx12.dll",
                "4.0.0",
                "0004.0000.0000.0604",
                &"c".repeat(64),
            ),
        ];

        assert!(
            compose_fsr_packages(&entries)
                .expect("valid package composition")
                .is_empty(),
            "a release without a loader is not a package"
        );
    }

    #[test]
    fn composes_one_package_per_distinct_release() {
        let entries = vec![
            member_entry(
                "e-up-604",
                UPSCALER_ID_DX12,
                "amd_fidelityfx_upscaler_dx12.dll",
                "4.0.3",
                "0004.0000.0003.0604",
                &"a".repeat(64),
            ),
            member_entry(
                "e-ld-604",
                LOADER_ID_DX12,
                "amd_fidelityfx_loader_dx12.dll",
                "2.1.0.604",
                "0002.0001.0000.0604",
                &"b".repeat(64),
            ),
            member_entry(
                "e-up-700",
                UPSCALER_ID_DX12,
                "amd_fidelityfx_upscaler_dx12.dll",
                "4.1.0",
                "0004.0001.0000.0700",
                &"d".repeat(64),
            ),
            member_entry(
                "e-ld-700",
                LOADER_ID_DX12,
                "amd_fidelityfx_loader_dx12.dll",
                "2.2.0.700",
                "0002.0002.0000.0700",
                &"e".repeat(64),
            ),
        ];

        assert_eq!(
            compose_fsr_packages(&entries)
                .expect("valid package composition")
                .len(),
            2,
            "two distinct build numbers compose two packages"
        );
    }
}
