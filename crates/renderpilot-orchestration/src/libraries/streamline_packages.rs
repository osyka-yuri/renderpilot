//! Composes NVIDIA Streamline release packages from the manifest's per-plugin DLLs.
//!
//! Streamline plugins (`sl.*.dll`) must run as a matched set. The remote manifest
//! lists each plugin as its own entry; this module groups them by release version
//! into one multi-file [`LibraryArtifact`] so a single swap can update every
//! installed plugin of that release (mirroring FSR package composition).

use std::collections::BTreeMap;

use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, GraphicsTechnology, LibraryArtifact, Sha256Hash,
    Version,
};

use crate::ServiceError;

use super::composed_package::{ComposedPackage, PACKAGE_SOURCE, manifest_member_file};
use super::library_error;
use super::types::LibraryManifestEntry;

/// Whether a manifest library file name is a Streamline plugin DLL.
pub(super) fn is_streamline_file_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.starts_with("sl.") && lower.ends_with(".dll")
}

/// Composes one multi-file Streamline package per semantic release and build type.
///
/// Members are ordered by file name so `sl.common.dll` is the representative
/// (detection's non-FSR order), matching what the UI shows as the bundle version.
pub(super) fn compose_streamline_packages(
    entries: &[LibraryManifestEntry],
) -> Result<Vec<ComposedPackage>, ServiceError> {
    let mut by_release: BTreeMap<(Version, String), Vec<&LibraryManifestEntry>> = BTreeMap::new();

    for entry in entries {
        if !is_streamline_file_name(&entry.library.file_name) {
            continue;
        }
        let version = Version::parse(&entry.version.value).map_err(|error| {
            library_error(format!(
                "invalid Streamline version for {}: {error}",
                entry.entry_id
            ))
        })?;
        let key = (version, entry.build.build_type.to_ascii_lowercase());
        by_release.entry(key).or_default().push(entry);
    }

    let mut packages = Vec::new();
    for members in by_release.into_values() {
        // A single manifest plugin is already represented by its ordinary
        // single-file artifact. Only a real multi-plugin release needs a
        // composed artifact.
        if members.len() >= 2 {
            packages.push(build_package(members)?);
        }
    }
    Ok(packages)
}

fn build_package(mut members: Vec<&LibraryManifestEntry>) -> Result<ComposedPackage, ServiceError> {
    if members.len() < 2 {
        return Err(library_error(
            "Streamline package composition requires at least two plugin members",
        ));
    }

    members.sort_by(|left, right| {
        left.library
            .file_name
            .to_ascii_lowercase()
            .cmp(&right.library.file_name.to_ascii_lowercase())
    });

    let mut files = Vec::with_capacity(members.len());
    let mut member_entry_ids = Vec::with_capacity(members.len());

    for entry in &members {
        files.push(manifest_member_file(entry, None)?);
        member_entry_ids.push(entry.entry_id.clone());
    }

    let shas: Option<Vec<&Sha256Hash>> = files.iter().map(ComponentFile::sha256).collect();
    let shas = shas.ok_or_else(|| library_error("Streamline package member lacks SHA-256"))?;
    let artifact = LibraryArtifact::new(
        ArtifactId::for_bundle(shas),
        GraphicsTechnology::NvidiaStreamline,
        &members[0].library.file_name,
        files,
        ArtifactTrustLevel::ManifestDownloaded,
    )
    .map_err(|error| library_error(format!("failed to build Streamline package: {error}")))?
    .with_source(PACKAGE_SOURCE)
    .map_err(|error| library_error(format!("failed to tag Streamline package source: {error}")))?;

    ComposedPackage::new(artifact, member_entry_ids)
}

#[cfg(test)]
mod tests {
    use super::super::types::{
        BuildInfo, DllFileInfo, FilesInfo, HashesInfo, LibraryInfo, SignatureInfo, VersionInfo,
        ZstFileInfo,
    };
    use super::*;

