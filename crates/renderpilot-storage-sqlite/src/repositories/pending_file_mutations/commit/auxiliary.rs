use super::prelude::*;
use super::*;

pub(in crate::repositories) fn validate_auxiliary_preservations(
    binding: &OptiScalerAggregateBinding,
    by_path: &BTreeMap<String, Vec<&FoldEntry>>,
) -> AppResult<()> {
    for auxiliary in &binding.auxiliary {
        let source_key = normalized_path_key(auxiliary.source.as_str());
        let destination_key = normalized_path_key(auxiliary.destination.as_str());
        let destination_entries = by_path.get(&destination_key).map_or(&[][..], Vec::as_slice);
        expect_file_transition(
            destination_entries,
            auxiliary.destination.as_str(),
            &DurableObservation::Absent,
            &file_observation(&auxiliary.receipt),
        )?;
        require_action(
            destination_entries,
            &[ActionKind::Write],
            auxiliary.destination.as_str(),
        )?;
        if auxiliary.receipt.ownership() != FileOwnership::Owned {
            return Err(AppError::storage_failed(
                "OptiScaler auxiliary preservation must carry Owned custody",
            ));
        }
        if by_path.get(&source_key).is_none_or(Vec::is_empty) {
            return Err(AppError::storage_failed(format!(
                "OptiScaler auxiliary source '{}' is absent from the journal",
                auxiliary.source.as_str()
            )));
        }
    }
    for preservation in &binding.owned_preservations {
        let key = normalized_path_key(&preservation.source);
        let entries = by_path.get(&key).map_or(&[][..], Vec::as_slice);
        let Some(first) = entries.first() else {
            return Err(AppError::storage_failed(format!(
                "OptiScaler owned preservation source '{}' is absent from the journal",
                preservation.source
            )));
        };
        if !matches!(first.before, DurableObservation::File { .. })
            || preservation.prior.ownership() != FileOwnership::Owned
            || preservation.current.ownership() != FileOwnership::Owned
            || preservation.prior.identity() != preservation.current.identity()
            || first.before != file_observation(&preservation.current)
        {
            return Err(AppError::storage_failed(format!(
                "OptiScaler owned preservation source '{}' does not bind exact identity and digest",
                preservation.source
            )));
        }
        if endpoint_initial_receipt(entries).as_ref() != Some(&preservation.current) {
            return Err(AppError::storage_failed(format!(
                "OptiScaler owned preservation source '{}' has no exact current receipt",
                preservation.source
            )));
        }
    }
    Ok(())
}

pub(in crate::repositories) fn expect_file_transition(
    entries: &[&FoldEntry],
    path: &str,
    before: &DurableObservation,
    after: &DurableObservation,
) -> AppResult<()> {
    let Some(first) = entries.first() else {
        return Err(AppError::storage_failed(format!(
            "OptiScaler aggregate path '{path}' has no journal effect"
        )));
    };
    let last = entries.last().expect("first implies last");
    expect_observation(&first.before, before, path, "aggregate preimage")?;
    let Some(last_after) = &last.after else {
        return Err(AppError::storage_failed(format!(
            "OptiScaler aggregate path '{path}' has no durable postimage"
        )));
    };
    expect_observation(last_after, after, path, "aggregate postimage")
}

pub(in crate::repositories) fn expect_observation(
    actual: &DurableObservation,
    expected: &DurableObservation,
    path: &str,
    role: &str,
) -> AppResult<()> {
    if actual != expected {
        return Err(AppError::storage_failed(format!(
            "OptiScaler {role} mismatch at '{path}'"
        )));
    }
    Ok(())
}

pub(in crate::repositories) fn expect_optional_observation(
    actual: Option<&DurableObservation>,
    expected: &DurableObservation,
    path: &str,
) -> AppResult<()> {
    let Some(actual) = actual else {
        return Err(AppError::storage_failed(format!(
            "OptiScaler journal has no durable postimage at '{path}'"
        )));
    };
    expect_observation(actual, expected, path, "aggregate postimage")
}

