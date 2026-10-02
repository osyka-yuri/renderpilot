//! Read-only proposals and explicit cleanup for supported leftovers.

use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{GameId, GameInstallation, InstalledAddonHostKind};
use sha2::{Digest, Sha256};

use crate::{
    Context, ServiceError,
    catalog::{
        installation_lifecycle::{coordinator, residue::RetiredRootObservation},
        leftovers::local_removal::LocalCleanupTarget,
    },
    context::{IssuedRetiredLeftoversIntent, RetiredLeftoversObservation},
    game_mutation_lock::GameMutationGuard,
    mutation_boundary::enter_game_observation_boundary,
};

mod dto;
mod external_cleanup;
mod local_removal;
mod observation;
mod projection;
mod revision;
#[cfg(test)]
mod tests;

pub use self::dto::*;
use self::{
    observation::{capture_observation, root_issue, root_state},
    projection::{build_items, issue, proposal_for},
    revision::semantic_revision,
};

const LEAVE_SETTING_PREFIX: &str = "catalog.retired_game_leftovers.leave.v1.";

/// Lists current cleanup proposals and isolated discovery issues without mutation.
pub fn list_retired_game_leftovers(
    context: &Context,
) -> Result<ListRetiredGameLeftoversOutput, ServiceError> {
    let _catalog_guard = context.catalog_scan_guard();
    let games = context.storage().list_games()?;
    let registered_ids = games.iter().map(|game| game.id()).collect::<HashSet<_>>();
    let mut proposals = Vec::new();
    let mut issues = Vec::new();
    let mut eligible = HashSet::new();

    for game in &games {
        let _game_guard = enter_game_observation_boundary(game.id());
        match capture_observation(context, game, false) {
            Ok(Some(captured)) => {
                let Some(root_state) = root_state(&captured.observation.root) else {
                    issues.push(game_issue(
                        game.id(),
                        root_issue(&captured.observation.root),
                    ));
                    continue;
                };
                let items = build_items(context, game, &captured.observation);
                if items.is_empty() {
                    continue;
                }
                let Some(proposal) = make_proposal(
                    context,
                    game,
                    root_state,
                    &captured.observation,
                    items,
                    true,
                )?
                else {
                    continue;
                };
                if proposal.items.iter().any(|item| {
                    item.issue
                        .as_ref()
                        .is_some_and(|issue| issue.code == LeftoverIssueCode::ForeignPending)
                }) {
                    issues.push(game_issue(
                        game.id(),
                        issue(
                            LeftoverIssueCode::PendingConflict,
                            Some(
                                "A pending operation must use its existing recovery path."
                                    .to_owned(),
                            ),
                        ),
                    ));
                }
                if !proposal.can_clean
                    && !proposal.items.is_empty()
                    && !captured.observation.pending_nvapi.is_empty()
                {
                    issues.push(game_issue(
                        game.id(),
                        issue(
                            LeftoverIssueCode::PendingConflict,
                            Some("A game-scoped native operation is pending.".to_owned()),
                        ),
                    ));
                }
                eligible.insert(game.id().clone());
                proposals.push(proposal);
            }
            Ok(None) => {}
            Err(error) => issues.push(game_issue(
                game.id(),
                issue(LeftoverIssueCode::OperationFailed, Some(error.to_string())),
            )),
        }
    }

    // Orphan owner rows are reported without creating an actionable game
    // proposal: no registered installation identity remains to authorize it.
    for addon in context.storage().list_installed_addons()? {
        if !registered_ids.contains(&addon.game_id()) {
            issues.push(LeftoverGameIssue {
                game_id: Some(addon.game_id().as_str().to_owned()),
                issue: issue(
                    LeftoverIssueCode::UnsupportedOwner,
                    Some(
                        "A retained external owner has no registered installation identity."
                            .to_owned(),
                    ),
                ),
            });
        }
    }

    context.retain_retired_leftovers_intents(&eligible);
    Ok(ListRetiredGameLeftoversOutput { proposals, issues })
}