    fn member_entry(
        entry_id: &str,
        file_name: &str,
        version_value: &str,
        sort_key: &str,
        sha256: &str,
    ) -> LibraryManifestEntry {
        LibraryManifestEntry {
            entry_id: entry_id.to_owned(),
            library: LibraryInfo {
                id: file_name.trim_end_matches(".dll").replace('.', "_"),
                file_name: file_name.to_owned(),
            },
            version: VersionInfo {
                value: version_value.to_owned(),
                sort_key: sort_key.to_owned(),
            },
            build: BuildInfo {
                build_type: "release".to_owned(),
                label: None,
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
                    download_url: "https://example.test/sl.dll.zst".to_owned(),
                },
            },
            signature: SignatureInfo::Unsigned,
        }
    }

    #[test]
    fn composes_one_package_per_version_with_name_order() {
        let entries = vec![
            member_entry(
                "sl_interposer_2_9",
                "sl.interposer.dll",
                "2.9.0.0",
                "0002.0009.0000.0000",
                &"a".repeat(64),
            ),
            member_entry(
                "sl_common_2_9",
                "sl.common.dll",
                "2.9.0.0",
                "0002.0009.0000.0000",
                &"b".repeat(64),
            ),
            member_entry(
                "sl_dlss_2_9",
                "sl.dlss.dll",
                "2.9.0.0",
                "0002.0009.0000.0000",
                &"c".repeat(64),
            ),
            // Different version → second package.
            member_entry(
                "sl_common_2_8",
                "sl.common.dll",
                "2.8.0.0",
                "0002.0008.0000.0000",
                &"d".repeat(64),
            ),
            member_entry(
                "sl_interposer_2_8",
                "sl.interposer.dll",
                "2.8.0",
                "0002.0008.0000.0000",
                &"e".repeat(64),
            ),
        ];

        let packages = compose_streamline_packages(&entries).expect("valid package composition");
        assert_eq!(packages.len(), 2);

        let v29 = packages
            .iter()
            .find(|package| package.artifact.version().map(|v| v.as_str()) == Some("2.9.0.0"))
            .expect("2.9 package");
        assert_eq!(v29.artifact.file_name(), "sl.common.dll");
        assert_eq!(v29.artifact.files().len(), 3);
        assert_eq!(
            v29.artifact
                .files()
                .iter()
                .filter_map(|file| file.path().as_str().strip_prefix("manifest://"))
                .collect::<Vec<_>>(),
            vec!["sl_common_2_9", "sl_dlss_2_9", "sl_interposer_2_9"]
        );
        assert_eq!(
            v29.member_entry_ids,
            vec![
                "sl_common_2_9".to_owned(),
                "sl_dlss_2_9".to_owned(),
                "sl_interposer_2_9".to_owned()
            ]
        );
        // Install targets come from library file names, not virtual entry ids.
        assert_eq!(
            v29.artifact
                .files()
                .iter()
                .map(|file| file.install_as().expect("install target"))
                .collect::<Vec<_>>(),
            vec!["sl.common.dll", "sl.dlss.dll", "sl.interposer.dll"]
        );
    }

    #[test]
    fn rejects_case_insensitive_duplicate_install_targets() {
        let entries = vec![
            member_entry(
                "sl_common_lower",
                "sl.common.dll",
                "2.9.0.0",
                "0002.0009.0000.0000",
                &"a".repeat(64),
            ),
            member_entry(
                "sl_common_upper",
                "SL.COMMON.dll",
                "2.9.0.0",
                "0002.0009.0000.0000",
                &"b".repeat(64),
            ),
        ];

        let error = compose_streamline_packages(&entries)
            .expect_err("same basename with different casing is one install target");
        assert!(
            error
                .to_string()
                .contains("duplicate composed package install target")
        );
    }

    #[test]
    fn ignores_non_streamline_entries() {
        let entries = vec![
            member_entry(
                "sl_common",
                "sl.common.dll",
                "2.9.0.0",
                "0002.0009.0000.0000",
                &"a".repeat(64),
            ),
            LibraryManifestEntry {
                entry_id: "nvngx_dlss".to_owned(),
                library: LibraryInfo {
                    id: "nvngx_dlss".to_owned(),
                    file_name: "nvngx_dlss.dll".to_owned(),
                },
                version: VersionInfo {
                    value: "310.7.0.0".to_owned(),
                    sort_key: "0310.0007.0000.0000".to_owned(),
                },
                build: BuildInfo {
                    build_type: "release".to_owned(),
                    label: None,
                },
                files: FilesInfo {
                    dll: DllFileInfo {
                        size_bytes: 1,
                        hashes: HashesInfo {
                            sha256: "e".repeat(64),
                        },
                    },
                    zst: ZstFileInfo {
                        size_bytes: 1,
                        download_url: "https://example.test/dlss.dll.zst".to_owned(),
                    },
                },
                signature: SignatureInfo::Unsigned,
            },
        ];

        let packages = compose_streamline_packages(&entries).expect("valid package composition");
        assert!(
            packages.is_empty(),
            "single plugins remain single-file artifacts"
        );
    }
}
