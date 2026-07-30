//! Resolve uninstall roots, cascade effects, release plans, and mutation workset.
//!
//! Owns only what execute needs after planning: no leftover `mutation_paths`
//! (those are consumed into the durable workset).

use std::path::Path;

use renderpilot_domain::{AddonKind, ComponentId, GameId, InstalledAddon, LibraryComponent};

use crate::addons::luma::dlss::PlannedDlss;
use crate::addons::luma::errors;
use crate::addons::mutation_targets::DurableWorkset;
use crate::addons::records;
use crate::catalog::cascade::ValidatedRollbackPlan;
use crate::{Context, ServiceError};

/// Filesystem reverse + DB commit inputs for the durable uninstall body.
pub(super) struct UninstallApply {
    pub(super) record: InstalledAddon,
    pub(super) rollback_specs: Vec<ValidatedRollbackPlan>,
    pub(super) next_components: Vec<LibraryComponent>,
    pub(super) release_plans: Vec<PlannedDlss>,
    pub(super) rolled_back_ids: Vec<ComponentId>,
}

/// Planned uninstall inputs for the durable apply/commit path.
pub(super) struct UninstallPlan {
    pub(super) apply: UninstallApply,
    pub(super) workset: DurableWorkset,
}

pub(super) fn plan_uninstall(
    context: &Context,
    game_id: &GameId,
) -> Result<UninstallPlan, ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::Luma)?
        .ok_or_else(errors::not_installed)?;

    // Installed add-on records intentionally survive catalog pruning, so an
    // uninstall must remain possible even if the game row was removed. In that
    // case the recorded add-on directory is the only safe mutation root.
    let game_root = crate::catalog::game_root_for_mutation(
        context.storage(),
        game_id,
        Path::new(record.addon_file().as_str())
            .parent()
            .map(Path::to_path_buf),
    )
    .map_err(|error| {
        if matches!(
            error.kind(),
            renderpilot_application::AppErrorKind::GameNotFound
        ) {
            errors::failed("Luma install record has no filesystem root".to_owned())
        } else {
            error.into()
        }
    })?;

    let owned_paths = records::owned_managed_paths(&record);
    let cascade =
        crate::catalog::cascade::cascade_for_owned_paths(context.storage(), game_id, &owned_paths)?;
    let release_plans = record
        .managed_files()
        .iter()
        .map(|managed| {
            let path = Path::new(managed.path().as_str());
            let consumed = cascade
                .rollback_specs
                .iter()
                .any(|spec| spec.contains_path(path));
            crate::addons::luma::dlss::plan_release_binding(context, game_id, managed, consumed)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let crate::catalog::cascade::CascadeResult {
        rollback_specs,
        next_components,
        mutation_paths,
    } = cascade;

    let targets = crate::addons::luma::mutation_targets::uninstall_targets(
        game_root,
        &record,
        mutation_paths,
    );
    let workset = targets.resolve_workset()?;

    let rolled_back_ids: Vec<_> = rollback_specs
        .iter()
        .map(|spec| spec.component_id().clone())
        .collect();

    Ok(UninstallPlan {
        apply: UninstallApply {
            record,
            rollback_specs,
            next_components,
            release_plans,
            rolled_back_ids,
        },
        workset,
    })
}
