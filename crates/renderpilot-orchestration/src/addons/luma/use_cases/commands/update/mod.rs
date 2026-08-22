//! Applies an update to an installed Luma add-on.
//!
//! Unlike RenoDX's single tracked add-on file, a Luma install's payload is a
//! whole tree with no per-file upstream checksums — so an update re-fetches the
//! **entire** release ZIP and diffs its flat, `/`-normalized relative-path set
//! against the record's `created_files` (minus the host slot, which is tracked
//! and updated separately) to find what was added, changed, or removed,
//! including a renamed main `.addon` (upstream renaming the asset), which
//! shows up as one remove plus one add (see `diff::compute_diff`).
//!
//! ## Control flow
//!
//! ```text
//! update()
//!   ├─ snapshot under game_mutation_lock (record + layout)
//!   ├─ prepare::  network outside lock (ZIP / host / dgVoodoo)
//!   └─ apply under lock:
//!        revalidate → engine sentinel → apply set-diff
//!        → host / dgVoodoo steps → record_rebuild → commit
//!        on failure: rollback::SetDiffRollback (+ sentinel policy)
//! ```
//!
//! Stages live in sibling modules (`prepare`, `apply`, `diff`, `host`,
//! `dgvoodoo`, `record_rebuild`, `revalidate`, `rollback`, `layout`). This
//! file is the only orchestration entry.
//!
//! Every disk mutation is captured in a `rollback::SetDiffRollback` as it
//! happens, so any later step's failure can undo everything applied so far.
//!
//! All remote inputs are prepared before the sentinel opens. Network work runs
//! **outside** the per-game `game_mutation_lock` so a slow ZIP/host download does not
//! block peer availability for the whole transfer; the lock is held only for
//! the initial snapshot and the final revalidation + disk apply (same 3-phase
//! contract as Luma install). The marker spans every disk write plus record
//! persistence and is cleared only after commit or a provably complete
//! rollback. A marker that predates the attempt survives a rollback and forces
//! a full payload convergence pass.

mod apply;
mod dgvoodoo;
mod diff;
mod host;
mod layout;
mod prepare;
mod record_rebuild;
mod revalidate;
mod rollback;
#[cfg(test)]
mod test_fixtures;

#[cfg(test)]
mod host_only_payload_guard_tests;
#[cfg(test)]
mod orchestration_tests;
#[cfg(test)]
mod snapshot_tests;
#[cfg(test)]
mod target_match_tests;

use std::path::PathBuf;

use renderpilot_domain::{AddonKind, GameId};

use layout::resolve_update_layout;
use prepare::PreparedUpdate;
use revalidate::{
    ensure_host_only_payload_still_intact, ensure_prepared_target_still_matches,
    ensure_record_still_matches_snapshot,
};
use rollback::UpdateFailure;

use crate::addons::engine;
use crate::addons::exclusivity;
use crate::addons::luma::errors;
use crate::addons::luma::game_context::require_game;
use crate::addons::luma::types::LumaManifest;
use crate::addons::records;
use crate::addons::reshade::types::ReshadeSourceCatalog;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Complete request for a Luma update or repair.
pub struct UpdateRequest<'a> {
    /// Application services and storage.
    pub context: &'a Context,
    /// Luma release manifest used to resolve the update.
    pub manifest: &'a LumaManifest,
    /// ReShade source catalog used when the host binary must be updated.
    pub reshade_sources: &'a ReshadeSourceCatalog,
    /// Game whose installation is being updated.
    pub game_id: &'a GameId,
    /// Whether to replace the full managed file set even when versions match.
    pub force_full: bool,
    /// Fresh permit authorizing this game-file mutation.
    pub safety: crate::GameSafetyPermit,
    /// Optional download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

