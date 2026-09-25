use super::*;

pub(in crate::addons::optiscaler) fn prior_release_receipt<'a>(
    state: Option<&'a OptiScalerInstallState>,
    path: &Path,
) -> Option<&'a OptiScalerFileReceipt> {
    state?
        .release_files
        .iter()
        .find(|receipt| crate::paths::same_path(Path::new(receipt.path.as_str()), path))
}

pub(in crate::addons::optiscaler) fn prior_runtime_binding<'a>(
    state: Option<&'a OptiScalerInstallState>,
    path: &Path,
) -> Option<&'a renderpilot_domain::OptiScalerModuleRuntimeBinding> {
    state?
        .runtime_bindings
        .iter()
        .find(|binding| crate::paths::same_path(Path::new(binding.path.as_str()), path))
}

/// Returns the persisted release/runtime endpoints that may be observed as
/// absent below the existing OptiScaler target root during repair or removal.
/// Sidecars, peers, external relocations, and private artifacts are excluded
/// deliberately and continue to use strict authority observation.
pub(crate) fn managed_state_endpoint_roots(
    state: &OptiScalerInstallState,
) -> Vec<(PathBuf, PathBuf)> {
    let root = PathBuf::from(state.target_dir.as_str());
    state
        .release_files
        .iter()
        .map(|receipt| (root.clone(), PathBuf::from(receipt.path.as_str())))
        .chain(
            state
                .runtime_bindings
                .iter()
                .map(|binding| (root.clone(), PathBuf::from(binding.path.as_str()))),
        )
        .collect()
}

