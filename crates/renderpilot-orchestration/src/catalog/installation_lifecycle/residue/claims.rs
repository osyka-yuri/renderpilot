//! Exact local file claims derived from existing canonical owner records.

use std::collections::BTreeMap;

use renderpilot_domain::{
    FileOwnership, ManagedFileBaseline, ManagedFileMode, OptiScalerFileBaseline,
    OptiScalerFileCleanup, OptiScalerReleaseFileBaseline, PathRef, ProxyRootPrestate, Sha256Hash,
    normalized_path_key, normalized_path_relation,
};
use renderpilot_storage_sqlite::LocalCleanupOwners;

use super::VerifiedRemnant;

mod component_sidecars;
mod renodx;

/// Kind of canonical owner that recorded a local file or directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LocalCleanupCategory {
    OptiscalerFile,
    AddonFile,
    ComponentFile,
    PrivateCustody,
    Directory,
}

/// Why a recorded path is visible but cannot be removed by explicit cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LocalCleanupBlockReason {
    UnsupportedOwner,
    PrivateOriginalCustody,
    UnprovenOwnership,
}

/// Finite observation data derived from the canonical local owner rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalCleanupClaim {
    path: PathRef,
    category: LocalCleanupCategory,
    sha256: Option<Sha256Hash>,
    native_identity: Option<String>,
    removable: bool,
    blocked_reason: Option<LocalCleanupBlockReason>,
}

impl LocalCleanupClaim {
    #[must_use]
    pub(crate) fn path(&self) -> &PathRef {
        &self.path
    }

    #[must_use]
    pub(crate) const fn category(&self) -> LocalCleanupCategory {
        self.category
    }

    #[must_use]
    pub(crate) fn sha256(&self) -> Option<&Sha256Hash> {
        self.sha256.as_ref()
    }

    #[must_use]
    pub(crate) fn native_identity(&self) -> Option<&str> {
        self.native_identity.as_deref()
    }

    #[must_use]
    pub(crate) const fn removable(&self) -> bool {
        self.removable
    }

    #[must_use]
    pub(crate) const fn blocked_reason(&self) -> Option<LocalCleanupBlockReason> {
        self.blocked_reason
    }
}

/// Derives exact paths from canonical owner records and finite producer bindings.
/// Filenames or sidecar conventions alone never grant authority.
pub(crate) fn local_cleanup_claims(owners: &LocalCleanupOwners) -> Vec<LocalCleanupClaim> {
    claims_by_key(owners).into_values().collect()
}