/// Removes only exact current items bound to the most recent List proposal.
pub fn clean_retired_game_leftovers(
    context: &Context,
    game_id: &GameId,
    intent: &str,
) -> Result<CleanupOutcome, ServiceError> {
    let issued = context.consume_retired_leftovers_intent(game_id);
    let _catalog_guard = context.catalog_scan_guard();
    let Some(game) = context.storage().find_game(game_id)? else {
        return Ok(stale_cleanup(
            game_id,
            None,
            issue(
                LeftoverIssueCode::StaleIntent,
                Some("The game is no longer registered.".to_owned()),
            ),
        ));
    };
    let game_guard = enter_game_observation_boundary(game_id);
    let Some(captured) = (match capture_for_action(context, &game) {
        Ok(captured) => captured,
        Err(error) => {
            return Ok(stale_cleanup(
                game_id,
                None,
                issue(LeftoverIssueCode::OperationFailed, Some(error.to_string())),
            ));
        }
    }) else {
        return Ok(stale_cleanup(
            game_id,
            None,
            issue(
                LeftoverIssueCode::ActiveOwnershipConflict,
                Some("The registered installation is no longer absent.".to_owned()),
            ),
        ));
    };
    let Some(initial_root_state) = root_state(&captured.observation.root) else {
        return Ok(stale_cleanup(
            game_id,
            None,
            root_issue(&captured.observation.root),
        ));
    };
    let initial_items = build_items(context, &game, &captured.observation);
    let initial_revision = semantic_revision(&captured.observation, &initial_items)?;
    if !issued.as_ref().is_some_and(|issued| {
        issued.token == intent && issued.semantic_revision == initial_revision
    }) {
        let remaining = make_proposal(
            context,
            &game,
            initial_root_state,
            &captured.observation,
            initial_items,
            true,
        )?;
        return Ok(stale_cleanup(
            game_id,
            remaining,
            issue(
                LeftoverIssueCode::StaleIntent,
                Some(
                    "The proposal changed. Review the current ownership before cleaning."
                        .to_owned(),
                ),
            ),
        ));
    }

    let mut steps = BTreeMap::<String, CleanupStep>::new();
    let mut local_targets = local_removal::project_local_targets(&captured.observation);
    local_targets.sort_by(local_action_order);
    if !captured.observation.pending_nvapi.is_empty() {
        let pending_issue = issue(
            LeftoverIssueCode::PendingConflict,
            Some("A game-scoped native operation is pending.".to_owned()),
        );
        for item in &initial_items {
            steps.insert(
                item.item_id.clone(),
                blocked_step(item, pending_issue.clone()),
            );
        }
    } else if !captured.observation.pending_files.is_empty() {
        let pending_issue = issue(
            LeftoverIssueCode::PendingConflict,
            Some("A filesystem operation for this game must finish through its existing recovery path.".to_owned()),
        );
        for item in &initial_items {
            steps.insert(
                item.item_id.clone(),
                blocked_step(item, pending_issue.clone()),
            );
        }
    } else {
        for target in &local_targets {
            let step = if target.item.disposition == LeftoverDisposition::Blocked {
                local_removal::target_step(target)
            } else {
                local_removal::remove_local_target(
                    context,
                    game_id,
                    &game_guard,
                    &captured.observation.owners,
                    target,
                )
            };
            steps.insert(step.item_id.clone(), step);
        }

        if let Some(item) = initial_items
            .iter()
            .find(|item| item.category == LeftoverCategory::EngineConfig)
        {
            let step = if item.disposition == LeftoverDisposition::Blocked {
                blocked_step(
                    item,
                    item.issue
                        .clone()
                        .unwrap_or_else(|| issue(LeftoverIssueCode::UnsupportedOwner, None)),
                )
            } else {
                match external_cleanup::release_engine_owner(
                    context,
                    &game_guard,
                    &captured.observation.owners,
                    &item.item_id,
                ) {
                    Ok(step) => step,
                    Err(error) => failed_step(item, error.to_string()),
                }
            };
            steps.insert(step.item_id.clone(), step);
        }

        for item in initial_items
            .iter()
            .filter(|item| item.category == LeftoverCategory::VulkanRegistration)
        {
            let step = if item.disposition == LeftoverDisposition::Blocked {
                blocked_step(
                    item,
                    item.issue
                        .clone()
                        .unwrap_or_else(|| issue(LeftoverIssueCode::ForeignPending, None)),
                )
            } else {
                let current = context.storage().read_local_cleanup_owners(game_id)?;
                match external_cleanup::unregister_vulkan_owner(
                    context,
                    &game_guard,
                    captured
                        ._shared_guard
                        .as_ref()
                        .expect("cleanup retains shared lock"),
                    &current,
                    &captured.observation,
                    &item.item_id,
                ) {
                    Ok(step) => step,
                    Err(error) => failed_step(item, error.to_string()),
                }
            };
            steps.insert(step.item_id.clone(), step);
        }
    }

    for item in &initial_items {
        steps.entry(item.item_id.clone()).or_insert_with(|| {
            blocked_step(
                item,
                item.issue.clone().unwrap_or_else(|| {
                    issue(
                        LeftoverIssueCode::UnsupportedOwner,
                        Some("This cleanup item has no supported action.".to_owned()),
                    )
                }),
            )
        });
    }

    let steps = initial_items
        .iter()
        .filter_map(|item| steps.remove(&item.item_id))
        .collect::<Vec<_>>();
    let any_success = steps.iter().any(step_succeeded);
    let any_blocked = steps.iter().any(|step| {
        matches!(
            step.outcome,
            CleanupStepOutcome::Blocked | CleanupStepOutcome::Failed
        )
    });

    let pending_nvapi_empty = captured.observation.pending_nvapi.is_empty();
    let mut cleanup_issue = None;
    if !any_blocked {
        if let RetiredRootObservation::ResidueOnly(root) = &captured.observation.root
            && let Err(error) = local_removal::prune_empty_directories(root)
        {
            cleanup_issue = Some(issue(
                LeftoverIssueCode::OperationFailed,
                Some(error.to_string()),
            ));
        }
        if let Err(error) = collect_if_empty_and_released(context, &game, &game_guard) {
            cleanup_issue.get_or_insert_with(|| {
                issue(LeftoverIssueCode::OperationFailed, Some(error.to_string()))
            });
        }
    }

    // Drop shared authority before taking it again to build the truthful
    // response from post-action native state. The game lock remains held.
    drop(captured);
    let (remaining, refresh_issue) =
        if let Some(current_game) = current_registered_game(context, game_id)? {
            match capture_observation(context, &current_game, true) {
                Ok(Some(current)) => {
                    if let Some(state) = root_state(&current.observation.root) {
                        let items = build_items(context, &current_game, &current.observation);
                        match make_proposal(
                            context,
                            &current_game,
                            state,
                            &current.observation,
                            items,
                            false,
                        ) {
                            Ok(proposal) => (proposal, None),
                            Err(error) => (
                                None,
                                Some(issue(
                                    LeftoverIssueCode::OperationFailed,
                                    Some(error.to_string()),
                                )),
                            ),
                        }
                    } else {
                        (None, Some(root_issue(&current.observation.root)))
                    }
                }
                Ok(None) => (None, None),
                Err(error) => (
                    None,
                    Some(issue(
                        LeftoverIssueCode::OperationFailed,
                        Some(error.to_string()),
                    )),
                ),
            }
        } else {
            (None, None)
        };

    let status = if remaining.is_none()
        && !any_blocked
        && cleanup_issue.is_none()
        && refresh_issue.is_none()
    {
        CleanupStatus::Complete
    } else if any_success {
        CleanupStatus::Partial
    } else {
        CleanupStatus::Blocked
    };
    let outcome_issue = cleanup_issue.or(refresh_issue).or_else(|| {
        (status != CleanupStatus::Complete).then(|| {
            issue(
                if !pending_nvapi_empty {
                    LeftoverIssueCode::PendingConflict
                } else {
                    LeftoverIssueCode::OperationFailed
                },
                Some("Some supported leftovers remain and can be reviewed again.".to_owned()),
            )
        })
    });
    Ok(CleanupOutcome {
        game_id: game_id.as_str().to_owned(),
        status,
        steps,
        remaining_proposal: remaining,
        issue: outcome_issue,
    })
}