pub(in crate::addons::optiscaler) fn build_operations<P: AsRef<Path>>(
    targets: &[MutationTarget],
    write_paths: &HashSet<String>,
    delete_paths: &HashSet<String>,
    relocations: &[(PathBuf, PathBuf)],
    exact_preimages: &HashMap<String, crate::file_mutation::optiscaler::PlannedPreimage>,
    post_commit_directories: &[renderpilot_domain::OptiScalerDirectoryReceipt],
    preferred_paths: impl IntoIterator<Item = P>,
) -> Result<Vec<crate::file_mutation::optiscaler::OptiScalerPlannedOperation>, ServiceError> {
    use crate::file_mutation::optiscaler::{
        OptiScalerPlannedOperation as Operation, PlannedParticipant as Participant, PlannedPreimage,
    };
    let participant = |target: MutationTarget,
                       exact_preimage: bool,
                       force_absent: bool|
     -> Result<Participant, ServiceError> {
        let key = crate::paths::normalized_key(&target.path);
        let expected = target.expected_sha256();
        let persisted = exact_preimages.get(&key);
        if let Some(expected) = expected {
            let current = persisted.and_then(|preimage| match preimage {
                PlannedPreimage::Exact { current, .. }
                | PlannedPreimage::ExactReused { current, .. } => Some(current),
                PlannedPreimage::Absent | PlannedPreimage::Verify => None,
            });
            if let Some(current) = current {
                if current.digest() != expected {
                    return Err(failed(format!(
                        "OptiScaler planned digest does not match exact receipt at {}",
                        target.path.display()
                    )));
                }
            } else if persisted.is_none() && (exact_preimage || force_absent) {
                return Err(failed(format!(
                    "OptiScaler destructive target has no exact receipt at {}",
                    target.path.display()
                )));
            }
        }
        let preimage = if force_absent {
            PlannedPreimage::Absent
        } else if let Some(preimage) = persisted {
            if exact_preimage {
                preimage.clone()
            } else {
                match preimage {
                    PlannedPreimage::Exact { current, .. } => PlannedPreimage::Exact {
                        current: current.clone(),
                        prior_owned: None,
                    },
                    PlannedPreimage::ExactReused { current, .. } => PlannedPreimage::ExactReused {
                        current: current.clone(),
                        authority: crate::file_mutation::optiscaler::ReusedMutationAuthority::ObservationOnly,
                    },
                    PlannedPreimage::Absent => PlannedPreimage::Absent,
                    PlannedPreimage::Verify => PlannedPreimage::Verify,
                }
            }
        } else {
            if target.expects_absence() || write_paths.contains(&key) {
                PlannedPreimage::Absent
            } else {
                PlannedPreimage::Verify
            }
        };
        Ok(Participant {
            path: target.path,
            preimage,
        })
    };
    let mut preferred_order = HashMap::new();
    for (index, path) in preferred_paths.into_iter().enumerate() {
        preferred_order
            .entry(crate::paths::normalized_key(path.as_ref()))
            .or_insert(index);
    }
    let mut ordered_targets = targets.to_vec();
    ordered_targets.sort_by_key(|target| {
        let is_parent_directory = target.is_absent_directory()
            && !relocations.iter().any(|(source, destination)| {
                crate::paths::same_path(source, &target.path)
                    || crate::paths::same_path(destination, &target.path)
            });
        (
            !is_parent_directory,
            preferred_order
                .get(&crate::paths::normalized_key(&target.path))
                .copied()
                .unwrap_or(usize::MAX),
        )
    });
    let mut relocation_keys = HashSet::new();
    let mut pre_relocation_deletes = HashSet::new();
    let mut operations = Vec::new();
    for (source, destination) in relocations {
        let source_key = crate::paths::normalized_key(source);
        let destination_key = crate::paths::normalized_key(destination);
        if !relocation_keys.insert(source_key) || !relocation_keys.insert(destination_key) {
            continue;
        }
        // A no-replace relocation can target a slot that is currently occupied
        // by the outer file being removed.  Make that destructive producer
        // ordinal explicit before the relocation so the destination resolves
        // to its exact PriorPostimage (Absent), rather than observing a live
        // occupied path at preparation time.
        if delete_paths.contains(&crate::paths::normalized_key(destination)) {
            let destination_target = targets
                .iter()
                .find(|target| crate::paths::same_path(&target.path, destination))
                .cloned()
                .ok_or_else(|| {
                    failed(format!(
                        "relocation destination has no delete target at {}",
                        destination.display()
                    ))
                })?;
            let destination_key = crate::paths::normalized_key(destination);
            let destination_preimage = exact_preimages.get(&destination_key);
            let operation = if matches!(destination_preimage, Some(PlannedPreimage::Absent)) {
                // An explicit retained-original restoration also accepts a
                // manually absent active target. Its no-replace relocation
                // still consumes the same sealed Absent postimage.
                Operation::Verify(participant(destination_target, false, true)?)
            } else {
                Operation::Delete(participant(destination_target, true, false)?)
            };
            operations.push(operation);
            pre_relocation_deletes.insert(crate::paths::normalized_key(destination));
        }
        let source_target = targets
            .iter()
            .find(|target| crate::paths::same_path(&target.path, source))
            .cloned()
            .unwrap_or_else(|| MutationTarget::copy(source));
        let destination_target = targets
            .iter()
            .find(|target| crate::paths::same_path(&target.path, destination))
            .cloned()
            .unwrap_or_else(|| MutationTarget::absent_file(destination));
        operations.push(Operation::Relocate {
            source: participant(source_target, true, false)?,
            // Relocate is no-replace.  An exact destination observation is
            // never destructive authority; an occupied destination must
            // fail closed in the custody planner.
            destination: participant(destination_target, false, true)?,
        });
    }
    for target in ordered_targets {
        let key = crate::paths::normalized_key(&target.path);
        if relocation_keys.contains(&key)
            && (!write_paths.contains(&key) && !delete_paths.contains(&key)
                || pre_relocation_deletes.contains(&key))
        {
            continue;
        }
        // Fresh proxy installation relocates a ReShade host away from the
        // root slot and then writes OptiScaler into that now-absent slot.
        // The ordered custody planner binds this repeated participant to the
        // preceding Relocate postimage.
        let post_relocation_absent = relocation_keys.contains(&key);
        let operation = if delete_paths.contains(&key) {
            if matches!(exact_preimages.get(&key), Some(PlannedPreimage::Absent)) {
                // A managed endpoint can be absent after repair drift. Its
                // terminal release proof is Verify(Absent), never an invalid
                // Delete(Absent); the executor consumes that same ordinal.
                Operation::Verify(participant(target.clone(), false, true)?)
            } else {
                Operation::Delete(participant(target.clone(), true, post_relocation_absent)?)
            }
        } else if write_paths.contains(&key) {
            Operation::Write(participant(target.clone(), true, post_relocation_absent)?)
        } else if target.is_absent_directory() {
            Operation::CreateDirectory(participant(target.clone(), false, true)?)
        } else {
            Operation::Verify(participant(target.clone(), false, false)?)
        };
        operations.push(operation);
    }
    operations.extend(
        post_commit_directories
            .iter()
            .cloned()
            .map(Operation::PostCommitRemoveDirectory),
    );
    Ok(operations)
}

