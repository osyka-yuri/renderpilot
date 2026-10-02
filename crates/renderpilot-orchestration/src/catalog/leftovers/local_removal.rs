//! Exact, remove-only handling for current canonical local claims.

use std::{collections::BTreeMap, path::Path};

use renderpilot_domain::{
    GameId, NormalizedPathRelation, Sha256Hash, normalized_path_key, normalized_path_relation,
};
use renderpilot_storage_sqlite::LocalCleanupOwners;

use crate::{
    Context, ServiceError,
    catalog::{
        installation_lifecycle::residue::{
            LocalCleanupBlockReason as BlockReason, LocalCleanupCategory as ClaimCategory,
            LocalCleanupClaim as CanonicalClaim, RetiredRootObservation, VerifiedResidueRoot,
            local_cleanup_claims_by_key, observe_registered_root,
        },
        leftovers::dto::{
            CleanupStep, CleanupStepOutcome, LeftoverCategory, LeftoverDisposition, LeftoverIssue,
            LeftoverIssueCode, LeftoverItem,
        },
    },
    context::RetiredLeftoversObservation,
    fs::{AuthorityMode, EntryKind, EntryObservation, LeafName, VerifiedDir},
    game_mutation_lock::GameMutationGuard,
};

#[derive(Debug, Clone)]
pub(super) struct LocalCleanupTarget {
    pub(super) claim: CanonicalClaim,
    pub(super) item: LeftoverItem,
    root_identity: String,
    native_identity: String,
}

pub(super) fn project_local_targets(
    observation: &RetiredLeftoversObservation,
) -> Vec<LocalCleanupTarget> {
    let RetiredRootObservation::ResidueOnly(root) = &observation.root else {
        return Vec::new();
    };
    let claims = local_cleanup_claims_by_key(&observation.owners);
    let mut targets = Vec::new();
    for remnant in root.remnants() {
        let Some(claim) = claims.get(&normalized_path_key(remnant.path().as_str())) else {
            continue;
        };
        let issue = claim_issue(claim, remnant.category(), remnant.removable());
        let disposition = if issue.is_some() {
            LeftoverDisposition::Blocked
        } else {
            LeftoverDisposition::Cleanable
        };
        let category = display_category(claim.category());
        let selector = format!(
            "local:{}:{}:{}",
            claim.category() as u8,
            claim.sha256().map(Sha256Hash::as_str).unwrap_or(""),
            claim.native_identity().unwrap_or_default()
        );
        let item_id = super::projection::canonical_item_id(
            observation.owners.game().id(),
            category,
            Some(claim.path().as_str()),
            &selector,
        );
        targets.push(LocalCleanupTarget {
            claim: claim.clone(),
            item: LeftoverItem {
                item_id,
                category,
                path: Some(claim.path().as_str().to_owned()),
                disposition,
                issue,
            },
            root_identity: root.native_root_identity().to_owned(),
            native_identity: remnant.native_identity().to_owned(),
        });
    }
    targets.sort_by(|left, right| {
        (left.claim.category(), left.claim.path().as_str())
            .cmp(&(right.claim.category(), right.claim.path().as_str()))
    });
    targets
}