/// Records a Leave choice only while the current semantic proposal is the one
/// the user saw. No canonical/native state is modified.
pub fn leave_retired_game_leftovers(
    context: &Context,
    game_id: &GameId,
    intent: &str,
) -> Result<LeaveOutcome, ServiceError> {
    let issued = context.consume_retired_leftovers_intent(game_id);
    let _catalog_guard = context.catalog_scan_guard();
    let Some(game) = context.storage().find_game(game_id)? else {
        return Ok(stale_leave(game_id, None, LeftoverIssueCode::StaleIntent));
    };
    let _game_guard = enter_game_observation_boundary(game_id);
    let Some(captured) = (match capture_for_action(context, &game) {
        Ok(captured) => captured,
        Err(_) => {
            return Ok(stale_leave(
                game_id,
                None,
                LeftoverIssueCode::OperationFailed,
            ));
        }
    }) else {
        return Ok(stale_leave(
            game_id,
            None,
            LeftoverIssueCode::ActiveOwnershipConflict,
        ));
    };
    let Some(root_state) = root_state(&captured.observation.root) else {
        return Ok(stale_leave(
            game_id,
            None,
            root_issue(&captured.observation.root).code,
        ));
    };
    let items = build_items(context, &game, &captured.observation);
    let revision = semantic_revision(&captured.observation, &items)?;
    let leave_key = leave_setting_key(game_id);
    let was_left = context.storage().get_setting(&leave_key)?.as_deref() == Some(revision.as_str());
    if !items.is_empty()
        && !was_left
        && issued
            .as_ref()
            .is_some_and(|issued| issued.token == intent && issued.semantic_revision == revision)
    {
        context.storage().set_setting(&leave_key, &revision)?;
        return Ok(LeaveOutcome {
            game_id: game_id.as_str().to_owned(),
            status: LeaveStatus::Left,
            leave_revision: Some(revision),
            remaining_proposal: None,
            issue: None,
        });
    }

    let remaining = make_proposal(
        context,
        &game,
        root_state,
        &captured.observation,
        items,
        true,
    )?;
    if let Some(proposal) = remaining {
        return Ok(LeaveOutcome {
            game_id: game_id.as_str().to_owned(),
            status: LeaveStatus::Stale,
            leave_revision: None,
            remaining_proposal: Some(proposal),
            issue: Some(issue(
                LeftoverIssueCode::StaleIntent,
                Some(
                    "The proposal changed. Review the current ownership before choosing again."
                        .to_owned(),
                ),
            )),
        });
    }
    Ok(stale_leave(game_id, None, LeftoverIssueCode::StaleIntent))
}

