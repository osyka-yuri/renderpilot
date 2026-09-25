//! Recovery for the one post-commit directory action.
//!
//! This action is deliberately narrower than the file actions: it never
//! captures or restores the directory and never allocates a private artifact.
//! The receipt identity is historical evidence; the live directory is removed
//! exactly once, only after a durable intent.

use std::path::Path;

use renderpilot_domain::{
    DurableObservation, OperationEffect as DomainOperationEffect, OperationRecord,
    RemoveDirectoryEffect, RemoveDirectoryState as DomainRemoveDirectoryState,
};

use super::{PreparedFileMutation, durable, native, observe, set_endpoint_expected};
use crate::ServiceError;
use crate::fs::{self, AuthorityMode};

pub(super) fn apply(prepared: &mut PreparedFileMutation<'_>) -> Result<(), ServiceError> {
    validate_order(&prepared.journal)?;
    for index in 0..prepared.journal.operations().len() {
        let Some(DomainOperationEffect::PostCommitRemoveDirectory(effect)) = prepared
            .journal
            .operations()
            .get(index)
            .map(OperationRecord::effect)
        else {
            continue;
        };

        match effect.state().clone() {
            DomainRemoveDirectoryState::Planned { directory } => {
                let path = Path::new(effect.endpoint().path());
                let expected = native(&directory);
                let live = observe(path);
                if live != expected && live != super::DiskObservation::Absent {
                    return Err(super::token_drift(path, &expected, &live));
                }
                let durable_live = durable(&live);

                let mut next = prepared.journal.clone();
                if let DomainOperationEffect::PostCommitRemoveDirectory(value) =
                    next.operations_mut()[index].effect_mut()
                {
                    *value.state_mut() = DomainRemoveDirectoryState::RemoveIntent {
                        directory: directory.clone(),
                        live: durable_live.clone(),
                    };
                }
                prepared.cas(next)?;
                complete_remove_intent(prepared, index, directory, &durable_live)?;
            }
            DomainRemoveDirectoryState::RemoveIntent { directory, live } => {
                complete_remove_intent(prepared, index, directory, &live)?;
            }
            DomainRemoveDirectoryState::Applied { directory } => {
                let path =
                    Path::new(prepared.journal.operations()[index].effect().endpoints()[0].path());
                let expected = native(&directory);
                let observed = observe(path);
                if observed != super::DiskObservation::Absent {
                    return Err(super::token_drift(
                        path,
                        &super::DiskObservation::Absent,
                        &observed,
                    ));
                }
                if !matches!(expected, super::DiskObservation::Directory { .. }) {
                    return Err(crate::failed(
                        "post-commit directory history is not an exact directory",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_order(journal: &renderpilot_domain::OptiScalerJournal) -> Result<(), ServiceError> {
    let mut previous: Option<(usize, String)> = None;
    let mut seen_postcommit = false;
    for (index, record) in journal.operations().iter().enumerate() {
        let DomainOperationEffect::PostCommitRemoveDirectory(effect) = record.effect() else {
            if seen_postcommit && record.effect().is_precommit() {
                return Err(crate::failed(
                    "post-commit directory actions must form the journal suffix",
                ));
            }
            continue;
        };
        seen_postcommit = true;
        let path = Path::new(effect.endpoint().path());
        let key = crate::paths::normalized_key(path);
        let order = (component_depth(path), key);
        if let Some((previous_depth, previous_key)) = &previous
            && (*previous_depth < order.0
                || (*previous_depth == order.0 && previous_key > &order.1))
        {
            return Err(crate::failed(format!(
                "post-commit directory journal order is not deepest-first at ordinal {index}"
            )));
        }
        previous = Some(order);
    }
    Ok(())
}

fn component_depth(path: &Path) -> usize {
    path.components()
        .filter(|component| matches!(component, std::path::Component::Normal(_)))
        .count()
}

pub(super) fn ensure_rollback_planned(effect: &RemoveDirectoryEffect) -> Result<(), ServiceError> {
    if matches!(effect.state(), DomainRemoveDirectoryState::Planned { .. }) {
        Ok(())
    } else {
        Err(crate::failed(
            "post-commit directory cleanup reached rollback after commit",
        ))
    }
}

pub(super) fn ensure_cleanup_ready(
    journal: &renderpilot_domain::OptiScalerJournal,
) -> Result<(), ServiceError> {
    validate_order(journal)?;
    if journal.operations().iter().any(|record| {
        matches!(
            record.effect(),
            DomainOperationEffect::PostCommitRemoveDirectory(effect)
                if !matches!(effect.state(), DomainRemoveDirectoryState::Applied { .. })
        )
    }) {
        return Err(crate::failed(
            "private namespace cleanup cannot begin before every post-commit directory is applied",
        ));
    }
    Ok(())
}

fn complete_remove_intent(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    directory: DurableObservation,
    live: &DurableObservation,
) -> Result<(), ServiceError> {
    let path = Path::new(prepared.journal.operations()[index].effect().endpoints()[0].path());
    let expected_directory = native(&directory);
    let expected_live = native(live);
    let observed = super::observe(path);

    match (&expected_live, &observed) {
        (
            super::DiskObservation::Absent | super::DiskObservation::Directory { .. },
            super::DiskObservation::Absent,
        ) => {
            // Either already absent, or the exact remove syscall completed before
            // the CAS. The intent already binds the only permitted live preimage, so
            // no IO is needed and the postimage is accepted after this observation.
        }
        (
            super::DiskObservation::Directory { identity },
            super::DiskObservation::Directory {
                identity: observed_identity,
            },
        ) if identity == observed_identity => {
            remove_exact_empty_directory(path, identity)?;
            let after = super::observe(path);
            if after != super::DiskObservation::Absent {
                return Err(super::token_drift(
                    path,
                    &super::DiskObservation::Absent,
                    &after,
                ));
            }
        }
        (_, observed) => {
            return Err(super::token_drift(path, &expected_live, observed));
        }
    }

    if !matches!(expected_directory, super::DiskObservation::Directory { .. }) {
        return Err(crate::failed(
            "post-commit directory history is not an exact directory",
        ));
    }

    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::PostCommitRemoveDirectory(value) =
        next.operations_mut()[index].effect_mut()
    {
        *value.state_mut() = DomainRemoveDirectoryState::Applied { directory };
        set_endpoint_expected(value.endpoint_mut(), &super::DiskObservation::Absent);
    }
    prepared.cas(next)
}

fn remove_exact_empty_directory(path: &Path, expected_identity: &str) -> Result<(), ServiceError> {
    let (parent, leaf) = fs::verified_parent(path)?;
    parent.remove_empty_dir_exact(&leaf, expected_identity, AuthorityMode::CooperativeSameUid)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use renderpilot_domain::{
        OperationEffect, OptiScalerDirectoryReceipt, PathRef, PrivateArtifactSlots,
    };

    use super::*;
    use crate::file_mutation::MutationScope;
    use crate::file_mutation::optiscaler::{
        DiskObservation, OptiScalerPlannedOperation, ThreatModel,
    };

    #[test]
    fn exact_empty_directory_removal_is_idempotent_after_a_torn_syscall() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("owned");
        fs::create_dir(&path).expect("directory");
        let expected = match super::super::observe(&path) {
            DiskObservation::Directory { identity } => identity,
            other => panic!("expected directory, found {other:?}"),
        };

        remove_exact_empty_directory(&path, &expected).expect("remove");
        assert_eq!(super::super::observe(&path), DiskObservation::Absent);
        remove_exact_empty_directory(&path, &expected).expect("torn syscall retry");
        assert_eq!(super::super::observe(&path), DiskObservation::Absent);
    }

    #[test]
    fn exact_empty_directory_rejects_identity_type_and_nonempty_drift() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("owned");
        fs::create_dir(&path).expect("directory");
        let actual = match super::super::observe(&path) {
            DiskObservation::Directory { identity } => identity,
            other => panic!("expected directory, found {other:?}"),
        };
        assert!(remove_exact_empty_directory(&path, "foreign-identity").is_err());
        assert!(matches!(
            super::super::observe(&path),
            DiskObservation::Directory { identity } if identity == actual
        ));

        let file_path = root.path().join("foreign-file");
        fs::write(&file_path, b"foreign-file").expect("file");
        assert!(remove_exact_empty_directory(&file_path, &actual).is_err());
        assert!(matches!(
            super::super::observe(&file_path),
            DiskObservation::File { .. }
        ));

        fs::write(path.join("child"), b"not empty").expect("child");
        let current = match super::super::observe(&path) {
            DiskObservation::Directory { identity } => identity,
            other => panic!("expected directory, found {other:?}"),
        };
        assert!(remove_exact_empty_directory(&path, &current).is_err());
        assert!(matches!(
            super::super::observe(&path),
            DiskObservation::Directory { .. }
        ));

        let missing_parent_path = root.path().join("missing-parent").join("owned");
        assert!(remove_exact_empty_directory(&missing_parent_path, &current).is_err());
    }

    #[test]
    fn receipt_directory_starts_with_historical_identity_and_no_artifacts() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("owned");
        fs::create_dir(&path).expect("directory");
        let identity = match super::super::observe(&path) {
            DiskObservation::Directory { identity } => identity,
            other => panic!("expected directory, found {other:?}"),
        };
        let receipt = OptiScalerDirectoryReceipt {
            path: PathRef::new(path.to_string_lossy().into_owned()).expect("receipt path"),
            identity: identity.clone(),
        };
        let scope = MutationScope::single(root.path()).expect("scope");
        let journal = super::super::build_journal(
            &scope,
            &root.path().join("transaction-id"),
            &[OptiScalerPlannedOperation::PostCommitRemoveDirectory(
                receipt,
            )],
            ThreatModel::CooperativeSameUid,
        )
        .expect("journal");
        let record = &journal.operations()[0];
        let OperationEffect::PostCommitRemoveDirectory(effect) = record.effect() else {
            panic!("expected post-commit action");
        };
        assert!(matches!(
            effect.state(),
            DomainRemoveDirectoryState::Planned {
                directory: DurableObservation::Directory { identity: value }
            } if value == &identity
        ));
        assert_eq!(
            record.slots(),
            &PrivateArtifactSlots::new(
                DurableObservation::Absent,
                DurableObservation::Absent,
                DurableObservation::Absent,
            )
            .expect("empty slots")
        );
        assert!(matches!(
            effect.endpoint().expected_after(),
            renderpilot_domain::ExpectedAfter::Pending
        ));
        assert!(journal.validate().is_ok());
    }

    #[test]
    fn rollback_accepts_only_planned_and_performs_no_directory_io() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("owned");
        fs::create_dir(&path).expect("directory");
        let identity = match super::super::observe(&path) {
            DiskObservation::Directory { identity } => identity,
            other => panic!("expected directory, found {other:?}"),
        };
        let scope = MutationScope::single(root.path()).expect("scope");
        let receipt = OptiScalerDirectoryReceipt {
            path: PathRef::new(path.to_string_lossy().into_owned()).expect("receipt path"),
            identity,
        };
        let journal = super::super::build_journal(
            &scope,
            &root.path().join("transaction-id"),
            &[OptiScalerPlannedOperation::PostCommitRemoveDirectory(
                receipt,
            )],
            ThreatModel::CooperativeSameUid,
        )
        .expect("journal");
        let OperationEffect::PostCommitRemoveDirectory(effect) = journal.operations()[0].effect()
        else {
            panic!("expected post-commit action");
        };
        ensure_rollback_planned(effect).expect("planned cleanup remains untouched");
        assert!(path.is_dir());
        assert!(matches!(
            effect.state(),
            DomainRemoveDirectoryState::Planned { .. }
        ));
    }
}
