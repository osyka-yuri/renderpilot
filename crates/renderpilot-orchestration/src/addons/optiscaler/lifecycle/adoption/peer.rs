use super::super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::addons::optiscaler) enum PeerTopologyDirection {
    IntoOptiTopology,
    OutOfOptiTopology,
}

#[derive(Debug, Clone)]
pub(in crate::addons::optiscaler) struct PeerReceiptTransition {
    pub(in crate::addons::optiscaler) before: InstalledAddon,
    pub(in crate::addons::optiscaler) after: InstalledAddon,
}

#[derive(Debug, Clone)]
pub(in crate::addons::optiscaler) struct PeerSidecarTransition {
    pub(in crate::addons::optiscaler) source: PathBuf,
    pub(in crate::addons::optiscaler) destination: PathBuf,
    pub(in crate::addons::optiscaler) source_receipt: FileReceipt,
}

#[derive(Debug, Clone)]
pub(in crate::addons::optiscaler) struct PeerHostTransitionPlan {
    pub(in crate::addons::optiscaler) from: PathRef,
    pub(in crate::addons::optiscaler) to: PathRef,
    pub(in crate::addons::optiscaler) live_sha256: Sha256Hash,
    pub(in crate::addons::optiscaler) destination_receipt: Option<FileReceipt>,
    pub(in crate::addons::optiscaler) receipt: PeerReceiptTransition,
    pub(in crate::addons::optiscaler) sidecar: Option<PeerSidecarTransition>,
    pub(in crate::addons::optiscaler) destination_ownership: FileOwnership,
}

impl PeerHostTransitionPlan {
    /// Returns the custody that the canonical peer record assigns to the
    /// relocated host.  The proxy executor consumes this value directly when
    /// it materializes the matching topology receipt.
    pub(in crate::addons::optiscaler) fn destination_ownership(&self) -> FileOwnership {
        self.destination_ownership
    }

    pub(in crate::addons::optiscaler) fn workset_paths(
        &self,
    ) -> impl Iterator<Item = PathBuf> + '_ {
        self.sidecar
            .iter()
            .flat_map(|sidecar| [&sidecar.source, &sidecar.destination])
            .cloned()
    }

    /// Exact preimages proved by peer planning. These targets override the
    /// ordinary topology/workset snapshots so the custody planner cannot adopt a host or
    /// baseline-sidecar that appeared after planning as overwrite authority.
    pub(in crate::addons::optiscaler) fn mutation_targets(&self) -> Vec<MutationTarget> {
        let mut targets = vec![
            MutationTarget::copy(Path::new(self.from.as_str())),
            match &self.destination_receipt {
                Some(receipt) => MutationTarget::quarantine(
                    Path::new(self.to.as_str()),
                    Some(receipt.digest().clone()),
                ),
                None => MutationTarget::absent_file(Path::new(self.to.as_str())),
            },
        ];
        if let Some(sidecar) = &self.sidecar {
            targets.extend([
                MutationTarget::copy(&sidecar.source),
                MutationTarget::absent_file(&sidecar.destination),
            ]);
        }
        targets
    }

    pub(in crate::addons::optiscaler) fn relocations(&self) -> Vec<(PathBuf, PathBuf)> {
        let mut relocations = vec![(
            PathBuf::from(self.from.as_str()),
            PathBuf::from(self.to.as_str()),
        )];
        if let Some(sidecar) = &self.sidecar {
            relocations.push((sidecar.source.clone(), sidecar.destination.clone()));
        }
        relocations
    }

    pub(in crate::addons::optiscaler) fn execute_sidecar(
        &self,
        mutation: &mut PreparedFileMutation<'_>,
        changed: &mut Vec<String>,
    ) -> Result<(), ServiceError> {
        let Some(sidecar) = &self.sidecar else {
            return Ok(());
        };
        mutation.relocate_file_exact(
            &sidecar.source,
            &sidecar.destination,
            &sidecar.source_receipt,
            sidecar.source_receipt.ownership(),
        )?;
        changed.push(sidecar.destination.to_string_lossy().into_owned());
        changed.push(sidecar.source.to_string_lossy().into_owned());
        Ok(())
    }
}

