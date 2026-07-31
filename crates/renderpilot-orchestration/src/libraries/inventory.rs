//! One-pass reconciliation of active catalog definitions and local registrations.

use std::collections::{BTreeMap, HashMap, HashSet};

use renderpilot_application::{ActiveCatalogPackage, ArtifactRepository};
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, CatalogPackageAvailability, LibraryArtifact, ReleaseChannel,
};

use crate::ServiceError;

use super::artifact_builder;
use super::local_verifier::LocalArtifactVerifier;
use super::resolved::ValidatedCatalog;
use super::types::{LibraryCatalogStatus, LibraryLocalState, LibraryPackageSummary};

/// Active definition and matching local registration for one logical package.
#[derive(Debug, Clone)]
pub(super) struct InventoryEntry {
    pub(super) package_id: String,
    pub(super) active: Option<LibraryArtifact>,
    pub(super) local: Option<LibraryArtifact>,
    pub(super) local_state: LibraryLocalState,
}

impl InventoryEntry {
    pub(super) fn presentation_artifact(&self) -> Option<&LibraryArtifact> {
        self.active.as_ref().or(self.local.as_ref())
    }

    pub(super) const fn availability(&self) -> CatalogPackageAvailability {
        if self.active.is_some() {
            CatalogPackageAvailability::Available
        } else {
            CatalogPackageAvailability::LocalOnly
        }
    }

    pub(super) fn automatic_selection_allowed(&self) -> bool {
        self.active.as_ref().is_some_and(|artifact| {
            let Some(receipt) = artifact.metadata().catalog_package_receipt() else {
                return false;
            };
            if receipt.release().channel != ReleaseChannel::Stable {
                return false;
            }
            receipt.technology() != "xiph_vorbis"
                || (receipt.composite_provenance().is_some()
                    && artifact.files().iter().all(|file| {
                        file.pe_compatibility()
                            .is_some_and(|profile| profile.imports().is_some())
                    }))
        })
    }
}

/// Shared inventory used by both Libraries and replacement-candidate projections.
pub(crate) struct Inventory {
    entries: Vec<InventoryEntry>,
    uncatalogued_verified: Vec<LibraryArtifact>,
    catalog_status: LibraryCatalogStatus,
}

impl Inventory {
    pub(crate) fn load(
        context: &crate::Context,
        catalog: Option<&ValidatedCatalog>,
        catalog_status: LibraryCatalogStatus,
    ) -> Result<Self, ServiceError> {
        let active_artifacts = match catalog {
            Some(catalog) => artifact_builder::catalog_as_artifacts(catalog)?,
            None => Vec::new(),
        };
        let mut local_artifacts = context
            .storage()
            .list_artifacts()
            .map_err(ServiceError::from)?;
        backfill_legacy_receipts(context, &mut local_artifacts, &active_artifacts);

        let mut verifier = LocalArtifactVerifier::load(context.storage())?;
        let local_states = local_artifacts
            .iter()
            .map(|artifact| (artifact.id().clone(), verifier.artifact_state(artifact)))
            .collect::<HashMap<_, _>>();
        verifier.persist(context.storage())?;

        let uncatalogued_verified = local_artifacts
            .iter()
            .filter(|artifact| artifact.metadata().catalog_package_receipt().is_none())
            .filter(|artifact| {
                local_states.get(artifact.id()) == Some(&LibraryLocalState::Verified)
            })
            .cloned()
            .collect();

        let mut groups = BTreeMap::<String, PackageGroup>::new();
        for active in active_artifacts {
            let package_id = active
                .metadata()
                .catalog_package_receipt()
                .ok_or_else(|| {
                    ServiceError::invalid_input(format!(
                        "active catalog artifact `{}` has no package receipt",
                        active.id().as_str()
                    ))
                })?
                .package_id()
                .to_owned();
            groups.entry(package_id).or_default().active = Some(active);
        }
        for local in local_artifacts {
            let Some(receipt) = local.metadata().catalog_package_receipt() else {
                continue;
            };
            let local_state = *local_states.get(local.id()).ok_or_else(|| {
                ServiceError::invalid_input(format!(
                    "local artifact `{}` was not classified",
                    local.id().as_str()
                ))
            })?;
            groups
                .entry(receipt.package_id().to_owned())
                .or_default()
                .locals
                .push((local, local_state));
        }

        let mut ignored_registrations = 0;
        let mut affected_packages = 0;
        let mut entries = Vec::with_capacity(groups.len());
        for (package_id, group) in groups {
            let (entry, ignored) = resolve_group(package_id, group)?;
            ignored_registrations += ignored;
            affected_packages += usize::from(ignored > 0);
            entries.push(entry);
        }
        if ignored_registrations > 0 {
            log::warn!(
                "stale catalog package registrations ignored; registration_count={ignored_registrations}; package_count={affected_packages}"
            );
        }
        Ok(Self {
            entries,
            uncatalogued_verified,
            catalog_status,
        })
    }