/// Revalidates current owner, absence, root, and entry immediately before one
/// exact unlink. The original receipt remains in place for any later retry.
pub(super) fn remove_local_target(
    context: &Context,
    game_id: &GameId,
    game_guard: &GameMutationGuard,
    expected_owners: &LocalCleanupOwners,
    target: &LocalCleanupTarget,
) -> CleanupStep {
    let category = target.item.category;
    let fail = |code, detail: String| CleanupStep {
        item_id: target.item.item_id.clone(),
        category,
        outcome: CleanupStepOutcome::Blocked,
        issue: Some(LeftoverIssue {
            code,
            detail: Some(detail),
        }),
    };
    if game_guard.game_id() != game_id || expected_owners.game().id() != game_id {
        return fail(
            LeftoverIssueCode::StaleIntent,
            "The registered game changed after the cleanup proposal.".to_owned(),
        );
    }

    let target_key = normalized_path_key(target.claim.path().as_str());
    let result = (|| {
        let current = context.storage().read_local_cleanup_owners(game_id)?;
        if !same_owners(expected_owners, &current) || !current.availability().is_absent() {
            return Err(ServiceError::invalid_input(
                "canonical local ownership changed before exact cleanup",
            ));
        }
        let source = local_cleanup_claims_by_key(&current)
            .remove(&target_key)
            .ok_or_else(|| {
                ServiceError::invalid_input("canonical local claim disappeared before cleanup")
            })?;
        if source != target.claim || claim_issue(&source, target.claim.category(), true).is_some() {
            return Err(ServiceError::invalid_input(
                "canonical local claim changed or is not remove-only",
            ));
        }

        let root = match observe_registered_root(&current) {
            RetiredRootObservation::Missing(_) => return Ok(false),
            RetiredRootObservation::ResidueOnly(root) => root,
            RetiredRootObservation::PresentNonResidue { .. } => {
                return Err(ServiceError::invalid_input(
                    "installation root no longer contains only recorded cleanup remnants",
                ));
            }
            RetiredRootObservation::Indeterminate { reason } => {
                return Err(ServiceError::invalid_input(reason));
            }
        };
        if !same_path(root.root().as_str(), current.game().install_path().as_str()) {
            return Err(ServiceError::invalid_input(
                "registered installation root changed before cleanup",
            ));
        }
        if root.native_root_identity() != target.root_identity {
            return Err(ServiceError::invalid_input(
                "installation root identity changed before cleanup",
            ));
        }
        let remnant = root
            .remnants()
            .iter()
            .find(|remnant| normalized_path_key(remnant.path().as_str()) == target_key);
        let Some(remnant) = remnant else {
            return Ok(false);
        };
        if remnant.native_identity() != target.native_identity
            || target
                .claim
                .native_identity()
                .is_some_and(|identity| identity != target.native_identity)
            || remnant.digest() != target.claim.sha256()
            || !remnant.removable()
            || remnant.category() != target.claim.category()
        {
            return Err(ServiceError::invalid_input(
                "native cleanup entry no longer matches its canonical receipt",
            ));
        }

        let root_path = Path::new(root.root().as_str());
        let root_authority =
            VerifiedDir::open_absolute_components(root_path, Some(root.native_root_identity()))?;
        let target_path = Path::new(target.claim.path().as_str());
        if normalized_path_relation(root.root().as_str(), target.claim.path().as_str())
            != NormalizedPathRelation::LeftAncestor
        {
            return Err(ServiceError::invalid_input(
                "canonical local claim is not strictly below the installation root",
            ));
        }
        let parent_path = target_path.parent().ok_or_else(|| {
            ServiceError::invalid_input("canonical local claim has no parent directory")
        })?;
        let parent = VerifiedDir::open_absolute_components(parent_path, None)?;
        let leaf = LeafName::from_path(target_path)?;
        let Some(before) = parent.observe_leaf(&leaf)? else {
            return Ok(false);
        };
        if before.identity != target.native_identity || !entry_matches(&target.claim, &before) {
            return Err(ServiceError::invalid_input(
                "native cleanup entry changed immediately before removal",
            ));
        }
        if root_authority.identity() != target.root_identity {
            return Err(ServiceError::invalid_input(
                "installation root identity changed immediately before removal",
            ));
        }

        if target.claim.category() == ClaimCategory::Directory {
            let identity = target.claim.native_identity().ok_or_else(|| {
                ServiceError::invalid_input("recorded directory has no native identity")
            })?;
            prune_observed_directories(&root, Some(target_path))?;
            parent.remove_empty_dir_exact(&leaf, identity, AuthorityMode::CooperativeSameUid)?;
        } else {
            parent.remove_exact(&leaf, &before, AuthorityMode::CooperativeSameUid)?;
        }
        Ok(true)
    })();

    match result {
        Ok(true) => CleanupStep {
            item_id: target.item.item_id.clone(),
            category,
            outcome: CleanupStepOutcome::Removed,
            issue: None,
        },
        Ok(false) => CleanupStep {
            item_id: target.item.item_id.clone(),
            category,
            outcome: CleanupStepOutcome::AlreadyAbsent,
            issue: None,
        },
        Err(error) => fail(LeftoverIssueCode::OperationFailed, error.to_string()),
    }
}

