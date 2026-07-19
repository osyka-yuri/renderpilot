//! Local mutation phase for a prepared Luma update.
//!
//! Every write is captured in one [`SetDiffRollback`]. This module performs no
//! network work and returns rollback completeness to the outer sentinel owner.

use std::path::{Path, PathBuf};

use renderpilot_domain::{AddonKind, InstalledAddon, PathRef};

use super::dgvoodoo;
use super::diff::{SetDiff, changed_replacements, compute_diff};
use super::host::{self, PreparedHostUpdate};
use super::prepare::{PreparedFullUpdate, PreparedHostOnly};
use super::record_rebuild::{rebuild_record_paths, tracked_addon_source};
use super::rollback::{SetDiffRollback, UpdateFailure, remove_payload_file};
use crate::Context;
use crate::addons::engine::{self, FileOp, InstallOptions, InstallPlan};
use crate::addons::file_update::apply_replacements_with_outcome;
use crate::addons::luma::fetch::types::{LumaPayload, LumaPayloadFile};
use crate::addons::luma::tracking;
use crate::addons::records::replace_source_with_role;
use crate::catalog::cascade::ValidatedRollbackPlan;
use crate::net::ProgressObserver;
use renderpilot_domain::TrackedSourceRole;

pub(super) fn apply_host_only(
    context: &Context,
    record: &InstalledAddon,
    prepared: PreparedHostOnly,
    progress: Option<&ProgressObserver<'_>>,
    mutation_id: &str,
) -> Result<(), UpdateFailure> {
    let PreparedHostOnly {
        target: _,
        mut sources,
        host,
        dgvoodoo,
        addon_version,
    } = prepared;
    let mut rollback = SetDiffRollback::default();
    let host_path = apply_prepared_components(host, dgvoodoo, &mut sources, &mut rollback)?;

    crate::addons::progress::emit_tool_finalizing(progress, AddonKind::Luma);
    if let Err(error) = host::persist_host_only_result(
        context,
        record,
        sources,
        host_path,
        &rollback.added,
        addon_version,
        mutation_id,
    ) {
        return Err(rollback.fail(error, true));
    }
    Ok(())
}

fn apply_prepared_components(
    host: PreparedHostUpdate,
    dgvoodoo: dgvoodoo::PreparedDgVoodooUpdate,
    sources: &mut Vec<renderpilot_domain::TrackedSource>,
    rollback: &mut SetDiffRollback,
) -> Result<Option<PathBuf>, UpdateFailure> {
    let mut host_outcome = match host.apply() {
        Ok(outcome) => outcome,
        Err(failure) => return Err(rollback.fail(failure.error, failure.rollback_complete)),
    };
    if let Some(source) = host_outcome.source.take() {
        replace_source_with_role(sources, TrackedSourceRole::HostBinary, Some(source));
    }
    rollback.replaced.append(&mut host_outcome.originals);

    let mut dgvoodoo_outcome = match dgvoodoo.apply() {
        Ok(outcome) => outcome,
        Err(failure) => return Err(rollback.fail(failure.error, failure.rollback_complete)),
    };
    replace_source_with_role(
        sources,
        TrackedSourceRole::DgVoodooWrapper,
        dgvoodoo_outcome.source.take(),
    );
    rollback.replaced.append(&mut dgvoodoo_outcome.originals);
    rollback
        .added
        .created_files
        .append(&mut dgvoodoo_outcome.receipt.created_files);
    rollback
        .added
        .backed_up_files
        .append(&mut dgvoodoo_outcome.receipt.backed_up_files);

    Ok(host_outcome.host_path)
}

fn install_added_files(
    payload_dir: &Path,
    added: Vec<LumaPayloadFile>,
) -> Result<engine::InstallReceipt, engine::InstallFailure> {
    if added.is_empty() {
        return Ok(engine::InstallReceipt::default());
    }
    let plan = InstallPlan {
        kind: AddonKind::Luma,
        ops: added
            .into_iter()
            .map(|file| FileOp::CreateNested {
                relative_path: file.relative_path,
                bytes: file.bytes,
            })
            .collect(),
    };
    engine::install_with_options_outcome(
        payload_dir,
        &plan,
        InstallOptions {
            manage_sentinel: false,
        },
    )
}

