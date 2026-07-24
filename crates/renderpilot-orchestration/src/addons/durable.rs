//! Shared durable-transaction ceremonies for add-on commands.
//!
//! Feature-specific planning stays in each tool; this module owns the repeated
//! engine-sentinel + `DurableFileTransaction` + `commit_game_mutation` wiring so
//! install/update/uninstall call sites cannot drift.
//!
//! ## Ceremony paths
//!
//! **Closed** (single `work` closure owns FS + DB half):
//! - [`run_install_mutation`] — install with engine sentinel finish hooks
//! - [`run_targets_mutation`] — in-place update / companion mutations
//! - [`run_uninstall_workset`] — uninstall / orphan cleanup (`Files` | `MetadataOnly`)
//!
//! **Multi-step** (caller opens sentinel and applies phases itself):
//! - [`prepare_targets_mutation`] → apply → [`finish_sentinel_mutation`]
//!
//! Prefer these helpers over hand-rolling [`file_mutation::DurableMutation`].
//! Catalog swap stays on the primitive (`subject_id` is a component, not a game).

use std::cell::RefCell;

use renderpilot_domain::{GameId, InstalledAddon};
use renderpilot_storage_sqlite::{GameMutationCommit, InstalledAddonMutation};

use crate::addons::engine::{OperationSentinel, PendingInstallCommit};
use crate::addons::mutation_targets::{DurableWorkset, MutationTargets};
use crate::file_mutation::{self, DurableFileTransaction, DurableMutation};
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

/// Applies filesystem install work under a durable transaction, upserts the
/// install record, and finishes the engine sentinel only after DB commit.
///
/// `install` must leave the engine sentinel open (`PendingInstallCommit`) until
/// this helper calls `finish_committed` / `finish_rolled_back`.
pub(crate) fn run_install_mutation(
    context: &Context,
    guard: &GameMutationGuard,
    targets: MutationTargets,
    feature: &str,
    game_id: &GameId,
    install: impl FnOnce() -> Result<(InstalledAddon, PendingInstallCommit), ServiceError>,
) -> Result<InstalledAddon, ServiceError> {
    let pending_commit = RefCell::new(None);
    run_targets_mutation(
        TargetsMutation {
            context,
            guard,
            targets,
            feature,
            game_id,
        },
        |mutation_id| -> Result<InstalledAddon, ServiceError> {
            let (record, commit) = install()?;
            *pending_commit.borrow_mut() = Some(commit);
            context.storage().commit_game_mutation(GameMutationCommit {
                game_id: record.game_id(),
                component_set: None,
                baseline_mutations: &[],
                addon: InstalledAddonMutation::Upsert(&record),
                mutation_id: Some(mutation_id),
            })?;
            Ok(record)
        },
        |_| {
            if let Some(commit) = pending_commit.borrow_mut().take() {
                commit.finish_committed();
            }
        },
        || {
            if let Some(commit) = pending_commit.borrow_mut().take() {
                commit.finish_rolled_back();
            }
        },
    )
}

/// Inputs for a closed durable mutation against unresolved [`MutationTargets`].
pub(crate) struct TargetsMutation<'a> {
    pub context: &'a Context,
    pub guard: &'a GameMutationGuard,
    pub targets: MutationTargets,
    pub feature: &'a str,
    pub game_id: &'a GameId,
}

/// Resolves [`MutationTargets`] into a scope and runs a durable file mutation.
///
/// `work` must call `commit_game_mutation` with the provided `mutation_id` when
/// the feature has a DB half. Use this for in-place updates and companion
/// mutations that do not open an engine install sentinel.
pub(crate) fn run_targets_mutation<T>(
    req: TargetsMutation<'_>,
    work: impl FnOnce(&str) -> Result<T, ServiceError>,
    on_committed: impl FnOnce(&T),
    on_rolled_back: impl FnOnce(),
) -> Result<T, ServiceError> {
    let (scope, paths) = req.targets.into_scope_and_paths()?;
    file_mutation::run_durable_mutation(
        DurableMutation {
            context: req.context,
            guard: req.guard,
            scope: &scope,
            feature: req.feature,
            subject_id: Some(req.game_id.as_str()),
            paths,
        },
        work,
        on_committed,
        on_rolled_back,
    )
}