    pub(crate) fn package_output(&self) -> super::types::LibraryPackagesOutput {
        super::types::LibraryPackagesOutput {
            packages: self
                .entries
                .iter()
                .filter_map(super::projection::package_summary)
                .collect(),
            catalog_status: self.catalog_status,
        }
    }

    pub(crate) fn package(&self, package_id: &str) -> Option<LibraryPackageSummary> {
        self.entries
            .iter()
            .find(|entry| entry.package_id == package_id)
            .and_then(super::projection::package_summary)
    }

    pub(crate) fn replacement_projection(
        &self,
    ) -> (
        Vec<LibraryArtifact>,
        HashSet<ArtifactId>,
        HashMap<ArtifactId, ActiveCatalogPackage>,
    ) {
        let mut artifacts = self.uncatalogued_verified.clone();
        let mut downloaded_ids = artifacts
            .iter()
            .map(|artifact| artifact.id().clone())
            .collect::<HashSet<_>>();
        let mut active_catalog = HashMap::new();

        for entry in &self.entries {
            let candidate = match (&entry.active, &entry.local) {
                (_, Some(local)) if entry.local_state == LibraryLocalState::Verified => Some(local),
                (Some(active), _) => Some(active),
                (None, _) => None,
            };
            let Some(candidate) = candidate else {
                continue;
            };
            if entry.local_state == LibraryLocalState::Verified && entry.local.is_some() {
                downloaded_ids.insert(candidate.id().clone());
            }
            if let Some(active) = &entry.active
                && let Some(receipt) = active.metadata().catalog_package_receipt()
            {
                active_catalog.insert(
                    active.id().clone(),
                    ActiveCatalogPackage::from_receipt(
                        receipt,
                        entry.automatic_selection_allowed(),
                    ),
                );
            }
            artifacts.push(candidate.clone());
        }
        (artifacts, downloaded_ids, active_catalog)
    }
}

#[derive(Default)]
struct PackageGroup {
    active: Option<LibraryArtifact>,
    locals: Vec<(LibraryArtifact, LibraryLocalState)>,
}

fn resolve_group(
    package_id: String,
    mut group: PackageGroup,
) -> Result<(InventoryEntry, usize), ServiceError> {
    group
        .locals
        .sort_by(|left, right| left.0.id().cmp(right.0.id()));
    match group.active {
        Some(active) => {
            // Receipt deserialization enforces that a package artifact id is
            // derived from its revision, so comparing both would be redundant.
            let matching_index = group
                .locals
                .iter()
                .position(|(artifact, _)| artifact.id() == active.id());
            let local = matching_index.map(|index| group.locals.remove(index));
            let ignored = group.locals.len();
            let (local, local_state) = match local {
                Some((local, state)) => (Some(local), state),
                None => (None, LibraryLocalState::Absent),
            };
            Ok((
                InventoryEntry {
                    package_id,
                    active: Some(active),
                    local,
                    local_state,
                },
                ignored,
            ))
        }
        None => {
            let ignored = group.locals.len().saturating_sub(1);
            let Some((local, local_state)) = group.locals.into_iter().next() else {
                return Err(ServiceError::invalid_input(format!(
                    "inventory package group `{package_id}` has no artifact"
                )));
            };
            Ok((
                InventoryEntry {
                    package_id,
                    active: None,
                    local: Some(local),
                    local_state,
                },
                ignored,
            ))
        }
    }
}

