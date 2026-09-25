fn open_or_create_control_parent(path: &Path) -> Result<crate::fs::VerifiedDir, ServiceError> {
    match crate::fs::VerifiedDir::open(path) {
        Ok(value) => Ok(value),
        Err(open_error) => {
            let parent = path
                .parent()
                .filter(|value| !value.as_os_str().is_empty())
                .ok_or_else(|| {
                    crate::failed(format!(
                        "control parent cannot be created after {open_error}"
                    ))
                })?;
            let parent = open_or_create_control_parent(parent)?;
            let leaf = crate::fs::LeafName::from_path(path)?;
            match parent
                .create_directory_no_replace(&leaf, crate::fs::AuthorityMode::CooperativeSameUid)
            {
                Ok(value) => Ok(value),
                Err(create_error) => crate::fs::VerifiedDir::open(path).map_err(|retry_error| {
                    crate::failed(format!(
                        "control parent create failed: {create_error}; reopen failed: {retry_error}"
                    ))
                }),
            }
        }
    }
}

fn create_private_directory(path: &Path) -> Result<String, ServiceError> {
    let (parent, leaf) = crate::fs::verified_parent(path)?;
    let capability = capability_from_leaf(&leaf)?;
    Ok(parent
        .create_private_namespace(&leaf, &capability)?
        .identity()
        .to_owned())
}
fn capability_from_leaf(leaf: &crate::fs::LeafName) -> Result<[u8; 32], ServiceError> {
    let value = leaf.as_os_str().to_string_lossy();
    capability_from_hex(
        value
            .rsplit_once('-')
            .map(|(_, suffix)| suffix)
            .ok_or_else(|| crate::failed("private namespace has no capability"))?,
    )
}
fn capability_from_hex(encoded: &str) -> Result<[u8; 32], ServiceError> {
    hex::decode(encoded)
        .map_err(|error| crate::failed(format!("invalid namespace capability: {error}")))?
        .try_into()
        .map_err(|_| crate::failed("namespace capability must contain 256 bits"))
}

pub(crate) const MAX_OPTISCALER_CONFIGURATION_BYTES: usize = 16 * 1024 * 1024;

fn read_verified_file(path: &Path) -> Result<(Vec<u8>, crate::fs::EntryObservation), ServiceError> {
    let (parent, leaf) = crate::fs::verified_parent(path)?;
    parent.read_regular_file(&leaf, None)
}
fn write_private_artifact(
    artifact: &PrivateArtifact,
    bytes: &[u8],
) -> Result<DiskObservation, ServiceError> {
    match artifact.namespace.directory().create_file_no_replace(
        &artifact.leaf,
        bytes,
        crate::fs::AuthorityMode::CooperativeSameUid,
    )? {
        crate::fs::CreateFileNoReplace::Created { observation, .. } => Ok(match observation.kind {
            crate::fs::EntryKind::File => DiskObservation::File {
                identity: observation.identity,
                digest: Sha256Hash::new(
                    observation
                        .digest
                        .ok_or_else(|| crate::failed("created private artifact has no digest"))?,
                )
                .expect("authority digest"),
            },
            crate::fs::EntryKind::Directory => DiskObservation::Directory {
                identity: observation.identity,
            },
        }),
        crate::fs::CreateFileNoReplace::Occupied => {
            Err(crate::failed("private artifact leaf is occupied"))
        }
    }
}

fn create_private_artifact_directory(
    artifact: &PrivateArtifact,
) -> Result<DiskObservation, ServiceError> {
    let directory = artifact.namespace.directory().create_directory_no_replace(
        &artifact.leaf,
        crate::fs::AuthorityMode::CooperativeSameUid,
    )?;
    Ok(DiskObservation::Directory {
        identity: directory.identity().to_owned(),
    })
}

fn read_private_artifact(
    artifact: &PrivateArtifact,
    max_bytes: Option<usize>,
) -> Result<(Vec<u8>, crate::fs::EntryObservation), ServiceError> {
    match max_bytes {
        Some(max_bytes) => artifact.namespace.directory().read_regular_file_bounded(
            &artifact.leaf,
            None,
            max_bytes,
        ),
        None => artifact
            .namespace
            .directory()
            .read_regular_file(&artifact.leaf, None),
    }
}

