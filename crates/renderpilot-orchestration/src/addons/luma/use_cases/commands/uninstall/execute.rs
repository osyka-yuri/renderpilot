//! Filesystem reverse + DB commit body for Luma uninstall.

use std::path::Path;

use renderpilot_domain::{AddonKind, GameId, PeerCatalogRollbackClaim};
use renderpilot_storage_sqlite::ComponentBaselineMutation;

use crate::addons::engine::InstallChanges;
use crate::addons::luma::install::uninstall_engine_files;
use crate::addons::peer_lifecycle::package::{PeerMutationPackage, PeerMutationRequest};
use crate::addons::records;
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

use super::plan::{ActiveUninstallApply, UninstallApply};

pub(super) fn execute_active_uninstall(
    context: &Context,
    guard: &GameMutationGuard,
    apply: ActiveUninstallApply,
) -> Result<(), ServiceError> {
    let ActiveUninstallApply {
        record,
        topology,
        authority,
        cascade,
        program,
        payloads,
        planned_topology,
    } = apply;
    let component_set = cascade
        .catalog_claim()
        .map(PeerCatalogRollbackClaim::after_components);
    let baseline_mutations = cascade
        .catalog_claim()
        .map(|claim| {
            claim
                .deleted_baselines()
                .iter()
                .map(|entry| ComponentBaselineMutation::Delete {
                    component_id: entry.component_id(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let original_record = record;
    crate::addons::engine_config::service::release_record(
        context.storage(),
        guard.game_id(),
        &original_record,
        &format!("luma-release-{}", ulid::Ulid::generate()),
    )
    .map_err(|error| {
        ServiceError::command_failed(format!(
            "Luma Engine.ini release blocked uninstall: {error}"
        ))
    })?;
    let record =
        records::record_of_kind(context, guard.game_id(), AddonKind::Luma)?.ok_or_else(|| {
            ServiceError::command_failed("Luma record disappeared during Engine.ini release")
        })?;
    let record = records::verify_engine_config_release(&original_record, record, "Luma")?;
    let package = PeerMutationPackage::plan_active(PeerMutationRequest {
        peer_kind: AddonKind::Luma,
        before_peer: Some(&record),
        after_peer: None,
        before_topology: &topology,
        planned_after_topology: &planned_topology,
        program,
        payloads,
        game_root: authority.canonical_game_root().to_path_buf(),
        payload_root: authority.external_capability_root().map(Path::to_path_buf),
        component_set,
        baseline_mutations: &baseline_mutations,
        catalog_claim: cascade.catalog_claim(),
    })?;
    let prepared = context
        .peer_mutation_executor()
        .prepare_ordinary_file_peer(
            context,
            guard,
            crate::addons::mutation_features::LUMA_UNINSTALL,
            Some(guard.game_id().as_str()),
            package,
        )?;
    let mut changes = InstallChanges::default();
    let applied = prepared.apply(&mut changes)?;
    changes.sync_touched_dirs();
    applied.commit()?;
    crate::catalog::cascade::record_cascade_rollback_journal(
        context.storage(),
        guard.game_id(),
        &cascade.rollback_specs,
    );
    Ok(())
}

pub(super) fn execute_uninstall_body(
    context: &Context,
    game_id: &GameId,
    apply: &UninstallApply,
    mutation_id: Option<&str>,
) -> Result<(), ServiceError> {
    // Release shared Engine.ini ownership before any payload/cascade deletion
    // so an Engine.ini access failure leaves the add-on files and row intact.
    crate::addons::engine_config::service::release_record(
        context.storage(),
        game_id,
        &apply.record,
        &format!("luma-release-{}", ulid::Ulid::generate()),
    )
    .map_err(|error| {
        ServiceError::command_failed(format!(
            "Luma Engine.ini release blocked uninstall: {error}"
        ))
    })?;
    // Metadata-only: roots are gone -- FS reverse is best-effort so a
    // missing tree cannot block clearing the install row.
    if mutation_id.is_none() {
        if let Err(error) =
            crate::catalog::cascade::apply_cascade_rollback_fs(&apply.rollback_specs)
        {
            tracing::warn!("luma metadata-only uninstall cascade FS failed: {error}");
        }
        for release in &apply.release_plans {
            if let Err(error) = release.execute() {
                tracing::warn!("luma metadata-only uninstall managed release failed: {error}");
            }
        }
        if let Err(error) = uninstall_engine_files(&apply.record) {
            tracing::warn!("luma metadata-only uninstall engine cleanup failed: {error}");
        }
    } else {
        crate::catalog::cascade::apply_cascade_rollback_fs(&apply.rollback_specs)?;
        for release in &apply.release_plans {
            release.execute()?;
        }
        uninstall_engine_files(&apply.record)?;
    }
    let baseline_mutations = apply
        .rolled_back_ids
        .iter()
        .map(
            |component_id| renderpilot_storage_sqlite::ComponentBaselineMutation::Delete {
                component_id,
            },
        )
        .collect::<Vec<_>>();
    context
        .storage()
        .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
            game_id,
            component_set: Some(&apply.next_components),
            baseline_mutations: &baseline_mutations,
            addon: renderpilot_storage_sqlite::InstalledAddonMutation::Delete(AddonKind::Luma),
            mutation_id,
        })?;
    Ok(())
}

pub(super) fn journal_cascade_after_commit(
    context: &Context,
    game_id: &GameId,
    apply: &UninstallApply,
) {
    crate::catalog::cascade::record_cascade_rollback_journal(
        context.storage(),
        game_id,
        &apply.rollback_specs,
    );
}