pub(in crate::repositories) fn require_action(
    entries: &[&FoldEntry],
    accepted: &[ActionKind],
    path: &str,
) -> AppResult<()> {
    if entries.iter().any(|entry| accepted.contains(&entry.action)) {
        Ok(())
    } else {
        Err(AppError::storage_failed(format!(
            "OptiScaler aggregate path '{path}' has an unauthorized action"
        )))
    }
}

pub(in crate::repositories) fn file_observation(receipt: &FileReceipt) -> DurableObservation {
    DurableObservation::File {
        identity: receipt.identity().to_owned(),
        digest: receipt.digest().clone(),
    }
}

pub(in crate::repositories) fn endpoint_initial_receipt(
    entries: &[&FoldEntry],
) -> Option<FileReceipt> {
    entries.first().and_then(|entry| match &entry.before {
        DurableObservation::File { identity, digest } => {
            FileReceipt::owned(identity.clone(), digest.clone()).ok()
        }
        _ => None,
    })
}

pub(in crate::repositories) fn endpoint_code(endpoint: Endpoint) -> u8 {
    match endpoint {
        Endpoint::Single => 0,
        Endpoint::Source => 1,
        Endpoint::Destination => 2,
    }
}

pub(in crate::repositories) fn collect_fold_entries(
    journal: &OptiScalerJournal,
) -> AppResult<Vec<FoldEntry>> {
    let mut entries = Vec::new();
    let mut last_touch: BTreeMap<String, (u32, Endpoint)> = BTreeMap::new();
    let mut outputs: BTreeMap<(u32, u8), DurableObservation> = BTreeMap::new();
    for operation in journal.operations() {
        let mut keys = BTreeSet::new();
        for endpoint in operation.effect().endpoints() {
            let key = normalized_path_key(endpoint.path());
            if !keys.insert(key.clone()) {
                return Err(AppError::storage_failed(format!(
                    "OptiScaler operation {} touches one normalized path twice",
                    operation.operation_id()
                )));
            }
            let before = match endpoint.preimage() {
                Preimage::Initial {
                    observation,
                    receipt,
                    owned_basis,
                } => {
                    if let Some(receipt) = receipt
                        && &file_observation(receipt) != observation
                    {
                        return Err(AppError::storage_failed(
                            "OptiScaler initial receipt does not bind its observation",
                        ));
                    }
                    if let Some(basis) = owned_basis
                        && (basis.ownership() != FileOwnership::Owned
                            || receipt
                                .as_ref()
                                .is_none_or(|receipt| receipt.identity() != basis.identity()))
                    {
                        return Err(AppError::storage_failed(
                            "OptiScaler owned basis does not bind the initial identity",
                        ));
                    }
                    if last_touch.contains_key(&key) {
                        return Err(AppError::storage_failed(format!(
                            "OptiScaler repeated path '{}' does not use its prior postimage",
                            endpoint.path()
                        )));
                    }
                    observation.clone()
                }
                Preimage::PriorPostimage {
                    operation_id,
                    endpoint: prior_endpoint,
                } => {
                    if *operation_id >= operation.operation_id()
                        || last_touch.get(&key) != Some(&(*operation_id, *prior_endpoint))
                    {
                        return Err(AppError::storage_failed(format!(
                            "OptiScaler repeated path '{}' does not bind its last producer",
                            endpoint.path()
                        )));
                    }
                    outputs
                        .get(&(*operation_id, endpoint_code(*prior_endpoint)))
                        .cloned()
                        .ok_or_else(|| {
                            AppError::storage_failed(format!(
                                "OptiScaler prior postimage for '{}' is not durably known",
                                endpoint.path()
                            ))
                        })?
                }
            };
            let (action, after, terminal) =
                effect_endpoint_result(operation.effect(), endpoint.endpoint(), &before)?;
            if !terminal && action != ActionKind::PostCommitRemoveDirectory {
                return Err(AppError::storage_failed(format!(
                    "OptiScaler operation {} is not terminal at the commit boundary",
                    operation.operation_id()
                )));
            }
            let entry = FoldEntry {
                operation_id: operation.operation_id(),
                endpoint: endpoint.endpoint(),
                path: endpoint.path().to_owned(),
                preimage: endpoint.preimage().clone(),
                before,
                after,
                action,
            };
            if let Some(after) = &entry.after {
                outputs.insert(
                    (entry.operation_id, endpoint_code(entry.endpoint)),
                    after.clone(),
                );
            }
            last_touch.insert(key, (operation.operation_id(), endpoint.endpoint()));
            entries.push(entry);
        }
    }
    Ok(entries)
}