/// Resolves [`MutationTargets`] and prepares a durable transaction (multi-step path).
///
/// Pair with [`finish_sentinel_mutation`] after engine work, or use
/// [`run_targets_mutation`] for the closed path.
pub(crate) fn prepare_targets_mutation(
    req: TargetsMutation<'_>,
) -> Result<DurableFileTransaction, ServiceError> {
    let (scope, paths) = req.targets.into_scope_and_paths()?;
    DurableFileTransaction::prepare(
        req.context,
        req.guard,
        &scope,
        req.feature,
        Some(req.game_id.as_str()),
        paths,
    )
}

/// Inputs for uninstall/orphan cleanup against a resolved [`DurableWorkset`].
pub(crate) struct UninstallWorkset<'a> {
    pub context: &'a Context,
    pub guard: &'a GameMutationGuard,
    pub workset: DurableWorkset,
    pub feature: &'a str,
    pub game_id: &'a GameId,
}

/// Runs an uninstall/orphan cleanup against a resolved [`DurableWorkset`].
///
/// - [`DurableWorkset::Files`]: durable snapshot + `apply_and_commit(Some(id))`
/// - [`DurableWorkset::MetadataOnly`]: no file-mutation row; `apply_and_commit(None)`
///
/// `on_committed` runs only after a successful commit (journal side effects).
pub(crate) fn run_uninstall_workset(
    req: UninstallWorkset<'_>,
    mut apply_and_commit: impl FnMut(Option<&str>) -> Result<(), ServiceError>,
    on_committed: impl FnOnce(),
) -> Result<(), ServiceError> {
    match req.workset {
        DurableWorkset::Files { scope, paths } => file_mutation::run_durable_mutation(
            DurableMutation {
                context: req.context,
                guard: req.guard,
                scope: &scope,
                feature: req.feature,
                subject_id: Some(req.game_id.as_str()),
                paths,
            },
            |mutation_id| apply_and_commit(Some(mutation_id)),
            |_| on_committed(),
            || {},
        ),
        DurableWorkset::MetadataOnly => {
            apply_and_commit(None)?;
            on_committed();
            Ok(())
        }
    }
}

/// Completes a durable transaction that already prepared outside
/// [`file_mutation::run_durable_mutation`] and owns an [`OperationSentinel`].
///
/// Used by multi-step updates that open the sentinel before applying work that
/// itself calls `commit_game_mutation`.
pub(crate) fn finish_sentinel_mutation(
    context: &Context,
    sentinel: OperationSentinel,
    mutation: DurableFileTransaction,
    result: Result<(), ServiceError>,
    local_rollback_complete: bool,
    feature_label: &str,
) -> Result<(), ServiceError> {
    if result.is_err() && !local_rollback_complete {
        log::warn!(
            "{feature_label}'s local rollback was incomplete; using the durable before-state"
        );
    }
    let sentinel_path = sentinel.path().display().to_string();
    let sentinel = RefCell::new(Some(sentinel));
    mutation
        .commit_or_rollback(
            context.storage(),
            || result,
            |_| {
                if let Some(sentinel) = sentinel.borrow_mut().take() {
                    sentinel.finish_committed();
                }
            },
            || {
                if let Some(sentinel) = sentinel.borrow_mut().take() {
                    sentinel.finish_rolled_back();
                }
            },
        )
        .map_err(|error| {
            if error.is_rollback_also_failed() {
                log::warn!(
                    "{feature_label} rollback was incomplete; leaving sentinel `{sentinel_path}`: {error}"
                );
            }
            error
        })
}