fn capture_for_action(
    context: &Context,
    game: &GameInstallation,
) -> Result<Option<observation::CapturedObservation>, ServiceError> {
    capture_observation(context, game, true)
}

fn make_proposal(
    context: &Context,
    game: &GameInstallation,
    root_state: LeftoverRootState,
    observation: &RetiredLeftoversObservation,
    items: Vec<LeftoverItem>,
    respect_leave: bool,
) -> Result<Option<LeftoverProposal>, ServiceError> {
    if items.is_empty() {
        return Ok(None);
    }
    let revision = semantic_revision(observation, &items)?;
    if respect_leave
        && context
            .storage()
            .get_setting(&leave_setting_key(game.id()))?
            .as_deref()
            == Some(revision.as_str())
    {
        return Ok(None);
    }
    let issued = IssuedRetiredLeftoversIntent {
        token: fresh_intent(),
        semantic_revision: revision.clone(),
    };
    let proposal = proposal_for(game, root_state, revision, &issued, observation, items);
    context.issue_retired_leftovers_intent(game.id().clone(), issued);
    Ok(Some(proposal))
}

fn collect_if_empty_and_released(
    context: &Context,
    game: &GameInstallation,
    game_guard: &GameMutationGuard,
) -> Result<(), ServiceError> {
    let owners = context.storage().read_local_cleanup_owners(game.id())?;
    if owners.game() != game
        || !owners.availability().is_absent()
        || owners.addon().is_some_and(|addon| {
            addon.host_kind() == Some(InstalledAddonHostKind::SharedVulkanLayer)
        })
        || owners.engine_journal().is_some()
        || !context
            .storage()
            .pending_file_mutations_for_game(game.id())?
            .is_empty()
        || context
            .storage()
            .pending_shared_vulkan_mutation()?
            .is_some()
        || context
            .storage()
            .list_pending_nvapi_operations()?
            .iter()
            .any(|row| row.game_id.as_deref() == Some(game.id().as_str()))
    {
        return Ok(());
    }
    match coordinator::observe_registered_root_under_lock(context, game_guard, game)? {
        coordinator::RegisteredRootObservation::ConfirmedAbsent {
            collection_error: Some(message),
            ..
        } => Err(ServiceError::command_failed(message)),
        _ => Ok(()),
    }
}