pub(crate) fn claims_by_key(owners: &LocalCleanupOwners) -> BTreeMap<String, LocalCleanupClaim> {
    let mut claims = BTreeMap::new();
    let root = owners.game().install_path();

    if let Some(state) = owners.optiscaler_state() {
        for receipt in &state.release_files {
            let baseline_absent =
                matches!(&receipt.baseline, OptiScalerReleaseFileBaseline::Absent);
            let removable = receipt.installed.ownership() == FileOwnership::Owned
                && receipt.cleanup == OptiScalerFileCleanup::RemoveIfUnchanged
                && baseline_absent;
            let blocked_reason = if matches!(
                &receipt.baseline,
                OptiScalerReleaseFileBaseline::RetainedFsrEntryPoint { .. }
            ) {
                Some(LocalCleanupBlockReason::PrivateOriginalCustody)
            } else if removable {
                None
            } else if receipt.installed.ownership() == FileOwnership::Owned {
                Some(LocalCleanupBlockReason::UnsupportedOwner)
            } else {
                Some(LocalCleanupBlockReason::UnprovenOwnership)
            };
            insert_if_below_root(
                &mut claims,
                root,
                LocalCleanupClaim {
                    path: receipt.path.clone(),
                    category: LocalCleanupCategory::OptiscalerFile,
                    sha256: Some(receipt.installed.digest().clone()),
                    native_identity: Some(receipt.installed.identity().to_owned()),
                    removable,
                    blocked_reason,
                },
            );

            if let OptiScalerReleaseFileBaseline::RetainedFsrEntryPoint {
                custody_path,
                original,
                ..
            } = &receipt.baseline
            {
                insert_if_below_root(
                    &mut claims,
                    root,
                    LocalCleanupClaim {
                        path: custody_path.clone(),
                        category: LocalCleanupCategory::PrivateCustody,
                        sha256: Some(original.digest().clone()),
                        native_identity: Some(original.identity().to_owned()),
                        removable: false,
                        blocked_reason: Some(LocalCleanupBlockReason::PrivateOriginalCustody),
                    },
                );
            }
        }

        for binding in &state.runtime_bindings {
            let baseline_absent = matches!(&binding.baseline, OptiScalerFileBaseline::Absent);
            let removable =
                binding.installed.ownership() == FileOwnership::Owned && baseline_absent;
            insert_if_below_root(
                &mut claims,
                root,
                LocalCleanupClaim {
                    path: binding.path.clone(),
                    category: LocalCleanupCategory::OptiscalerFile,
                    sha256: Some(binding.installed.digest().clone()),
                    native_identity: Some(binding.installed.identity().to_owned()),
                    removable,
                    blocked_reason: if removable {
                        None
                    } else if binding.installed.ownership() == FileOwnership::Owned {
                        Some(LocalCleanupBlockReason::UnsupportedOwner)
                    } else {
                        Some(LocalCleanupBlockReason::UnprovenOwnership)
                    },
                },
            );
        }

        for directory in &state.directory_receipts {
            insert_if_below_root(
                &mut claims,
                root,
                LocalCleanupClaim {
                    path: directory.path.clone(),
                    category: LocalCleanupCategory::Directory,
                    sha256: None,
                    native_identity: Some(directory.identity.clone()),
                    removable: true,
                    blocked_reason: None,
                },
            );
        }
    }

    if let Some(topology) = owners.topology() {
        let baseline_absent = topology.root_prestate == ProxyRootPrestate::Absent;
        for link in std::iter::once(&topology.outer).chain(topology.downstream.iter()) {
            let removable = link.receipt.ownership() == FileOwnership::Owned && baseline_absent;
            insert_if_below_root(
                &mut claims,
                root,
                LocalCleanupClaim {
                    path: link.path.clone(),
                    category: LocalCleanupCategory::OptiscalerFile,
                    sha256: Some(link.receipt.digest().clone()),
                    native_identity: Some(link.receipt.identity().to_owned()),
                    removable,
                    blocked_reason: if removable {
                        None
                    } else if link.receipt.ownership() == FileOwnership::Owned {
                        Some(LocalCleanupBlockReason::UnsupportedOwner)
                    } else {
                        Some(LocalCleanupBlockReason::UnprovenOwnership)
                    },
                },
            );
        }
    }

    if let Some(addon) = owners.addon() {
        for file in addon.managed_files() {
            let absent = matches!(file.baseline(), ManagedFileBaseline::Absent);
            let removable = file.mode() == ManagedFileMode::Owned && absent;
            insert_if_below_root(
                &mut claims,
                root,
                LocalCleanupClaim {
                    path: file.path().clone(),
                    category: LocalCleanupCategory::AddonFile,
                    sha256: Some(file.installed_sha256().clone()),
                    native_identity: None,
                    removable,
                    blocked_reason: if removable {
                        None
                    } else if file.mode() == ManagedFileMode::Owned {
                        Some(LocalCleanupBlockReason::UnsupportedOwner)
                    } else {
                        Some(LocalCleanupBlockReason::UnprovenOwnership)
                    },
                },
            );
        }
        renodx::insert_claims(&mut claims, root, addon);
    }

    let executable = owners.game().confirmed_executable();
    for (_component_id, baseline) in owners.baselines() {
        component_sidecars::insert_claims(&mut claims, root, baseline);
        for active in baseline.expected_active_files() {
            let Some(digest) = active.sha256() else {
                continue;
            };
            if executable.is_some_and(|path| same_path(path, active.path()))
                || baseline
                    .d3d12_executable()
                    .is_some_and(|value| same_path(value.executable_path(), active.path()))
            {
                continue;
            }
            let original = baseline
                .files()
                .iter()
                .find(|candidate| same_path(candidate.path(), active.path()));
            let removable = match original {
                None => true,
                Some(original) => original.sha256().is_some_and(|hash| hash != digest),
            };
            insert_if_below_root(
                &mut claims,
                root,
                LocalCleanupClaim {
                    path: active.path().clone(),
                    category: LocalCleanupCategory::ComponentFile,
                    sha256: Some(digest.clone()),
                    native_identity: None,
                    removable,
                    blocked_reason: if removable {
                        None
                    } else if original.is_some_and(|file| file.sha256().is_none()) {
                        Some(LocalCleanupBlockReason::UnprovenOwnership)
                    } else {
                        Some(LocalCleanupBlockReason::UnsupportedOwner)
                    },
                },
            );
        }
    }

    claims
}

fn insert_if_below_root(
    claims: &mut BTreeMap<String, LocalCleanupClaim>,
    root: &PathRef,
    claim: LocalCleanupClaim,
) {
    if normalized_path_relation(root.as_str(), claim.path.as_str())
        != renderpilot_domain::NormalizedPathRelation::LeftAncestor
    {
        return;
    }

    let key = normalized_path_key(claim.path.as_str());
    match claims.entry(key) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(claim);
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            merge_claim(entry.get_mut(), &claim);
        }
    }
}