/// Applies an update to the installed Luma add-on and, when needed, its ReShade
/// host.
///
/// When `force_full` is true (desktop Repair), prepare always re-fetches the
/// release ZIP and runs a full set-diff even if the ETag pre-check reports current.
pub async fn update(request: UpdateRequest<'_>) -> Result<(), ServiceError> {
    let UpdateRequest {
        context,
        manifest,
        reshade_sources,
        game_id,
        force_full,
        safety,
        progress,
    } = request;
    // Phase 1: snapshot under the per-game lock, then release for network work.
    let (snapshot, had_torn_marker) = {
        let _guard =
            crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
        let _game = require_game(context, game_id)?;
        let record = records::record_of_kind(context, game_id, AddonKind::Luma)?
            .ok_or_else(errors::not_installed)?;
        let layout = resolve_update_layout(context, manifest, game_id, &record)?;
        let had_torn_marker = engine::is_install_torn(layout.sentinel_dir(), AddonKind::Luma);
        (record, had_torn_marker)
    };

    // Phase 2: downloads and validation only — no game-folder mutation.
    let prepared = prepare::prepare_update(
        context,
        manifest,
        reshade_sources,
        &snapshot,
        progress,
        had_torn_marker,
        force_full,
    )
    .await?;

    // Phase 3: re-lock, revalidate, apply under sentinel.
    let guard =
        crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
    let record = crate::addons::records::record_of_kind(context, game_id, AddonKind::Luma)?
        .ok_or_else(errors::not_installed)?;
    ensure_record_still_matches_snapshot(&snapshot, &record)?;
    let layout = resolve_update_layout(context, manifest, game_id, &record)?;
    let current_target = crate::addons::luma::use_cases::update_target::resolve_update_target(
        context, manifest, game_id,
    )?;
    ensure_prepared_target_still_matches(&prepared, &layout, current_target.as_ref())?;
    let scan_dirs = layout.scan_dir_paths();
    exclusivity::ensure_not_blocked(
        context,
        game_id,
        AddonKind::Luma,
        Some(scan_dirs.as_slice()),
    )?;
    // A torn install that appeared after a host-only prepare needs a full
    // payload pass; ask the caller to retry rather than apply a stale plan.
    if matches!(prepared, PreparedUpdate::HostOnly(_))
        && engine::is_install_torn(layout.sentinel_dir(), AddonKind::Luma)
    {
        return Err(errors::state_changed_retry_update());
    }
    // Payload files deleted while network prepare ran unlocked: HostOnly was
    // planned against an intact tree and would leave a broken install.
    ensure_host_only_payload_still_intact(&prepared, &record)?;

    let game_root = crate::catalog::game_root_for_mutation(context.storage(), game_id, None)
        .map_err(|_| errors::invalid("game is no longer present in the library".to_owned()))?;
    let targets = update_mutation_targets(context, &record, &layout, &prepared, game_root)?;
    crate::FileSafetyAuthority::new().authorize_game_commit(
        context,
        crate::addons::mutation_features::LUMA_UPDATE,
        &guard,
        &safety,
        || {
            let mutation = crate::addons::durable::prepare_targets_mutation(
                crate::addons::durable::TargetsMutation {
                    context,
                    guard: &guard,
                    targets,
                    feature: crate::addons::mutation_features::LUMA_UPDATE,
                    game_id,
                },
            )?;

            let sentinel =
                match engine::OperationSentinel::begin(layout.sentinel_dir(), AddonKind::Luma) {
                    Ok(sentinel) => sentinel,
                    Err(error) => {
                        return mutation.commit_or_rollback(
                            context.storage(),
                            || Err::<(), _>(error),
                            |_| {},
                            || {},
                        );
                    }
                };
            let result = match prepared {
                PreparedUpdate::HostOnly(prepared) => {
                    apply::apply_host_only(context, &record, *prepared, progress, mutation.id())
                }
                PreparedUpdate::Full(prepared) => {
                    // `payload_dir` is the set-diff root (may differ from game_dir when
                    // ReShade AddonPath is split). Host/dgVoodoo use target.game_dir.
                    apply::apply_set_diff_with_mutation(
                        context,
                        &record,
                        &layout.payload_dir,
                        *prepared,
                        progress,
                        mutation.id(),
                    )
                }
            };
            finish_durable_transaction(context, sentinel, mutation, result)
        },
    )
}

fn finish_durable_transaction(
    context: &Context,
    sentinel: engine::OperationSentinel,
    mutation: crate::file_mutation::DurableFileTransaction,
    result: Result<(), UpdateFailure>,
) -> Result<(), ServiceError> {
    let (result, local_rollback_complete) = match result {
        Ok(()) => (Ok(()), true),
        Err(failure) => (Err(failure.error), failure.rollback_complete),
    };
    crate::addons::durable::finish_sentinel_mutation(
        context,
        sentinel,
        mutation,
        result,
        local_rollback_complete,
        "Luma update",
    )
}

fn update_mutation_targets(
    context: &Context,
    record: &renderpilot_domain::InstalledAddon,
    layout: &layout::UpdateLayout,
    prepared: &PreparedUpdate,
    game_root: PathBuf,
) -> Result<crate::addons::mutation_targets::MutationTargets, ServiceError> {
    let mut extra = vec![engine::sentinel_path(
        layout.sentinel_dir(),
        AddonKind::Luma,
    )];
    if let Some(path) = prepared.host_write_path() {
        extra.push(path.to_path_buf());
    }
    extra.extend(prepared.dgvoodoo_write_paths());

    if let PreparedUpdate::Full(prepared) = prepared {
        extra.extend(
            prepared
                .payload
                .files
                .iter()
                .map(|file| layout.payload_dir.join(&file.relative_path)),
        );
        extra.extend(prepared.dependency_paths.iter().cloned());

        // Same cascade selector as apply (`cascade_for_disappearing_owned`) so
        // the durable snapshot set cannot diverge from the apply-time plan.
        let cascade = crate::addons::luma::dlss::cascade_for_disappearing_owned(
            context.storage(),
            record,
            &prepared.payload.files,
        )?;
        extra.extend(cascade.mutation_paths);
    }

    Ok(
        crate::addons::mutation_targets::MutationTargets::for_record(
            record,
            [game_root, layout.payload_dir.clone()],
            extra,
        ),
    )
}