pub(in crate::repositories) fn effect_endpoint_result(
    effect: &OperationEffect,
    endpoint: Endpoint,
    before: &DurableObservation,
) -> AppResult<(ActionKind, Option<DurableObservation>, bool)> {
    use renderpilot_domain::{
        CreateDirectoryState, DeleteState, RelocateState, RemoveDirectoryState, VerifyState,
        WriteState,
    };
    match effect {
        OperationEffect::Write(payload) => match payload.state() {
            WriteState::Applied { live, custody } => {
                if endpoint != Endpoint::Single
                    || !live.is_exact()
                    || !matches!(live, DurableObservation::File { .. })
                    || !write_custody_matches_preimage(custody, before)
                {
                    return Err(AppError::storage_failed(
                        "OptiScaler write lacks exact custody and file postimage",
                    ));
                }
                expect_endpoint_after(payload.endpoint(), live)?;
                Ok((ActionKind::Write, Some(live.clone()), true))
            }
            WriteState::Preserved => Ok((ActionKind::Write, Some(before.clone()), true)),
            _ => Ok((ActionKind::Write, None, false)),
        },
        OperationEffect::Delete(payload) => match payload.state() {
            DeleteState::Applied { custody } => {
                if endpoint != Endpoint::Single
                    || !matches!(before, DurableObservation::File { .. })
                    || custody != before
                {
                    return Err(AppError::storage_failed(
                        "OptiScaler delete lacks exact file custody preimage",
                    ));
                }
                expect_endpoint_after(payload.endpoint(), &DurableObservation::Absent)?;
                Ok((ActionKind::Delete, Some(DurableObservation::Absent), true))
            }
            DeleteState::Preserved => Ok((ActionKind::Delete, Some(before.clone()), true)),
            _ => Ok((ActionKind::Delete, None, false)),
        },
        OperationEffect::Verify(payload) => match payload.state() {
            VerifyState::Applied { observed } => {
                if !before.is_exact() || observed != before {
                    return Err(AppError::storage_failed(
                        "OptiScaler verification lacks an exact unchanged observation",
                    ));
                }
                expect_endpoint_after(payload.endpoint(), observed)?;
                Ok((ActionKind::Verify, Some(observed.clone()), true))
            }
            VerifyState::Preserved => Ok((ActionKind::Verify, Some(before.clone()), true)),
            VerifyState::Planned => Ok((ActionKind::Verify, None, false)),
        },
        OperationEffect::Relocate(payload) => match payload.state() {
            RelocateState::Applied {
                source_after,
                destination_after,
            } => {
                let after = match endpoint {
                    Endpoint::Source => source_after,
                    Endpoint::Destination => destination_after,
                    Endpoint::Single => {
                        return Err(AppError::storage_failed(
                            "OptiScaler relocation has an invalid endpoint role",
                        ));
                    }
                };
                let record_endpoint = match endpoint {
                    Endpoint::Source => payload.source(),
                    Endpoint::Destination => payload.destination(),
                    Endpoint::Single => unreachable!(),
                };
                expect_endpoint_after(record_endpoint, after)?;
                Ok((ActionKind::Relocate, Some(after.clone()), true))
            }
            RelocateState::Preserved => Ok((ActionKind::Relocate, Some(before.clone()), true)),
            _ => Ok((ActionKind::Relocate, None, false)),
        },
        OperationEffect::CreateDirectory(payload) => match payload.state() {
            CreateDirectoryState::Applied { live } => {
                if !matches!(before, DurableObservation::Absent)
                    || !matches!(live, DurableObservation::Directory { .. })
                {
                    return Err(AppError::storage_failed(
                        "OptiScaler directory creation lacks an exact absent-to-directory transition",
                    ));
                }
                expect_endpoint_after(payload.endpoint(), live)?;
                Ok((ActionKind::CreateDirectory, Some(live.clone()), true))
            }
            CreateDirectoryState::Preserved => {
                Ok((ActionKind::CreateDirectory, Some(before.clone()), true))
            }
            _ => Ok((ActionKind::CreateDirectory, None, false)),
        },
        OperationEffect::PostCommitRemoveDirectory(payload) => match payload.state() {
            RemoveDirectoryState::Applied { directory } => {
                if !matches!(before, DurableObservation::Directory { .. }) || before != directory {
                    return Err(AppError::storage_failed(
                        "OptiScaler directory cleanup lacks an exact directory preimage",
                    ));
                }
                expect_endpoint_after(payload.endpoint(), &DurableObservation::Absent)?;
                Ok((
                    ActionKind::PostCommitRemoveDirectory,
                    Some(DurableObservation::Absent),
                    true,
                ))
            }
            RemoveDirectoryState::Planned { .. } | RemoveDirectoryState::RemoveIntent { .. } => {
                Ok((ActionKind::PostCommitRemoveDirectory, None, false))
            }
        },
    }
}