fn backfill_legacy_receipts(
    context: &crate::Context,
    local_artifacts: &mut [LibraryArtifact],
    active_artifacts: &[LibraryArtifact],
) {
    let active_by_id = active_artifacts
        .iter()
        .map(|artifact| (artifact.id(), artifact))
        .collect::<HashMap<_, _>>();
    let mut batch = Vec::new();
    for local in local_artifacts {
        if local.trust_level() != ArtifactTrustLevel::CatalogDownloaded
            || local.metadata().catalog_package_receipt().is_some()
        {
            continue;
        }
        let Some(active) = active_by_id.get(local.id()) else {
            continue;
        };
        let Some(receipt) = active.metadata().catalog_package_receipt().cloned() else {
            continue;
        };
        if receipt.artifact_id() != *local.id() || !legacy_contract_matches(local, active) {
            continue;
        }
        let updated = local.clone().with_metadata(
            local
                .metadata()
                .clone()
                .with_catalog_package_receipt(receipt),
        );
        *local = updated.clone();
        batch.push(updated);
    }
    if !batch.is_empty()
        && let Err(error) = context.storage().upsert_artifacts(&batch)
    {
        log::warn!(
            "catalog receipt reconciliation backfill failed; count={}; reason={error}",
            batch.len()
        );
    }
}