/// Prunes only directories observed by the successful complete residue proof.
/// Files and entries added or replaced since that proof remain protected by
/// native remove-empty and identity checks.
pub(super) fn prune_empty_directories(root: &VerifiedResidueRoot) -> Result<(), ServiceError> {
    let root_path = Path::new(root.root().as_str());
    let Some(root_authority) = open_observed_directory(root_path, root.native_root_identity())?
    else {
        return Ok(());
    };
    prune_observed_directories(root, None)?;

    // Windows cannot unlink the root while its verified directory handle is
    // retained. Descendant parent handles have already left scope above.
    drop(root_authority);

    if let Some(parent_path) = root_path
        .parent()
        .filter(|parent_path| !parent_path.as_os_str().is_empty())
    {
        let parent = VerifiedDir::open_absolute_components(parent_path, None)?;
        let leaf = LeafName::from_path(root_path)?;
        parent.remove_empty_dir(
            &leaf,
            root.native_root_identity(),
            AuthorityMode::CooperativeSameUid,
        )?;
    }

    Ok(())
}

/// Removes only observed directories strictly below `scope`, deepest first.
/// `None` includes every observed descendant of the installation root.
fn prune_observed_directories(
    root: &VerifiedResidueRoot,
    scope: Option<&Path>,
) -> Result<(), ServiceError> {
    let root_path = Path::new(root.root().as_str());
    let parent_identities = root
        .directories()
        .iter()
        .map(|directory| (directory.relative_path(), directory.native_identity()))
        .chain([(Path::new(""), root.native_root_identity())])
        .collect::<BTreeMap<_, _>>();
    let mut directories = root
        .directories()
        .iter()
        .filter(|directory| {
            scope.is_none_or(|scope| {
                let directory_path = root_path.join(directory.relative_path());
                normalized_path_relation(
                    scope.to_string_lossy().as_ref(),
                    directory_path.to_string_lossy().as_ref(),
                ) == NormalizedPathRelation::LeftAncestor
            })
        })
        .collect::<Vec<_>>();
    directories.sort_by(|left, right| {
        right
            .relative_path()
            .components()
            .count()
            .cmp(&left.relative_path().components().count())
            .then_with(|| left.relative_path().cmp(right.relative_path()))
    });

    for directory in directories {
        let parent_relative = directory.relative_path().parent().ok_or_else(|| {
            ServiceError::invalid_input("verified residue directory has no parent")
        })?;
        let expected_parent_identity = parent_identities.get(parent_relative).ok_or_else(|| {
            ServiceError::invalid_input("verified residue directory parent was not observed")
        })?;
        let parent_path = root_path.join(parent_relative);
        let Some(parent) = open_observed_directory(&parent_path, expected_parent_identity)? else {
            continue;
        };
        let directory_path = root_path.join(directory.relative_path());
        let leaf = LeafName::from_path(&directory_path)?;
        parent.remove_empty_dir(
            &leaf,
            directory.native_identity(),
            AuthorityMode::CooperativeSameUid,
        )?;
    }

    Ok(())
}

fn open_observed_directory(
    path: &Path,
    expected_identity: &str,
) -> Result<Option<VerifiedDir>, ServiceError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => {
            let directory = VerifiedDir::open_absolute_components(path, None)?;
            Ok((directory.identity() == expected_identity).then_some(directory))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(crate::failed(format!(
            "cannot inspect residue directory {}: {error}",
            path.display()
        ))),
    }
}