pub(in crate::repositories) fn write_custody_matches_preimage(
    custody: &DurableObservation,
    before: &DurableObservation,
) -> bool {
    match custody {
        DurableObservation::Absent => true,
        DurableObservation::File { digest, .. } => {
            matches!(before, DurableObservation::File { digest: before_digest, .. } if digest == before_digest)
        }
        DurableObservation::Directory { .. }
        | DurableObservation::NonRegular
        | DurableObservation::Unreadable => false,
    }
}

pub(in crate::repositories) fn expect_endpoint_after(
    endpoint: &OperationEndpoint,
    expected: &DurableObservation,
) -> AppResult<()> {
    match endpoint.expected_after() {
        ExpectedAfter::Known(observation) if observation == expected => Ok(()),
        _ => Err(AppError::storage_failed(
            "OptiScaler action postimage is not bound to its endpoint token",
        )),
    }
}

fn has_parent_escape_normalized(normalized: &str) -> bool {
    normalized
        .split('/')
        .any(|component| component == ".." || component == ".")
}

pub(in crate::repositories) fn has_parent_escape(path: &str) -> bool {
    has_parent_escape_normalized(&normalized_path_key(path))
}

pub(in crate::repositories) fn is_lexically_within(path: &str, root: &str) -> bool {
    let path_key = normalized_path_key(path);
    let root_key = normalized_path_key(root);
    if has_parent_escape_normalized(&path_key) || has_parent_escape_normalized(&root_key) {
        return false;
    }
    let path = path_key.trim_end_matches('/');
    let root = root_key.trim_end_matches('/');
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub(in crate::repositories) fn direct_parent_lexical(path: &str) -> Option<String> {
    let key = normalized_path_key(path);
    let slash = key.rfind('/')?;
    if slash == 0 {
        return Some("/".to_owned());
    }
    if slash == 2 && key.as_bytes().get(1) == Some(&b':') {
        return Some(key[..3].to_owned());
    }
    Some(key[..slash].to_owned())
}

/// SQLite does not inspect live files.  Native observation belongs to the
/// filesystem authority, not this persistence adapter.
pub(in crate::repositories) fn validate_optiscaler_file_receipt_metadata(
    path: &renderpilot_domain::PathRef,
) -> AppResult<()> {
    if path.as_str().trim().is_empty() || path.as_str().contains('\0') {
        return Err(AppError::storage_failed(
            "OptiScaler durable file receipt has an invalid path",
        ));
    }
    Ok(())
}

/// SQLite does not inspect live directories; native observation belongs to the
/// filesystem authority.
pub(in crate::repositories) fn validate_optiscaler_directory_receipt_metadata(
    path: &renderpilot_domain::PathRef,
    expected_identity: &str,
) -> AppResult<()> {
    if path.as_str().trim().is_empty()
        || path.as_str().contains('\0')
        || expected_identity.trim().is_empty()
    {
        return Err(AppError::storage_failed(
            "OptiScaler durable directory receipt is invalid",
        ));
    }
    Ok(())
}