/// Captures exact identity and digest through the native authority.  It is
/// used only for read-only adoption/reconciliation; writes use the applied
/// journal token via `PreparedFileMutation::receipt_for`.
pub(in crate::addons::optiscaler) fn exact_receipt_from_live(
    path: &Path,
    ownership: FileOwnership,
) -> Result<FileReceipt, ServiceError> {
    maybe_exact_receipt_from_live(path, ownership)?
        .ok_or_else(|| failed(format!("file is absent: {}", path.display())))
}

/// Observes one authority parent and classifies an absent leaf without a
/// racy `exists` probe. This is read-only reconciliation evidence; mutation
/// authorization remains the applied custody token.
pub(in crate::addons::optiscaler) fn maybe_exact_receipt_from_live(
    path: &Path,
    ownership: FileOwnership,
) -> Result<Option<FileReceipt>, ServiceError> {
    let (parent, leaf) = crate::fs::verified_parent(path).map_err(|error| {
        failed(format!(
            "failed to acquire file authority {}: {error}",
            path.display()
        ))
    })?;
    let Some(observation) = parent.observe_leaf(&leaf).map_err(|error| {
        failed(format!(
            "failed to observe file authority {}: {error}",
            path.display()
        ))
    })?
    else {
        return Ok(None);
    };
    if observation.kind != crate::fs::EntryKind::File {
        return Err(failed(format!(
            "path is not a regular file: {}",
            path.display()
        )));
    }
    receipt_from_observation(path, observation, ownership).map(Some)
}

/// Observes an OptiScaler-managed endpoint below a retained existing game
/// root.  Only a missing descendant becomes `None`; every problem acquiring
/// the root or walking a real ancestor remains a hard authority error.  This
/// narrow repair/release helper must never be used for peers, custody, private
/// artifacts, or external relocation endpoints.
pub(in crate::addons::optiscaler) fn maybe_exact_managed_receipt_from_live(
    managed_root: &Path,
    path: &Path,
    ownership: FileOwnership,
) -> Result<Option<FileReceipt>, ServiceError> {
    if !crate::paths::is_within(path, managed_root) || crate::paths::same_path(path, managed_root) {
        return Err(failed(format!(
            "OptiScaler managed endpoint is outside its persisted root: {}",
            path.display()
        )));
    }
    let root = crate::fs::VerifiedDir::open(managed_root).map_err(|error| {
        failed(format!(
            "failed to acquire managed OptiScaler root {}: {error}",
            managed_root.display()
        ))
    })?;
    let observation = root.observe_descendant(path).map_err(|error| {
        failed(format!(
            "failed to observe managed OptiScaler endpoint {}: {error}",
            path.display()
        ))
    })?;
    let Some(observation) = observation else {
        return Ok(None);
    };
    if observation.kind != crate::fs::EntryKind::File {
        return Err(failed(format!(
            "managed OptiScaler endpoint is not a regular file: {}",
            path.display()
        )));
    }
    receipt_from_observation(path, observation, ownership).map(Some)
}

fn receipt_from_observation(
    path: &Path,
    observation: crate::fs::EntryObservation,
    ownership: FileOwnership,
) -> Result<FileReceipt, ServiceError> {
    let digest = observation
        .digest
        .ok_or_else(|| failed(format!("file digest is unavailable: {}", path.display())))?;
    let digest = Sha256Hash::new(digest).map_err(|error| {
        failed(format!(
            "invalid file digest for {}: {error}",
            path.display()
        ))
    })?;
    let receipt = match ownership {
        FileOwnership::Owned => FileReceipt::owned(observation.identity, digest),
        FileOwnership::Reused => FileReceipt::reused(observation.identity, digest),
    }
    .map_err(|error| {
        failed(format!(
            "invalid exact receipt for {}: {error}",
            path.display()
        ))
    })?;
    receipt.validate().map_err(|error| {
        failed(format!(
            "invalid exact receipt for {}: {error}",
            path.display()
        ))
    })?;
    Ok(receipt)
}