fn merge_claim(existing: &mut LocalCleanupClaim, additional: &LocalCleanupClaim) {
    let compatible = existing
        .sha256
        .as_ref()
        .zip(additional.sha256.as_ref())
        .is_none_or(|(left, right)| left == right)
        && existing
            .native_identity
            .as_ref()
            .zip(additional.native_identity.as_ref())
            .is_none_or(|(left, right)| left == right);
    if !compatible {
        existing.sha256 = None;
        existing.native_identity = None;
        existing.removable = false;
        existing.blocked_reason = Some(LocalCleanupBlockReason::UnprovenOwnership);
        return;
    }

    if existing.sha256.is_none() {
        existing.sha256.clone_from(&additional.sha256);
    }
    if existing.native_identity.is_none() {
        existing
            .native_identity
            .clone_from(&additional.native_identity);
    }
    if !additional.removable {
        existing.removable = false;
        existing.blocked_reason =
            stronger_reason(existing.blocked_reason, additional.blocked_reason);
    }
}

fn stronger_reason(
    existing: Option<LocalCleanupBlockReason>,
    additional: Option<LocalCleanupBlockReason>,
) -> Option<LocalCleanupBlockReason> {
    use LocalCleanupBlockReason::{PrivateOriginalCustody, UnprovenOwnership, UnsupportedOwner};
    match (existing, additional) {
        (Some(PrivateOriginalCustody), _) | (_, Some(PrivateOriginalCustody)) => {
            Some(PrivateOriginalCustody)
        }
        (Some(UnprovenOwnership), _) | (_, Some(UnprovenOwnership)) => Some(UnprovenOwnership),
        (Some(UnsupportedOwner), _) | (_, Some(UnsupportedOwner)) => Some(UnsupportedOwner),
        (None, None) => None,
    }
}

fn same_path(left: &PathRef, right: &PathRef) -> bool {
    normalized_path_key(left.as_str()) == normalized_path_key(right.as_str())
}

pub(super) fn remnant_from_claim(
    claim: &LocalCleanupClaim,
    native_identity: String,
    digest: Option<Sha256Hash>,
) -> VerifiedRemnant {
    VerifiedRemnant {
        path: claim.path.clone(),
        native_identity,
        digest,
        category: claim.category,
        removable: claim.removable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(
        category: LocalCleanupCategory,
        digest: &str,
        removable: bool,
        blocked_reason: Option<LocalCleanupBlockReason>,
    ) -> LocalCleanupClaim {
        LocalCleanupClaim {
            path: PathRef::new("C:/Games/Example/owned.dll").expect("claim path"),
            category,
            sha256: Some(Sha256Hash::new(digest).expect("claim digest")),
            native_identity: Some("native-file-id".to_owned()),
            removable,
            blocked_reason,
        }
    }

    #[test]
    fn compatible_duplicate_categories_remove_once_but_conflicts_block() {
        let digest = "a".repeat(64);
        let mut merged = claim(LocalCleanupCategory::AddonFile, &digest, true, None);
        merge_claim(
            &mut merged,
            &claim(LocalCleanupCategory::ComponentFile, &digest, true, None),
        );
        assert_eq!(merged.category(), LocalCleanupCategory::AddonFile);
        assert!(merged.removable());

        let mut restricted = claim(LocalCleanupCategory::AddonFile, &digest, true, None);
        merge_claim(
            &mut restricted,
            &claim(
                LocalCleanupCategory::ComponentFile,
                &digest,
                false,
                Some(LocalCleanupBlockReason::UnsupportedOwner),
            ),
        );
        assert!(!restricted.removable());

        let mut conflicting = claim(LocalCleanupCategory::AddonFile, &digest, true, None);
        merge_claim(
            &mut conflicting,
            &claim(
                LocalCleanupCategory::ComponentFile,
                &"b".repeat(64),
                true,
                None,
            ),
        );
        assert!(!conflicting.removable());
        assert_eq!(
            conflicting.blocked_reason(),
            Some(LocalCleanupBlockReason::UnprovenOwnership)
        );

        let mut identity_conflict = claim(LocalCleanupCategory::AddonFile, &digest, true, None);
        let mut different_identity =
            claim(LocalCleanupCategory::ComponentFile, &digest, true, None);
        different_identity.native_identity = Some("different-native-file-id".to_owned());
        merge_claim(&mut identity_conflict, &different_identity);
        assert!(!identity_conflict.removable());
        assert_eq!(
            identity_conflict.blocked_reason(),
            Some(LocalCleanupBlockReason::UnprovenOwnership)
        );
    }
}