fn remove_private_artifact(
    artifact: &PrivateArtifact,
    expected: &crate::fs::EntryObservation,
) -> Result<(), ServiceError> {
    let Some(observed) = artifact
        .namespace
        .directory()
        .observe_leaf(&artifact.leaf)?
    else {
        return Ok(());
    };
    if &observed != expected {
        return Err(crate::failed("private artifact identity changed"));
    }
    artifact.namespace.directory().remove_exact(
        &artifact.leaf,
        expected,
        crate::fs::AuthorityMode::CooperativeSameUid,
    )
}

fn move_into_private_artifact(
    source: &Path,
    artifact: &PrivateArtifact,
    expected: &DiskObservation,
) -> Result<(), ServiceError> {
    let (source_parent, source_leaf) = crate::fs::verified_parent(source)?;
    let expected = private_entry_observation(expected)?;
    match source_parent.rename_no_replace(
        &source_leaf,
        artifact.namespace.directory(),
        &artifact.leaf,
        Some(&expected),
        crate::fs::AuthorityMode::CooperativeSameUid,
    )? {
        crate::fs::RenameNoReplace::Moved => Ok(()),
        crate::fs::RenameNoReplace::Occupied => Err(crate::failed(
            "exact private artifact destination is occupied",
        )),
    }
}

fn move_from_private_artifact(
    artifact: &PrivateArtifact,
    destination: &Path,
    expected: &DiskObservation,
) -> Result<(), ServiceError> {
    let (destination_parent, destination_leaf) = crate::fs::verified_parent(destination)?;
    let expected = private_entry_observation(expected)?;
    match artifact.namespace.directory().rename_no_replace(
        &artifact.leaf,
        &destination_parent,
        &destination_leaf,
        Some(&expected),
        crate::fs::AuthorityMode::CooperativeSameUid,
    )? {
        crate::fs::RenameNoReplace::Moved => Ok(()),
        crate::fs::RenameNoReplace::Occupied => {
            Err(crate::failed("exact live destination is occupied"))
        }
    }
}

/// Publishes a staged private artifact as a new game-visible entry.  This is
/// intentionally separate from `move_from_private_artifact`: custody restore
/// and rollback retain the original descriptor, while a newly published stage
/// must inherit the public destination parent's Windows DACL.
fn publish_from_private_artifact(
    artifact: &PrivateArtifact,
    destination: &Path,
    expected: &DiskObservation,
) -> Result<(), ServiceError> {
    let (destination_parent, destination_leaf) = crate::fs::verified_parent(destination)?;
    let expected = private_entry_observation(expected)?;
    match artifact.namespace.directory().publish_staged_no_replace(
        &artifact.leaf,
        &destination_parent,
        &destination_leaf,
        &expected,
        crate::fs::AuthorityMode::CooperativeSameUid,
    )? {
        crate::fs::RenameNoReplace::Moved => Ok(()),
        crate::fs::RenameNoReplace::Occupied => Err(crate::failed(
            "exact live publication destination is occupied",
        )),
    }
}

fn overwrite_file_exact(
    path: &Path,
    expected: &DiskObservation,
    bytes: &[u8],
) -> Result<DiskObservation, ServiceError> {
    let DiskObservation::File { identity, digest } = expected else {
        return Err(crate::failed(
            "in-place update requires an exact file preimage",
        ));
    };
    let (parent, leaf) = crate::fs::verified_parent(path)?;
    let observation = parent.overwrite_regular_file(
        &leaf,
        &crate::fs::EntryObservation {
            kind: crate::fs::EntryKind::File,
            identity: identity.clone(),
            digest: Some(digest.as_str().to_owned()),
        },
        bytes,
    )?;
    let digest = Sha256Hash::new(
        observation
            .digest
            .ok_or_else(|| crate::failed("in-place update produced no file digest"))?,
    )
    .map_err(|error| {
        crate::failed(format!(
            "in-place update produced an invalid digest: {error}"
        ))
    })?;
    Ok(DiskObservation::File {
        identity: observation.identity,
        digest,
    })
}

