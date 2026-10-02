//! Read-only proof that an absent registration's root contains only exact,
//! currently recorded RenderPilot remnants.

use std::path::{Path, PathBuf};

use renderpilot_domain::{PathRef, Sha256Hash, normalized_path_key};
use renderpilot_storage_sqlite::LocalCleanupOwners;

use crate::fs::VerifiedDir;

use super::presence::{AbsenceEvidence, InstallationPresence, probe_installation_root};

mod claims;
#[cfg(test)]
pub(crate) mod test_support;
mod walker;

pub(crate) use claims::claims_by_key as local_cleanup_claims_by_key;
pub(crate) use claims::local_cleanup_claims;
pub(crate) use claims::{LocalCleanupBlockReason, LocalCleanupCategory, LocalCleanupClaim};
use claims::{claims_by_key, remnant_from_claim};
use walker::{ClaimedTreeScan, TreeEntry, scan_tree_against_claims, tree_digest};

/// Read-only classification of the currently registered root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RetiredRootObservation {
    /// The root is missing below a readable volume or share anchor.
    Missing(AbsenceEvidence),
    /// A complete no-follow walk found only exact remnants and harmless
    /// unclaimed directories.
    /// An empty remnant list is valid only for an already-absent registration
    /// whose completely observed root contains no files or recorded directories.
    ResidueOnly(VerifiedResidueRoot),
    /// The root is readable but contains an unknown, changed, or unowned entry.
    PresentNonResidue { native_root_identity: String },
    /// The root or walk could not be completely observed.
    Indeterminate { reason: String },
}

/// Current native observation of a root containing only finite recorded claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedResidueRoot {
    root: PathRef,
    native_root_identity: String,
    snapshot_digest: Sha256Hash,
    remnants: Vec<VerifiedRemnant>,
    directories: Vec<VerifiedResidueDirectory>,
}

impl VerifiedResidueRoot {
    pub(crate) fn root(&self) -> &PathRef {
        &self.root
    }

    pub(crate) fn native_root_identity(&self) -> &str {
        &self.native_root_identity
    }

    pub(crate) fn snapshot_digest(&self) -> &Sha256Hash {
        &self.snapshot_digest
    }

    pub(crate) fn remnants(&self) -> &[VerifiedRemnant] {
        &self.remnants
    }

    pub(crate) fn directories(&self) -> &[VerifiedResidueDirectory] {
        &self.directories
    }
}

/// One subdirectory observed by the complete no-follow residue proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedResidueDirectory {
    relative_path: PathBuf,
    native_identity: String,
}

impl VerifiedResidueDirectory {
    pub(crate) fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    pub(crate) fn native_identity(&self) -> &str {
        &self.native_identity
    }
}

/// One actual file or recorded directory found by the complete no-follow walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedRemnant {
    path: PathRef,
    native_identity: String,
    digest: Option<Sha256Hash>,
    category: LocalCleanupCategory,
    removable: bool,
}

impl VerifiedRemnant {
    pub(crate) fn path(&self) -> &PathRef {
        &self.path
    }

    pub(crate) fn native_identity(&self) -> &str {
        &self.native_identity
    }

    pub(crate) fn digest(&self) -> Option<&Sha256Hash> {
        self.digest.as_ref()
    }

    pub(crate) const fn category(&self) -> LocalCleanupCategory {
        self.category
    }

    pub(crate) const fn removable(&self) -> bool {
        self.removable
    }
}

/// Observes the registered root and hashes only regular files with a current
/// exact owner claim. Unknown files stop the walk before their contents are
/// read, which avoids rereading large game archives during routine scans.
pub(crate) fn observe_registered_root(owners: &LocalCleanupOwners) -> RetiredRootObservation {
    let root = Path::new(owners.game().install_path().as_str());
    match probe_installation_root(root) {
        InstallationPresence::ConfirmedAbsent { evidence } => {
            return RetiredRootObservation::Missing(evidence);
        }
        InstallationPresence::Indeterminate { reason } => {
            return RetiredRootObservation::Indeterminate { reason };
        }
        InstallationPresence::Present => {}
    }

    let root_authority = match VerifiedDir::open_absolute_components(root, None) {
        Ok(authority) if !authority.identity().trim().is_empty() => authority,
        Ok(_) => {
            return RetiredRootObservation::Indeterminate {
                reason: "native installation root identity is unavailable".to_owned(),
            };
        }
        Err(error) => {
            return RetiredRootObservation::Indeterminate {
                reason: format!("cannot acquire no-follow root authority: {error}"),
            };
        }
    };
    let native_root_identity = root_authority.identity().to_owned();
    drop(root_authority);

    let claims = claims_by_key(owners);
    let tree = match scan_tree_against_claims(root, &native_root_identity, &claims) {
        Ok(ClaimedTreeScan::Complete(tree)) => tree,
        Ok(ClaimedTreeScan::NotOnlyKnownRemnants) => {
            return RetiredRootObservation::PresentNonResidue {
                native_root_identity,
            };
        }
        Err(reason) => return RetiredRootObservation::Indeterminate { reason },
    };

    // Active empty roots do not establish absence. Once already absent, a
    // completely empty root has no remaining canonical local evidence and can
    // use the established ordered collection path.
    if tree.entries.is_empty() && !owners.availability().is_absent() {
        return RetiredRootObservation::PresentNonResidue {
            native_root_identity,
        };
    }

    let snapshot_digest = match tree_digest(&tree) {
        Ok(digest) => digest,
        Err(reason) => return RetiredRootObservation::Indeterminate { reason },
    };
    let remnants = tree
        .entries
        .iter()
        .filter_map(|(relative, entry)| {
            let path = root.join(relative);
            let claim = claims.get(&normalized_path_key(&path_string(&path)))?;
            let (native_identity, digest) = match entry {
                TreeEntry::Directory { native_identity } => {
                    if claim.category() != LocalCleanupCategory::Directory {
                        return None;
                    }
                    (native_identity.clone(), None)
                }
                TreeEntry::File {
                    native_identity,
                    digest,
                } => {
                    if claim.category() == LocalCleanupCategory::Directory {
                        return None;
                    }
                    (native_identity.clone(), Some(digest.clone()))
                }
            };
            Some(remnant_from_claim(claim, native_identity, digest))
        })
        .collect::<Vec<_>>();
    let directories = tree
        .entries
        .iter()
        .filter_map(|(relative_path, entry)| match entry {
            TreeEntry::Directory { native_identity } => Some(VerifiedResidueDirectory {
                relative_path: relative_path.clone(),
                native_identity: native_identity.clone(),
            }),
            TreeEntry::File { .. } => None,
        })
        .collect::<Vec<_>>();
    let has_observed_file = tree
        .entries
        .values()
        .any(|entry| matches!(entry, TreeEntry::File { .. }));
    if remnants.is_empty() && (!owners.availability().is_absent() || has_observed_file) {
        return RetiredRootObservation::PresentNonResidue {
            native_root_identity,
        };
    }

    RetiredRootObservation::ResidueOnly(VerifiedResidueRoot {
        root: owners.game().install_path().clone(),
        native_root_identity,
        snapshot_digest,
        remnants,
        directories,
    })
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
#[path = "residue/tests.rs"]
mod tests;