/// Applies a fully prepared release set-diff, host update, managed dependency,
/// and record persistence under the outer transaction sentinel.
///
/// `payload_dir` is the recorded add-on parent (set-diff / nested payload root).
/// When ReShade `AddonPath` is split this is **not** the executable directory;
/// host and dgVoodoo writes use [`ResolvedUpdateTarget::game_dir`] instead.
///
/// `mutation_id` is the durable [`crate::file_mutation::DurableFileTransaction`]
/// id; persistence marks that row committed atomically with the install record.
pub(super) fn apply_set_diff_with_mutation(
    context: &Context,
    record: &InstalledAddon,
    payload_dir: &Path,
    prepared: PreparedFullUpdate,
    progress: Option<&ProgressObserver<'_>>,
    mutation_id: &str,
) -> Result<(), UpdateFailure> {
    let PreparedFullUpdate {
        target,
        mut payload,
        host,
        dgvoodoo,
        dependency_paths,
    } = prepared;
    let planned = plan_set_diff_side_effects(context, record, payload_dir, &payload)?;
    // Side-effects planned; drain file bodies into the set-diff so apply moves
    // them into engine ops without cloning.
    let files = std::mem::take(&mut payload.files);
    let SetDiff {
        added,
        changed,
        removed,
    } = compute_diff(record, payload_dir, files, &dependency_paths);
    let mut rollback = SetDiffRollback::default();

    apply_payload_set_diff(payload_dir, record, added, changed, &removed, &mut rollback)?;

    let mut sources = vec![tracked_addon_source(&target.asset, &payload)];
    let host_path = apply_prepared_components(host, dgvoodoo, &mut sources, &mut rollback)?;
    let next_dlss = apply_host_cascade_dlss(&planned.rollback_specs, planned.dlss, &mut rollback)?;

    crate::addons::progress::emit_tool_finalizing(progress, AddonKind::Luma);
    persist_set_diff_result(PersistSetDiff {
        context,
        record,
        mutation_id,
        rollback: &rollback,
        payload: &payload,
        new_addon_file: &planned.new_addon_file,
        removed: &removed,
        host_path: host_path.as_deref(),
        dependency_paths: &dependency_paths,
        sources,
        managed_files: crate::addons::tracking::ManagedFilesUpdate::Replace(
            next_dlss.into_iter().collect(),
        ),
        next_components: planned.next_components.as_deref(),
        rollback_specs: &planned.rollback_specs,
    })?;

    engine::cleanup_empty_dirs_best_effort(&removed, payload_dir);
    crate::fs::stamp_mtime_best_effort(
        &planned.new_addon_path,
        payload.last_modified.as_deref(),
        None,
    );
    Ok(())
}

struct PlannedSetDiffSideEffects {
    rollback_specs: Vec<ValidatedRollbackPlan>,
    next_components: Option<Vec<renderpilot_domain::GraphicsComponent>>,
    dlss: crate::addons::luma::dlss::PlannedDlss,
    new_addon_path: PathBuf,
    new_addon_file: PathRef,
}

fn plan_set_diff_side_effects(
    context: &Context,
    record: &InstalledAddon,
    payload_dir: &Path,
    payload: &LumaPayload,
) -> Result<PlannedSetDiffSideEffects, UpdateFailure> {
    let existing_dlss = crate::addons::luma::dlss::find_managed_dlss_binding(record);
    let cascade = crate::addons::luma::dlss::cascade_for_disappearing_owned(
        context.storage(),
        record,
        &payload.files,
    )
    .map_err(|error| SetDiffRollback::default().fail(error.into(), true))?;
    let next_components = if cascade.rollback_specs.is_empty() {
        None
    } else {
        Some(cascade.next_components)
    };
    let owned_unwound = existing_dlss.is_some_and(|managed| {
        cascade
            .rollback_specs
            .iter()
            .any(|spec| spec.contains_path(Path::new(managed.path().as_str())))
    });
    let dlss = crate::addons::luma::dlss::plan_update(
        context,
        record.game_id(),
        payload_dir,
        &payload.files,
        existing_dlss,
        owned_unwound,
    )
    .map_err(|error| SetDiffRollback::default().fail(error, true))?;
    let new_addon_path = payload_dir.join(&payload.main_addon_rel);
    let new_addon_file = match crate::addons::record::to_path_ref(&new_addon_path) {
        Ok(path) => path,
        Err(error) => return Err(SetDiffRollback::default().fail(error, true)),
    };
    Ok(PlannedSetDiffSideEffects {
        rollback_specs: cascade.rollback_specs,
        next_components,
        dlss,
        new_addon_path,
        new_addon_file,
    })
}