fn overwrite_file_with_stable_identity(
    path: &Path,
    current: &DiskObservation,
    stable_identity: &str,
    bytes: &[u8],
) -> Result<DiskObservation, ServiceError> {
    let DiskObservation::File { identity, digest } = current else {
        return Err(crate::failed(
            "retained-handle update requires an exact current file observation",
        ));
    };
    if identity != stable_identity {
        return Err(crate::failed(
            "retained-handle update identity differs from its durable authority",
        ));
    }
    let (parent, leaf) = crate::fs::verified_parent(path)?;
    let observation = parent.overwrite_regular_file_with_stable_identity(
        &leaf,
        &crate::fs::EntryObservation {
            kind: crate::fs::EntryKind::File,
            identity: identity.clone(),
            digest: Some(digest.as_str().to_owned()),
        },
        stable_identity,
        bytes,
    )?;
    let digest = Sha256Hash::new(
        observation
            .digest
            .ok_or_else(|| crate::failed("retained-handle update produced no file digest"))?,
    )
    .map_err(|error| {
        crate::failed(format!(
            "retained-handle update produced an invalid digest: {error}"
        ))
    })?;
    Ok(DiskObservation::File {
        identity: observation.identity,
        digest,
    })
}

fn private_entry_observation(
    value: &DiskObservation,
) -> Result<crate::fs::EntryObservation, ServiceError> {
    match value {
        DiskObservation::File { identity, digest } => Ok(crate::fs::EntryObservation {
            kind: crate::fs::EntryKind::File,
            identity: identity.clone(),
            digest: Some(digest.as_str().to_owned()),
        }),
        DiskObservation::Directory { identity } => Ok(crate::fs::EntryObservation {
            kind: crate::fs::EntryKind::Directory,
            identity: identity.clone(),
            digest: None,
        }),
        _ => Err(crate::failed(
            "private artifact expected observation is unsafe",
        )),
    }
}
fn move_exact_no_replace(
    source: &Path,
    destination: &Path,
    expected: &DiskObservation,
) -> Result<(), ServiceError> {
    let (source_parent, source_leaf) = crate::fs::verified_parent(source)?;
    let (destination_parent, destination_leaf) = crate::fs::verified_parent(destination)?;
    let expected = private_entry_observation(expected)?;
    match source_parent.rename_no_replace(
        &source_leaf,
        &destination_parent,
        &destination_leaf,
        Some(&expected),
        crate::fs::AuthorityMode::CooperativeSameUid,
    )? {
        crate::fs::RenameNoReplace::Moved => Ok(()),
        crate::fs::RenameNoReplace::Occupied => {
            Err(crate::failed("exact rename destination is occupied"))
        }
    }
}
fn remove_exact_leaf(
    path: &Path,
    expected: &crate::fs::EntryObservation,
) -> Result<(), ServiceError> {
    let (parent, leaf) = crate::fs::verified_parent(path)?;
    let Some(observed) = parent.observe_leaf(&leaf)? else {
        return Ok(());
    };
    if &observed != expected {
        return Err(crate::failed(format!(
            "private artifact identity changed: {}",
            path.display()
        )));
    }
    parent.remove_exact(
        &leaf,
        expected,
        crate::fs::AuthorityMode::CooperativeSameUid,
    )
}

fn cleanup_transaction_namespace(
    root: &Path,
    id: &str,
    journal: &OptiScalerJournal,
    expected_identity: Option<&str>,
) -> Result<(), ServiceError> {
    let binding = journal.control_namespace();
    let capability = capability_from_hex(binding.capability().as_str())?;
    let expected_path = root.join(format!("control-{id}-{}", binding.capability()));
    if !crate::paths::same_path(Path::new(binding.path()), &expected_path) {
        return Err(crate::failed(
            "control namespace path is not bound to the mutation root",
        ));
    }
    let path = expected_path.as_path();
    if observe(path) == DiskObservation::Absent {
        return Ok(());
    }
    let parent = crate::fs::VerifiedDir::open(
        path.parent()
            .ok_or_else(|| crate::failed("control namespace has no parent"))?,
    )?;
    let namespace = match (binding.identity(), expected_identity) {
        (Some(identity), Some(expected)) if identity != expected => {
            return Err(crate::failed("control namespace cleanup identity changed"));
        }
        (Some(identity), _) => {
            crate::fs::ControlNamespace::reopen(&parent, id, &capability, identity)?
        }
        (None, _)
            if matches!(
                journal.materialization(),
                DomainMaterializationState::ControlCreateIntent
            ) =>
        {
            crate::fs::ControlNamespace::reopen_observed(&parent, id, &capability)?
        }
        (None, _) => return Err(crate::failed("control namespace has no durable identity")),
    };
    namespace.remove_empty(crate::fs::AuthorityMode::CooperativeSameUid)?;
    Ok(())
}