fn legacy_contract_matches(local: &LibraryArtifact, active: &LibraryArtifact) -> bool {
    local.id() == active.id()
        && local.technology() == active.technology()
        && local.file_name() == active.file_name()
        && local.files().len() == active.files().len()
        && local
            .files()
            .iter()
            .zip(active.files())
            .all(|(local, active)| {
                local.version() == active.version()
                    && local.sha256() == active.sha256()
                    && local.install_as() == active.install_as()
                    && local.pe_compatibility() == active.pe_compatibility()
            })
        && local.metadata().release() == active.metadata().release()
        && local.metadata().upstream_package() == active.metadata().upstream_package()
        && local.metadata().runtime_target() == active.metadata().runtime_target()
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        Architecture, ArtifactMetadata, CatalogPackageReceiptV1, CatalogReceiptSchemaV1,
        CatalogSignatureReceipt, CatalogTargetReceipt, ComponentFile, LibraryTechnology,
        PackageRelease, PackageVersion, PathRef, Sha256Hash,
    };

    use super::*;

    fn artifact(package_id: &str, revision_digit: char) -> LibraryArtifact {
        let revision =
            Sha256Hash::new(revision_digit.to_string().repeat(64)).expect("revision digest");
        let receipt = CatalogPackageReceiptV1 {
            schema_version: CatalogReceiptSchemaV1,
            package_id: package_id.to_owned(),
            vendor: "nvidia".to_owned(),
            technology: "dlss_super_resolution".to_owned(),
            variant: "runtime".to_owned(),
            display_name: "DLSS".to_owned(),
            release: PackageRelease {
                version: PackageVersion::parse("1.0.0").expect("package version"),
                channel: ReleaseChannel::Stable,
                label: None,
                components: Default::default(),
            },
            target: CatalogTargetReceipt {
                os: "windows".to_owned(),
                architecture: Architecture::X64,
                compatibility: None,
            },
            provenance: None,
            revision_sha256: revision.clone(),
            primary_file_name: "nvngx_dlss.dll".to_owned(),
            primary_sha256: Sha256Hash::new("f".repeat(64)).expect("member digest"),
            primary_signature: CatalogSignatureReceipt::Unsigned,
            legal_documents: Vec::new(),
            size_bytes: 1,
        };
        LibraryArtifact::new(
            ArtifactId::for_package_revision(&revision),
            LibraryTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(
                    PathRef::new(format!("C:/cache/{revision_digit}/nvngx_dlss.dll"))
                        .expect("path"),
                )
                .with_sha256(Sha256Hash::new("f".repeat(64)).expect("member digest")),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(ArtifactMetadata::default().with_catalog_package_receipt(receipt))
    }

    #[test]
    fn group_resolution_keeps_only_the_matching_active_registration() {
        let active = artifact("pkg", 'a');
        let local = active.clone();
        let stale = artifact("pkg", 'b');

        let (active_only, ignored) = resolve_group(
            "pkg".to_owned(),
            PackageGroup {
                active: Some(active.clone()),
                locals: Vec::new(),
            },
        )
        .expect("active group");
        assert!(active_only.local.is_none());
        assert_eq!(active_only.local_state, LibraryLocalState::Absent);
        assert_eq!(ignored, 0);

        let (local_only, ignored) = resolve_group(
            "pkg".to_owned(),
            PackageGroup {
                active: None,
                locals: vec![(local.clone(), LibraryLocalState::Verified)],
            },
        )
        .expect("local group");
        assert_eq!(local_only.local.as_ref(), Some(&local));
        assert_eq!(ignored, 0);

        let (matched, ignored) = resolve_group(
            "pkg".to_owned(),
            PackageGroup {
                active: Some(active),
                locals: vec![
                    (stale, LibraryLocalState::Verified),
                    (local.clone(), LibraryLocalState::Verified),
                ],
            },
        )
        .expect("matched group");
        assert_eq!(matched.local.as_ref(), Some(&local));
        assert_eq!(matched.local_state, LibraryLocalState::Verified);
        assert_eq!(ignored, 1);
    }

    #[test]
    fn stale_registration_is_replaced_by_active_candidate_and_unverified_local_only_is_excluded() {
        let active = artifact("active", 'a');
        let stale = artifact("active", 'b');
        let missing = artifact("missing", 'c');
        let verified = artifact("verified", 'd');
        let (active_entry, ignored) = resolve_group(
            "active".to_owned(),
            PackageGroup {
                active: Some(active.clone()),
                locals: vec![(stale, LibraryLocalState::Verified)],
            },
        )
        .expect("active group");
        assert_eq!(ignored, 1);
        let inventory = Inventory {
            entries: vec![
                active_entry,
                InventoryEntry {
                    package_id: "missing".to_owned(),
                    active: None,
                    local: Some(missing),
                    local_state: LibraryLocalState::Missing,
                },
                InventoryEntry {
                    package_id: "verified".to_owned(),
                    active: None,
                    local: Some(verified.clone()),
                    local_state: LibraryLocalState::Verified,
                },
            ],
            uncatalogued_verified: Vec::new(),
            catalog_status: LibraryCatalogStatus::Active,
        };

        let (artifacts, downloaded, active_catalog) = inventory.replacement_projection();
        assert_eq!(artifacts, vec![active.clone(), verified.clone()]);
        assert_eq!(downloaded, HashSet::from([verified.id().clone()]));
        assert!(active_catalog.contains_key(active.id()));
    }

    #[test]
    fn fallback_envelope_is_explicitly_incomplete() {
        let inventory = Inventory {
            entries: Vec::new(),
            uncatalogued_verified: Vec::new(),
            catalog_status: LibraryCatalogStatus::LocalFallback,
        };
        let output = inventory.package_output();
        assert!(output.packages.is_empty());
        assert_eq!(output.catalog_status, LibraryCatalogStatus::LocalFallback);
    }

    #[test]
    fn legacy_backfill_requires_every_represented_contract_field_and_member_hash() {
        let active = artifact("pkg", 'a');
        let matching = active.clone().with_metadata(ArtifactMetadata::default());
        assert!(legacy_contract_matches(&matching, &active));

        let wrong_member = LibraryArtifact::new(
            active.id().clone(),
            active.technology(),
            active.file_name(),
            vec![
                ComponentFile::new(
                    PathRef::new("C:/cache/local/nvngx_dlss.dll").expect("local path"),
                )
                .with_sha256(Sha256Hash::new("e".repeat(64)).expect("wrong member digest")),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("legacy artifact");

        assert!(!legacy_contract_matches(&wrong_member, &active));
    }
}