/// Builds the complete peer-host transition before any custody preparation starts.
/// Only one exact host membership is transferable. Generic backing and
/// overlapping payload/config claims remain fail-closed because they cannot
/// prove the rest of the peer workset.
pub(in crate::addons::optiscaler) fn plan_peer_host_transition(
    direction: PeerTopologyDirection,
    context: &Context,
    game_id: &GameId,
    from: &PathRef,
    to: &PathRef,
    allowed_destination_sha256: Option<&Sha256Hash>,
) -> Result<Option<PeerHostTransitionPlan>, ServiceError> {
    let from_path = Path::new(from.as_str());
    let to_path = Path::new(to.as_str());
    if crate::paths::same_path(from_path, to_path) {
        return Ok(None);
    }
    let host_receipt =
        super::super::maybe_exact_receipt_from_live(from_path, FileOwnership::Reused)?.ok_or_else(
            || {
                failed(format!(
                    "active ReShade peer source is missing: {}",
                    from_path.display()
                ))
            },
        )?;
    if !crate::addons::reshade::scan::is_reshade_proxy_file(from_path) {
        return Err(failed(format!(
            "active ReShade peer source is missing or unrecognized: {}",
            from_path.display()
        )));
    }
    let live_sha256 = host_receipt.digest().clone();
    let destination_receipt =
        super::super::maybe_exact_receipt_from_live(to_path, FileOwnership::Reused)?;
    if let Some(receipt) = &destination_receipt
        && allowed_destination_sha256 != Some(receipt.digest())
    {
        return Err(failed(format!(
            "relocated ReShade peer destination is occupied by an unexpected file: {}",
            to_path.display()
        )));
    }
    let mut owners = context
        .storage()
        .list_installed_addons()?
        .into_iter()
        .filter(|record| record.game_id() == game_id)
        .filter(|record| matches!(record.kind(), AddonKind::RenoDx | AddonKind::Luma))
        .filter(|record| record_mentions_path(record, from_path));
    let Some(peer) = owners.next() else {
        return Ok(None);
    };
    if owners.next().is_some() {
        return Err(failed(format!(
            "multiple add-on receipts own proxy host {}; repair receipt ownership before changing the proxy chain",
            from.as_str()
        )));
    }
    let created_matches = peer
        .created_files()
        .iter()
        .filter(|path| crate::paths::same_path(Path::new(path.as_str()), from_path))
        .count();
    let backed_up_matches = peer
        .backed_up_files()
        .iter()
        .filter(|path| crate::paths::same_path(Path::new(path.as_str()), from_path))
        .count();
    let managed_matches = peer
        .managed_files()
        .iter()
        .filter(|file| crate::paths::same_path(Path::new(file.path().as_str()), from_path))
        .collect::<Vec<_>>();
    if managed_matches
        .first()
        .is_some_and(|file| file.installed_sha256() != &live_sha256)
    {
        return Err(failed(format!(
            "peer host {} changed from its persisted receipt hash",
            from.as_str()
        )));
    }
    if backed_up_matches != 0 {
        return Err(failed(format!(
            "cannot relocate peer host {} with a generic backed-up receipt",
            from.as_str()
        )));
    }
    if created_matches + managed_matches.len() != 1
        || created_matches > 1
        || managed_matches.len() > 1
        || crate::paths::same_path(Path::new(peer.addon_file().as_str()), from_path)
        || peer
            .registered_exe_path()
            .is_some_and(|path| crate::paths::same_path(Path::new(path.as_str()), from_path))
    {
        return Err(failed(format!(
            "peer host {} has ambiguous receipt membership",
            from.as_str()
        )));
    }
    if peer_claims_path(&peer, to_path) {
        return Err(failed(format!(
            "peer receipt already claims relocated ReShade destination {}",
            to_path.display()
        )));
    }
    if direction == PeerTopologyDirection::OutOfOptiTopology
        && peer.kind() == AddonKind::Luma
        && created_matches != 0
    {
        return Err(failed(
            "Luma OptiScaler uninstall requires a managed peer host source",
        ));
    }
    let sidecar = if let Some(file) = managed_matches.first() {
        match (file.mode(), file.baseline()) {
            (ManagedFileMode::Owned, ManagedFileBaseline::Absent)
            | (ManagedFileMode::Reused, ManagedFileBaseline::Present { .. }) => None,
            (ManagedFileMode::Owned, ManagedFileBaseline::Present { sha256 }) => {
                let source =
                    crate::fs::backup_path(from_path).map_err(|error| failed(error.to_string()))?;
                let source_receipt =
                    super::super::maybe_exact_receipt_from_live(&source, FileOwnership::Reused)?
                        .filter(|receipt| receipt.digest() == sha256)
                        .ok_or_else(|| {
                            failed(format!(
                                "peer host baseline sidecar is missing or drifted: {}",
                                source.display()
                            ))
                        })?;
                if source_receipt.digest() != sha256 {
                    return Err(failed(format!(
                        "peer host baseline sidecar is missing or drifted: {}",
                        source.display()
                    )));
                }
                let destination =
                    crate::fs::backup_path(to_path).map_err(|error| failed(error.to_string()))?;
                if super::super::maybe_exact_receipt_from_live(&destination, FileOwnership::Reused)?
                    .is_some()
                {
                    return Err(failed(format!(
                        "relocated peer baseline sidecar destination is occupied: {}",
                        destination.display()
                    )));
                }
                if peer_claims_path(&peer, &destination) {
                    return Err(failed(format!(
                        "peer receipt already claims relocated baseline sidecar {}",
                        destination.display()
                    )));
                }
                Some(PeerSidecarTransition {
                    source,
                    destination,
                    source_receipt,
                })
            }
            (ManagedFileMode::Reused, ManagedFileBaseline::Absent) => {
                return Err(failed(format!(
                    "reused peer host {} has an invalid absent baseline",
                    from.as_str()
                )));
            }
        }
    } else {
        None
    };
    let luma_out =
        direction == PeerTopologyDirection::OutOfOptiTopology && peer.kind() == AddonKind::Luma;
    let mut created_files = peer
        .created_files()
        .iter()
        .filter(|path| !crate::paths::same_path(Path::new(path.as_str()), from_path))
        .cloned()
        .collect::<Vec<_>>();
    let mut backed_up_files = peer.backed_up_files().to_vec();
    let mut managed_files: Vec<ManagedAddonFile> = peer
        .managed_files()
        .iter()
        .map(|file| {
            if crate::paths::same_path(Path::new(file.path().as_str()), from_path) {
                if luma_out {
                    return file.clone();
                }
                match file.mode() {
                    renderpilot_domain::ManagedFileMode::Owned => ManagedAddonFile::owned(
                        to.clone(),
                        file.baseline().clone(),
                        file.installed_sha256().clone(),
                    ),
                    renderpilot_domain::ManagedFileMode::Reused => {
                        ManagedAddonFile::reused(to.clone(), file.installed_sha256().clone())
                    }
                }
            } else {
                file.clone()
            }
        })
        .collect();
    if luma_out {
        let managed = managed_matches.first().ok_or_else(|| {
            failed("Luma OptiScaler uninstall requires one managed peer host source")
        })?;
        if managed.mode() == ManagedFileMode::Owned {
            created_files.push(to.clone());
            if matches!(managed.baseline(), ManagedFileBaseline::Present { .. }) {
                backed_up_files.push(to.clone());
            }
        }
        managed_files
            .retain(|file| !crate::paths::same_path(Path::new(file.path().as_str()), from_path));
    } else if created_matches == 1 {
        managed_files.push(ManagedAddonFile::owned(
            to.clone(),
            ManagedFileBaseline::Absent,
            live_sha256.clone(),
        ));
    }
    let preserve = match peer.kind() {
        AddonKind::RenoDx => crate::addons::tracking::PreserveMetadata::renodx(),
        AddonKind::Luma => crate::addons::tracking::PreserveMetadata::luma(),
        AddonKind::OptiScaler => {
            return Err(failed(
                "OptiScaler cannot be used as a peer relocation participant",
            ));
        }
    };
    let rebuilt = crate::addons::tracking::rebuild_install_record(
        &peer,
        crate::addons::tracking::RebuildParts {
            addon_file: peer.addon_file().clone(),
            addon_version: crate::addons::tracking::AddonVersionUpdate::Keep,
            managed_files: crate::addons::tracking::ManagedFilesUpdate::Replace(managed_files),
            created_files,
            backed_up_files,
            tracked_sources: peer.tracked_sources().to_vec(),
            label: "OptiScaler proxy-chain relocation".to_owned(),
        },
        preserve,
    )?;
    let destination_ownership = if luma_out {
        managed_matches
            .first()
            .map(|file| match file.mode() {
                ManagedFileMode::Owned => FileOwnership::Owned,
                ManagedFileMode::Reused => FileOwnership::Reused,
            })
            .ok_or_else(|| failed("Luma peer transition has no managed source custody"))?
    } else {
        rebuilt
            .managed_files()
            .iter()
            .find(|file| {
                crate::paths::same_path(Path::new(file.path().as_str()), Path::new(to.as_str()))
            })
            .map(|file| match file.mode() {
                ManagedFileMode::Owned => FileOwnership::Owned,
                ManagedFileMode::Reused => FileOwnership::Reused,
            })
            .ok_or_else(|| {
                failed("peer host transition did not produce a canonical managed destination")
            })?
    };
    Ok(Some(PeerHostTransitionPlan {
        from: from.clone(),
        to: to.clone(),
        live_sha256,
        destination_receipt,
        receipt: PeerReceiptTransition {
            before: peer,
            after: rebuilt,
        },
        sidecar,
        destination_ownership,
    }))
}

fn record_mentions_path(record: &InstalledAddon, path: &Path) -> bool {
    record
        .created_files()
        .iter()
        .chain(record.backed_up_files())
        .any(|candidate| crate::paths::same_path(Path::new(candidate.as_str()), path))
        || record
            .managed_files()
            .iter()
            .any(|file| crate::paths::same_path(Path::new(file.path().as_str()), path))
        || crate::paths::same_path(Path::new(record.addon_file().as_str()), path)
        || record
            .registered_exe_path()
            .is_some_and(|candidate| crate::paths::same_path(Path::new(candidate.as_str()), path))
}

fn peer_claims_path(record: &InstalledAddon, path: &Path) -> bool {
    record
        .created_files()
        .iter()
        .chain(record.backed_up_files())
        .any(|candidate| crate::paths::same_path(Path::new(candidate.as_str()), path))
        || record
            .managed_files()
            .iter()
            .any(|file| crate::paths::same_path(Path::new(file.path().as_str()), path))
        || crate::paths::same_path(Path::new(record.addon_file().as_str()), path)
        || record
            .registered_exe_path()
            .is_some_and(|candidate| crate::paths::same_path(Path::new(candidate.as_str()), path))
}