pub(super) fn target_step(target: &LocalCleanupTarget) -> CleanupStep {
    CleanupStep {
        item_id: target.item.item_id.clone(),
        category: target.item.category,
        outcome: CleanupStepOutcome::Blocked,
        issue: target.item.issue.clone(),
    }
}

fn claim_issue(
    claim: &CanonicalClaim,
    observed_category: ClaimCategory,
    observed_removable: bool,
) -> Option<LeftoverIssue> {
    let code = match claim.blocked_reason() {
        Some(BlockReason::UnsupportedOwner) => Some(LeftoverIssueCode::UnsupportedOwner),
        Some(BlockReason::PrivateOriginalCustody) => {
            Some(LeftoverIssueCode::PrivateOriginalCustody)
        }
        Some(BlockReason::UnprovenOwnership) => Some(LeftoverIssueCode::UnprovenOwnership),
        None => None,
    }
    .or_else(|| (!claim.removable()).then_some(LeftoverIssueCode::UnprovenOwnership))
    .or_else(|| {
        (claim.category() == ClaimCategory::PrivateCustody)
            .then_some(LeftoverIssueCode::PrivateOriginalCustody)
    })
    .or_else(|| {
        (claim.category() != observed_category || claim.removable() != observed_removable)
            .then_some(LeftoverIssueCode::UnprovenOwnership)
    })
    .or_else(|| {
        (claim.category() == ClaimCategory::Directory && claim.native_identity().is_none())
            .then_some(LeftoverIssueCode::UnprovenOwnership)
    })
    .or_else(|| {
        (claim.category() != ClaimCategory::Directory && claim.sha256().is_none())
            .then_some(LeftoverIssueCode::UnprovenOwnership)
    });
    code.map(|code| LeftoverIssue {
        code,
        detail: Some(match code {
            LeftoverIssueCode::PrivateOriginalCustody => {
                "This recorded file preserves original game content.".to_owned()
            }
            LeftoverIssueCode::UnsupportedOwner => {
                "This owner has no remove-only cleanup policy.".to_owned()
            }
            _ => "This path does not have exact remove-only ownership evidence.".to_owned(),
        }),
    })
}

fn display_category(category: ClaimCategory) -> LeftoverCategory {
    match category {
        ClaimCategory::OptiscalerFile => LeftoverCategory::OptiscalerFile,
        ClaimCategory::AddonFile => LeftoverCategory::AddonFile,
        ClaimCategory::ComponentFile => LeftoverCategory::ComponentFile,
        ClaimCategory::PrivateCustody => LeftoverCategory::PrivateCustody,
        ClaimCategory::Directory => LeftoverCategory::Directory,
    }
}

fn entry_matches(claim: &CanonicalClaim, entry: &EntryObservation) -> bool {
    let identity_matches = claim
        .native_identity()
        .is_none_or(|identity| identity == entry.identity);
    if !identity_matches {
        return false;
    }
    if claim.category() == ClaimCategory::Directory {
        entry.kind == EntryKind::Directory && entry.digest.is_none()
    } else {
        entry.kind == EntryKind::File
            && claim
                .sha256()
                .is_some_and(|sha| entry.digest.as_deref() == Some(sha.as_str()))
    }
}

pub(super) fn same_owners(left: &LocalCleanupOwners, right: &LocalCleanupOwners) -> bool {
    let addon_matches = match (left.addon(), right.addon()) {
        (Some(left), Some(right)) => left.eq_ignoring_persistence_timestamps(right),
        (None, None) => true,
        _ => false,
    };

    left.game() == right.game()
        && left.availability() == right.availability()
        && left.components() == right.components()
        && left.baselines() == right.baselines()
        && left.optiscaler_state() == right.optiscaler_state()
        && left.topology() == right.topology()
        && addon_matches
        && left.engine_journal() == right.engine_journal()
}

fn same_path(left: &str, right: &str) -> bool {
    normalized_path_key(left) == normalized_path_key(right)
}