/// Reads one regular participant through the retained authority entry and
/// returns the receipt for the exact bytes that were read.  Callers that use
/// the bytes for planning must retain this receipt as their preimage instead
/// of observing the path again with a separate hash read.
pub(in crate::addons::optiscaler) fn read_exact_file_from_live(
    path: &Path,
    ownership: FileOwnership,
) -> Result<Option<(Vec<u8>, FileReceipt)>, ServiceError> {
    let (parent, leaf) = crate::fs::verified_parent(path).map_err(|error| {
        failed(format!(
            "failed to acquire file authority {}: {error}",
            path.display()
        ))
    })?;
    let Some(observation) = parent.observe_leaf(&leaf).map_err(|error| {
        failed(format!(
            "failed to observe file authority {}: {error}",
            path.display()
        ))
    })?
    else {
        return Ok(None);
    };
    if observation.kind != crate::fs::EntryKind::File {
        return Err(failed(format!(
            "path is not a regular file: {}",
            path.display()
        )));
    }
    let (bytes, read_observation) = parent
        .read_regular_file_bounded(
            &leaf,
            Some(&observation),
            crate::file_mutation::optiscaler::MAX_OPTISCALER_CONFIGURATION_BYTES,
        )
        .map_err(|error| {
            failed(format!(
                "failed to read file through retained authority {}: {error}",
                path.display()
            ))
        })?;
    if read_observation != observation {
        return Err(failed(format!(
            "file identity or digest changed while reading {}",
            path.display()
        )));
    }
    let digest = observation
        .digest
        .ok_or_else(|| failed(format!("file digest is unavailable: {}", path.display())))?;
    let digest = Sha256Hash::new(digest).map_err(|error| {
        failed(format!(
            "invalid file digest for {}: {error}",
            path.display()
        ))
    })?;
    let receipt = match ownership {
        FileOwnership::Owned => FileReceipt::owned(observation.identity, digest),
        FileOwnership::Reused => FileReceipt::reused(observation.identity, digest),
    }
    .map_err(|error| {
        failed(format!(
            "invalid exact receipt for {}: {error}",
            path.display()
        ))
    })?;
    receipt.validate().map_err(|error| {
        failed(format!(
            "invalid exact receipt for {}: {error}",
            path.display()
        ))
    })?;
    Ok(Some((bytes, receipt)))
}

/// Performs the read-only peer-host receipt proof used by matcher availability
/// before release or module downloads begin. The guarded lifecycle repeats the
/// same proof immediately before custody preparation.
pub(in crate::addons::optiscaler) fn preflight_peer_host_transition(
    context: &Context,
    game_id: &GameId,
    from: &Path,
    to: &Path,
) -> Result<(), ServiceError> {
    let from = path_ref(from)?;
    let to = path_ref(to)?;
    adoption::plan_peer_host_transition(
        adoption::PeerTopologyDirection::IntoOptiTopology,
        context,
        game_id,
        &from,
        &to,
        None,
    )
    .map(|_| ())
}

/// Selects receipt-bound directories that are not part of a new target's
/// directory workset. They are appended to the custody manifest as post-commit
/// cleanup obligations, never as ordinary file targets.
pub(in crate::addons::optiscaler) fn post_commit_directory_receipts(
    receipts: &[renderpilot_domain::OptiScalerDirectoryReceipt],
    protected_root: Option<&Path>,
) -> Vec<renderpilot_domain::OptiScalerDirectoryReceipt> {
    let mut receipts = receipts.to_vec();
    receipts.sort_by(|left, right| {
        right
            .path
            .as_str()
            .len()
            .cmp(&left.path.as_str().len())
            .then_with(|| right.path.as_str().cmp(left.path.as_str()))
    });
    receipts
        .into_iter()
        .filter(|receipt| {
            let path = Path::new(receipt.path.as_str());
            !protected_root.is_some_and(|root| {
                crate::paths::same_path(path, root)
                    || crate::paths::is_within(path, root)
                    || crate::paths::is_within(root, path)
            })
        })
        .collect()
}
