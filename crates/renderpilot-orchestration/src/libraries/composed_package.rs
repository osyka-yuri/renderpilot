//! Shared envelope for multi-file manifest packages (FSR, Streamline, …).
//!
//! Technology-specific modules group manifest entries into packages; download
//! orchestration only needs the composed artifact plus member entry ids.

use std::collections::HashSet;

use renderpilot_domain::{ComponentFile, LibraryArtifact, PathRef, Sha256Hash, Version};

use crate::ServiceError;

use super::library_error;
use super::types::LibraryManifestEntry;

/// Source tag attached to every composed package artifact.
pub(super) const PACKAGE_SOURCE: &str = "manifest-download";

/// One composed multi-file package: a virtual multi-file artifact plus member
/// entry ids in artifact file order (for download materialization).
#[derive(Debug)]
pub(super) struct ComposedPackage {
    pub artifact: LibraryArtifact,
    pub member_entry_ids: Vec<String>,
}

impl ComposedPackage {
    /// Creates a multi-file package after validating the identities that bind
    /// manifest members to install targets. This is the common invariant for
    /// FSR and Streamline; technology-specific grouping only chooses members.
    pub(super) fn new(
        artifact: LibraryArtifact,
        member_entry_ids: Vec<String>,
    ) -> Result<Self, ServiceError> {
        if artifact.files().len() != member_entry_ids.len() || artifact.files().len() < 2 {
            return Err(library_error(
                "composed package must have one manifest id per at least two files",
            ));
        }

        let mut entry_ids = HashSet::with_capacity(member_entry_ids.len());
        let mut install_targets = HashSet::with_capacity(artifact.files().len());
        for (file, entry_id) in artifact.files().iter().zip(&member_entry_ids) {
            if entry_id.trim().is_empty() || !entry_ids.insert(entry_id.as_str()) {
                return Err(library_error(format!(
                    "duplicate or empty composed package member id: {entry_id}"
                )));
            }
            let target = file
                .install_as()
                .or_else(|| file.path().file_name())
                .ok_or_else(|| {
                    library_error(format!(
                        "composed package member has no install target: {entry_id}"
                    ))
                })?
                .to_ascii_lowercase();
            insert_unique_target(&mut install_targets, target)?;
        }

        Ok(Self {
            artifact,
            member_entry_ids,
        })
    }
}

fn insert_unique_target(targets: &mut HashSet<String>, target: String) -> Result<(), ServiceError> {
    if targets.contains(&target) {
        return Err(library_error(format!(
            "duplicate composed package install target: {target}"
        )));
    }
    targets.insert(target);
    Ok(())
}

/// Builds a package member with a virtual `manifest://` source path.
///
/// The virtual path uses the manifest entry id (unique, stable). The install
/// basename is always recorded in `install_as`: either an explicit rename
/// (FSR loader → game entry-point name) or the library's real DLL file name.
/// That keeps composed-package uniqueness checks honest — two members with
/// different entry ids but the same DLL basename (`sl.common.dll` vs
/// `SL.COMMON.dll`) cannot collapse into one package.
pub(super) fn manifest_member_file(
    entry: &LibraryManifestEntry,
    install_as: Option<&str>,
) -> Result<ComponentFile, ServiceError> {
    let path = PathRef::new(format!("manifest://{}", entry.entry_id))
        .map_err(|error| library_error(format!("invalid manifest member path: {error}")))?;
    let sha256 = Sha256Hash::new(&entry.files.dll.hashes.sha256)
        .map_err(|error| library_error(format!("invalid manifest member SHA-256: {error}")))?;
    let version = Version::parse(&entry.version.value)
        .map_err(|error| library_error(format!("invalid manifest member version: {error}")))?;

    let install_target = install_as.unwrap_or(entry.library.file_name.as_str());
    Ok(ComponentFile::new(path)
        .with_sha256(sha256)
        .with_version(version)
        .with_install_as(install_target))
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{ArtifactId, ArtifactTrustLevel, GraphicsTechnology};

    use super::*;

    fn package(files: Vec<ComponentFile>) -> LibraryArtifact {
        LibraryArtifact::new(
            ArtifactId::new("artifact:package").expect("id"),
            GraphicsTechnology::NvidiaStreamline,
            "sl.common.dll",
            files,
            ArtifactTrustLevel::ManifestDownloaded,
        )
        .expect("artifact")
    }

    /// Virtual path uses a distinct entry id; install target is the real DLL basename.
    fn member(entry_id: &str, install_as: &str) -> ComponentFile {
        ComponentFile::new(PathRef::new(format!("manifest://{entry_id}")).expect("path"))
            .with_sha256(Sha256Hash::new("a".repeat(64)).expect("sha"))
            .with_install_as(install_as)
    }

    #[test]
    fn package_rejects_duplicate_member_ids_and_targets() {
        let duplicate_ids = ComposedPackage::new(
            package(vec![
                member("common-a", "sl.common.dll"),
                member("common-b", "sl.interposer.dll"),
            ]),
            vec!["common".to_owned(), "common".to_owned()],
        )
        .expect_err("ids must be unique");
        assert!(duplicate_ids.to_string().contains("member id"));

        // Different entry ids but the same install basename (case-insensitive).
        let duplicate_targets = ComposedPackage::new(
            package(vec![
                member("common-lower", "sl.common.dll"),
                member("common-upper", "SL.COMMON.dll"),
            ]),
            vec!["common-a".to_owned(), "common-b".to_owned()],
        )
        .expect_err("targets must be unique");
        assert!(duplicate_targets.to_string().contains("install target"));
    }
}