fn cleanup_private_workspace(
    prepared: &PreparedFileMutation<'_>,
    workspace_id: usize,
    expected_identity: &str,
) -> Result<(), ServiceError> {
    let path = derived_private_workspace_path(
        &prepared.id,
        &prepared.journal,
        u32::try_from(workspace_id).map_err(|_| crate::failed("workspace id overflow"))?,
    )?;
    match observe(&path) {
        DiskObservation::Absent => Ok(()),
        DiskObservation::Directory { identity } if identity == expected_identity => {
            let (parent, leaf) = crate::fs::verified_parent(&path)?;
            let namespace = crate::fs::PrivateNamespace::reopen(&parent, &leaf, expected_identity)?;
            namespace.remove_empty(crate::fs::AuthorityMode::CooperativeSameUid)
        }
        observed => Err(crate::failed(format!(
            "private workspace cleanup observation changed at {}: expected directory identity {}, observed {observed:?}",
            path.display(),
            expected_identity
        ))),
    }
}

fn next_workspace_cleanup_state(
    journal: &OptiScalerJournal,
    workspace_id: usize,
) -> Result<JournalCleanup, ServiceError> {
    if workspace_id == 0 {
        let identity = journal
            .control_namespace()
            .identity()
            .ok_or_else(|| crate::failed("control namespace has no durable identity"))?;
        return Ok(JournalCleanup::ControlRemoveIntent {
            expected_identity: identity.to_owned(),
        });
    }
    let identity = journal
        .private_workspaces()
        .get(workspace_id - 1)
        .ok_or_else(|| crate::failed("prior workspace id is outside the journal"))?
        .identity()
        .ok_or_else(|| crate::failed("prior private workspace has no durable identity"))?;
    let prior_id =
        u32::try_from(workspace_id - 1).map_err(|_| crate::failed("workspace ordinal overflow"))?;
    Ok(JournalCleanup::WorkspaceRemoveIntent {
        workspace_id: prior_id,
        expected_identity: identity.to_owned(),
    })
}

fn cleanup_committed(prepared: &mut PreparedFileMutation<'_>) -> Result<(), ServiceError> {
    post_commit::ensure_cleanup_ready(&prepared.journal)?;
    loop {
        prepared.cleanup_private_artifacts()?;
        match prepared.journal.cleanup().clone() {
            JournalCleanup::Inactive => {
                let mut next = prepared.journal.clone();
                if let Some(workspace_id) =
                    prepared.journal.private_workspaces().len().checked_sub(1)
                {
                    let expected_identity = prepared.journal.private_workspaces()[workspace_id]
                        .identity()
                        .ok_or_else(|| crate::failed("private workspace has no durable identity"))?
                        .to_owned();
                    let target_id = u32::try_from(workspace_id)
                        .map_err(|_| crate::failed("workspace ordinal overflow"))?;
                    next.set_cleanup(JournalCleanup::WorkspaceRemoveIntent {
                        workspace_id: target_id,
                        expected_identity,
                    });
                } else {
                    let expected_identity = prepared
                        .journal
                        .control_namespace()
                        .identity()
                        .ok_or_else(|| crate::failed("control namespace has no durable identity"))?
                        .to_owned();
                    next.set_cleanup(JournalCleanup::ControlRemoveIntent { expected_identity });
                }
                prepared.cas(next)?;
            }
            JournalCleanup::WorkspaceRemoveIntent {
                workspace_id,
                expected_identity,
            } => {
                let workspace_id = usize::try_from(workspace_id)
                    .map_err(|_| crate::failed("cleanup ordinal overflow"))?;
                cleanup_private_workspace(prepared, workspace_id, &expected_identity)?;
                let mut next = prepared.journal.clone();
                next.set_cleanup(next_workspace_cleanup_state(
                    &prepared.journal,
                    workspace_id,
                )?);
                prepared.cas(next)?;
            }
            JournalCleanup::ControlRemoveIntent { expected_identity } => {
                cleanup_transaction_namespace(
                    &prepared.transaction_root,
                    &prepared.id,
                    &prepared.journal,
                    Some(&expected_identity),
                )?;
                let mut next = prepared.journal.clone();
                next.set_cleanup(JournalCleanup::Complete);
                prepared.cas(next)?;
            }
            JournalCleanup::Complete => break,
            JournalCleanup::ArtifactRemoveIntent { .. } => {
                return Err(crate::failed(
                    "committed cleanup artifact intent did not resolve to the inactive cursor",
                ));
            }
        }
    }
    prepared.delete_after_commit()
}