fn apply_payload_set_diff(
    payload_dir: &Path,
    record: &InstalledAddon,
    added: Vec<LumaPayloadFile>,
    changed: Vec<(PathBuf, Vec<u8>)>,
    removed: &[PathBuf],
    rollback: &mut SetDiffRollback,
) -> Result<(), UpdateFailure> {
    rollback.replaced = match apply_replacements_with_outcome(changed_replacements(changed)) {
        Ok(originals) => originals,
        Err(failure) => return Err(rollback.fail(failure.error, failure.rollback_complete)),
    };

    rollback.added = match install_added_files(payload_dir, added) {
        Ok(receipt) => receipt,
        Err(failure) => {
            return Err(rollback.fail(failure.error, failure.rollback_complete));
        }
    };

    for path in removed {
        match remove_payload_file(record, path) {
            Ok(Some(undo)) => rollback.removed.push(undo),
            Ok(None) => {}
            Err(error) => {
                return Err(rollback.fail(error, true));
            }
        }
    }
    Ok(())
}

fn apply_host_cascade_dlss(
    rollback_specs: &[ValidatedRollbackPlan],
    dlss: crate::addons::luma::dlss::PlannedDlss,
    rollback: &mut SetDiffRollback,
) -> Result<Option<renderpilot_domain::ManagedAddonFile>, UpdateFailure> {
    if let Err(error) = crate::catalog::cascade::apply_cascade_rollback_fs(rollback_specs) {
        // Outer durable TX restores a partial multi-file bundle; local rollback
        // still reverts payload/host writes made before cascade.
        return Err(rollback.fail(error.into(), false));
    }
    if let Err(error) = dlss.execute() {
        return Err(rollback.fail(error, rollback_specs.is_empty()));
    }
    Ok(dlss.binding)
}

/// Named inputs for the set-diff persist half (rebuild record + DB commit).
struct PersistSetDiff<'a> {
    context: &'a Context,
    record: &'a InstalledAddon,
    mutation_id: &'a str,
    rollback: &'a SetDiffRollback,
    payload: &'a LumaPayload,
    new_addon_file: &'a PathRef,
    removed: &'a [PathBuf],
    host_path: Option<&'a Path>,
    dependency_paths: &'a [PathBuf],
    sources: Vec<renderpilot_domain::TrackedSource>,
    managed_files: crate::addons::tracking::ManagedFilesUpdate,
    next_components: Option<&'a [renderpilot_domain::GraphicsComponent]>,
    rollback_specs: &'a [ValidatedRollbackPlan],
}

fn persist_set_diff_result(input: PersistSetDiff<'_>) -> Result<(), UpdateFailure> {
    let PersistSetDiff {
        context,
        record,
        mutation_id,
        rollback,
        payload,
        new_addon_file,
        removed,
        host_path,
        dependency_paths,
        sources,
        managed_files,
        next_components,
        rollback_specs,
    } = input;

    let (created_files, backed_up_files) =
        match rebuild_record_paths(record, removed, rollback, host_path, dependency_paths) {
            Ok(paths) => paths,
            Err(error) => return Err(rollback.fail(error, true)),
        };

    let refreshed = match tracking::rebuild(
        record,
        crate::addons::tracking::RebuildParts {
            addon_file: new_addon_file.clone(),
            addon_version: crate::addons::tracking::AddonVersionUpdate::Set(
                tracking::resolved_addon_version(record, payload),
            ),
            managed_files,
            created_files,
            backed_up_files,
            tracked_sources: sources,
            label: "Luma set-diff update rebuild".to_owned(),
        },
    ) {
        Ok(refreshed) => refreshed,
        Err(error) => return Err(rollback.fail(error, true)),
    };

    let rolled_back_ids: Vec<_> = rollback_specs
        .iter()
        .map(|spec| spec.component_id().clone())
        .collect();
    if let Err(error) =
        context
            .storage()
            .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
                game_id: record.game_id(),
                component_set: next_components,
                baseline_inserts: &[],
                baseline_deletes: &rolled_back_ids,
                addon: renderpilot_storage_sqlite::InstalledAddonMutation::Upsert(&refreshed),
                mutation_id: Some(mutation_id),
            })
    {
        return Err(rollback.fail(error.into(), true));
    }

    // Best-effort after durable DB commit (same placement as uninstall journal).
    crate::catalog::cascade::record_cascade_rollback_journal(
        context.storage(),
        record.game_id(),
        rollback_specs,
    );
    Ok(())
}