fn current_registered_game(
    context: &Context,
    game_id: &GameId,
) -> Result<Option<GameInstallation>, ServiceError> {
    Ok(context.storage().find_game(game_id)?)
}

fn local_action_order(left: &LocalCleanupTarget, right: &LocalCleanupTarget) -> std::cmp::Ordering {
    use crate::catalog::installation_lifecycle::residue::LocalCleanupCategory;
    match (left.claim.category(), right.claim.category()) {
        (LocalCleanupCategory::Directory, LocalCleanupCategory::Directory) => {
            let left_depth = Path::new(left.claim.path().as_str()).components().count();
            let right_depth = Path::new(right.claim.path().as_str()).components().count();
            right_depth
                .cmp(&left_depth)
                .then_with(|| left.claim.path().as_str().cmp(right.claim.path().as_str()))
        }
        (LocalCleanupCategory::Directory, _) => std::cmp::Ordering::Greater,
        (_, LocalCleanupCategory::Directory) => std::cmp::Ordering::Less,
        _ => left.claim.path().as_str().cmp(right.claim.path().as_str()),
    }
}

fn blocked_step(item: &LeftoverItem, issue_value: LeftoverIssue) -> CleanupStep {
    CleanupStep {
        item_id: item.item_id.clone(),
        category: item.category,
        outcome: CleanupStepOutcome::Blocked,
        issue: Some(issue_value),
    }
}

fn failed_step(item: &LeftoverItem, detail: String) -> CleanupStep {
    CleanupStep {
        item_id: item.item_id.clone(),
        category: item.category,
        outcome: CleanupStepOutcome::Failed,
        issue: Some(issue(LeftoverIssueCode::OperationFailed, Some(detail))),
    }
}

fn step_succeeded(step: &CleanupStep) -> bool {
    matches!(
        step.outcome,
        CleanupStepOutcome::Removed
            | CleanupStepOutcome::Released
            | CleanupStepOutcome::AlreadyAbsent
            | CleanupStepOutcome::Recovered
    )
}

fn stale_cleanup(
    game_id: &GameId,
    remaining_proposal: Option<LeftoverProposal>,
    issue_value: LeftoverIssue,
) -> CleanupOutcome {
    CleanupOutcome {
        game_id: game_id.as_str().to_owned(),
        status: CleanupStatus::Stale,
        steps: Vec::new(),
        remaining_proposal,
        issue: Some(issue_value),
    }
}

fn stale_leave(
    game_id: &GameId,
    remaining_proposal: Option<LeftoverProposal>,
    code: LeftoverIssueCode,
) -> LeaveOutcome {
    LeaveOutcome {
        game_id: game_id.as_str().to_owned(),
        status: LeaveStatus::Stale,
        leave_revision: None,
        remaining_proposal,
        issue: Some(issue(
            code,
            Some("The issued proposal is no longer current.".to_owned()),
        )),
    }
}

fn game_issue(game_id: &GameId, issue: LeftoverIssue) -> LeftoverGameIssue {
    LeftoverGameIssue {
        game_id: Some(game_id.as_str().to_owned()),
        issue,
    }
}

fn fresh_intent() -> String {
    format!("intent-{}", ulid::Ulid::generate())
}

fn leave_setting_key(game_id: &GameId) -> String {
    let digest = Sha256::digest(game_id.as_str().as_bytes());
    format!("{LEAVE_SETTING_PREFIX}{}", hex::encode(digest))
}
