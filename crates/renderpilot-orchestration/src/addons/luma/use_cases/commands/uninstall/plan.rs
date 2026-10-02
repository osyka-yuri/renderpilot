//! Resolve uninstall roots, cascade effects, release plans, and mutation plans.
//!
//! Inactive records produce the existing durable workset; active proxy topologies
//! produce the complete peer transition consumed by the peer executor.

use std::path::{Path, PathBuf};

use renderpilot_application::{GameRepository, ProxyTopologyRepository};
use renderpilot_domain::{
    AddonKind, ComponentId, GameId, GameProxyTopology, InstalledAddon, LibraryComponent,
    ManagedFileMode, NormalizedPathRelation, PlannedGameProxyTopology, normalized_path_key,
    normalized_path_relation,
};

use crate::addons::luma::dlss::PlannedDlss;
use crate::addons::luma::errors;
use crate::addons::luma::peer::{
    LumaPeerRootAuthority, PlannedManagedDlssRelease, compose_active_uninstall,
};
use crate::addons::mutation_targets::{DurableWorkset, MutationTargets};
use crate::addons::records;
use crate::addons::shared_vulkan_mutation::FileIntent;
use crate::catalog::cascade::{CascadeResult, ValidatedRollbackPlan};
use crate::peer_mutation_executor::ExactEndpointProgram;
use crate::{Context, ServiceError};

/// Filesystem reverse + DB commit inputs for the durable uninstall body.
pub(super) struct UninstallApply {
    pub(super) record: InstalledAddon,
    pub(super) rollback_specs: Vec<ValidatedRollbackPlan>,
    pub(super) next_components: Vec<LibraryComponent>,
    pub(super) current_components: Vec<LibraryComponent>,
    pub(super) has_catalog_claim: bool,
    pub(super) release_plans: Vec<PlannedDlss>,
    pub(super) rolled_back_ids: Vec<ComponentId>,
}

/// Planned uninstall inputs for the durable apply/commit path.
pub(super) enum UninstallPlan {
    Inactive(Box<InactiveUninstallPlan>),
    Active(Box<ActiveUninstallApply>),
}

/// Owned inactive-topology inputs retained through durable application and its
/// catalog journal.
pub(super) struct InactiveUninstallPlan {
    pub(super) apply: UninstallApply,
    pub(super) workset: DurableWorkset,
}

/// Read-only preparation for an explicit replacement of an inactive external
/// owner. It carries the existing uninstall plan without its Engine/catalog
/// commit effects so the caller can include these exact paths in its outer
/// durable replacement transaction.
pub(crate) struct PreparedExternalOwnerRelease {
    apply: UninstallApply,
    targets: MutationTargets,
}

impl PreparedExternalOwnerRelease {
    pub(crate) fn targets(&self) -> &MutationTargets {
        &self.targets
    }

    pub(crate) fn next_components(&self) -> &[LibraryComponent] {
        &self.apply.next_components
    }

    pub(crate) fn baseline_mutations(
        &self,
    ) -> Vec<renderpilot_storage_sqlite::ComponentBaselineMutation<'_>> {
        self.apply
            .rolled_back_ids
            .iter()
            .map(
                |component_id| renderpilot_storage_sqlite::ComponentBaselineMutation::Delete {
                    component_id,
                },
            )
            .collect()
    }

    /// Confirms this prepared release can participate in an SVAM file-only
    /// transaction without changing the existing game catalog projection.
    pub(crate) fn ensure_no_catalog_effects(&self) -> Result<(), ServiceError> {
        if self.apply.has_catalog_claim
            || !self.apply.rollback_specs.is_empty()
            || !self.apply.rolled_back_ids.is_empty()
            || self.apply.current_components != self.apply.next_components
        {
            return Err(ServiceError::invalid_input(
                "Luma external release has catalog effects that cannot join a file-only Shared Vulkan transaction",
            ));
        }
        Ok(())
    }

    /// Returns exact file-only before/after images for the existing SVAM
    /// participant composer. The receipt and coordinated-file plans are read
    /// again here so unsupported or drifted operations fail before any write.
    pub(crate) fn file_intents_for_shared_vulkan(&self) -> Result<Vec<FileIntent>, ServiceError> {
        self.ensure_no_catalog_effects()?;
        receipt_file_intents(&self.apply)
    }

    /// Applies only receipt-bounded filesystem reversals. Engine journal
    /// release, catalog mutation, and cascade journaling belong to the outer
    /// replacement commit.
    pub(crate) fn apply_filesystem_only(&self) -> Result<(), ServiceError> {
        super::execute::execute_external_owner_release_files(&self.apply)
    }

    /// Records cascade completion only after the outer owner replacement has
    /// committed successfully.
    pub(crate) fn journal_after_commit(&self, context: &Context, game_id: &GameId) {
        super::execute::journal_cascade_after_commit(context, game_id, &self.apply);
    }
}

/// Owned active-topology inputs kept alive through peer preparation, apply,
/// commit, and the post-commit catalog journal.
pub(super) struct ActiveUninstallApply {
    pub(super) record: InstalledAddon,
    pub(super) topology: GameProxyTopology,
    pub(super) authority: LumaPeerRootAuthority,
    pub(super) cascade: CascadeResult,
    pub(super) program: ExactEndpointProgram,
    pub(super) payloads: Vec<Option<Vec<u8>>>,
    pub(super) planned_topology: PlannedGameProxyTopology,
}

pub(super) fn plan_uninstall(
    context: &Context,
    game_id: &GameId,
) -> Result<UninstallPlan, ServiceError> {
    let record = records::record_of_kind(context, game_id, AddonKind::Luma)?
        .ok_or_else(errors::not_installed)?;
    let topology = context.storage().get_proxy_topology(game_id)?;
    match topology {
        Some(topology) => plan_active_uninstall(context, game_id, record, topology),
        None => plan_inactive_uninstall(context, game_id, record),
    }
}

fn plan_active_uninstall(
    context: &Context,
    game_id: &GameId,
    record: InstalledAddon,
    topology: GameProxyTopology,
) -> Result<UninstallPlan, ServiceError> {
    let game = context.storage().require_game(game_id)?;
    let downstream = topology.downstream.as_ref().ok_or_else(|| {
        ServiceError::invalid_input("active Luma uninstall topology has no downstream host")
    })?;
    let authority = resolve_active_runtime_authority(
        Path::new(game.install_path().as_str()),
        &topology,
        Path::new(downstream.path.as_str()),
    )?;
    authority.validate(Some(&record), None, None, topology.participant_paths())?;

    let host_key = normalized_path_key(downstream.path.as_str());
    let remaining_owned = record
        .managed_files()
        .iter()
        .filter(|managed| managed.mode() == ManagedFileMode::Owned)
        .filter(|managed| normalized_path_key(managed.path().as_str()) != host_key)
        .map(|managed| Path::new(managed.path().as_str()).to_path_buf())
        .collect::<Vec<_>>();
    let cascade = crate::catalog::cascade::cascade_for_managed_paths(
        context.storage(),
        game_id,
        &remaining_owned,
    )?;
    let releases = record
        .managed_files()
        .iter()
        .filter(|managed| normalized_path_key(managed.path().as_str()) != host_key)
        .map(|managed| {
            let consumed = cascade
                .rollback_specs
                .iter()
                .any(|spec| spec.contains_path(Path::new(managed.path().as_str())));
            crate::addons::luma::dlss::plan_release_binding(context, game_id, managed, consumed)
                .map(|plan| PlannedManagedDlssRelease::new(managed, plan))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let composition =
        compose_active_uninstall(&record, &topology, &authority, &cascade, &releases)?;
    let (program, payloads, planned_topology) = composition.into_parts();

    Ok(UninstallPlan::Active(Box::new(ActiveUninstallApply {
        record,
        topology,
        authority,
        cascade,
        program,
        payloads,
        planned_topology,
    })))
}

pub(super) fn resolve_active_runtime_authority(
    catalog_game_root: &Path,
    topology: &GameProxyTopology,
    downstream_path: &Path,
) -> Result<LumaPeerRootAuthority, ServiceError> {
    let runtime_root = Path::new(topology.root_slot.as_str())
        .parent()
        .ok_or_else(|| {
            ServiceError::invalid_input("active Luma uninstall topology root slot has no parent")
        })?;
    let runtime_root = crate::paths::canonical_candidate(runtime_root).map_err(|error| {
        ServiceError::invalid_input(format!(
            "active Luma uninstall runtime root is invalid: {error}"
        ))
    })?;
    let catalog_game_root =
        crate::paths::canonical_candidate(catalog_game_root).map_err(|error| {
            ServiceError::invalid_input(format!(
                "active Luma uninstall game root is invalid: {error}"
            ))
        })?;
    if !matches!(
        normalized_path_relation(
            &catalog_game_root.to_string_lossy(),
            &runtime_root.to_string_lossy(),
        ),
        NormalizedPathRelation::Equal | NormalizedPathRelation::LeftAncestor
    ) {
        return Err(ServiceError::invalid_input(
            "active Luma uninstall topology runtime root is outside the catalog game root",
        ));
    }
    LumaPeerRootAuthority::resolve(&runtime_root, downstream_path)
}

fn plan_inactive_uninstall(
    context: &Context,
    game_id: &GameId,
    record: InstalledAddon,
) -> Result<UninstallPlan, ServiceError> {
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

    let (apply, targets) =
        plan_inactive_uninstall_apply(context, game_id, record, Some(game_root))?;
    let workset = targets.resolve_workset()?;

    Ok(UninstallPlan::Inactive(Box::new(InactiveUninstallPlan {
        apply,
        workset,
    })))
}

/// Prepares the same inactive-owner reversal as ordinary uninstall, but omits
/// the current game root from its mutation scope. The caller validates that
/// every resulting receipt path and operation root is disjoint from that root.
pub(super) fn plan_external_owner_release(
    context: &Context,
    game_id: &GameId,
    record: InstalledAddon,
    current_game_root: &Path,
) -> Result<PreparedExternalOwnerRelease, ServiceError> {
    let (apply, targets) = plan_inactive_uninstall_apply(context, game_id, record, None)?;
    validate_external_targets(current_game_root, &targets)?;
    Ok(PreparedExternalOwnerRelease { apply, targets })
}

fn validate_external_targets(
    current_game_root: &Path,
    targets: &MutationTargets,
) -> Result<(), ServiceError> {
    for path in targets.paths.iter().chain(&targets.roots) {
        if !path.is_absolute() {
            return Err(ServiceError::invalid_input(format!(
                "Luma external owner has a non-absolute receipt operation path: {}",
                path.display()
            )));
        }
        let candidate = crate::paths::canonical_candidate(path).map_err(|error| {
            ServiceError::invalid_input(format!(
                "Luma external owner receipt path is invalid: {}: {error}",
                path.display()
            ))
        })?;
        if !matches!(
            normalized_path_relation(
                &current_game_root.to_string_lossy(),
                &candidate.to_string_lossy(),
            ),
            NormalizedPathRelation::Disjoint
        ) {
            return Err(ServiceError::invalid_input(format!(
                "Luma external owner receipt operation overlaps the current game root: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn plan_inactive_uninstall_apply(
    context: &Context,
    game_id: &GameId,
    record: InstalledAddon,
    game_root: Option<PathBuf>,
) -> Result<(UninstallApply, MutationTargets), ServiceError> {
    use renderpilot_application::ComponentRepository;

    let current_components = context.storage().list_components_for_game(game_id)?;
    let owned_paths = records::owned_managed_paths(&record);
    let cascade = crate::catalog::cascade::cascade_for_managed_paths(
        context.storage(),
        game_id,
        &owned_paths,
    )?;
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

    let has_catalog_claim = cascade.catalog_claim().is_some();
    let crate::catalog::cascade::CascadeResult {
        rollback_specs,
        next_components,
        mutation_paths,
        ..
    } = cascade;

    let targets = match game_root {
        Some(game_root) => crate::addons::luma::mutation_targets::uninstall_targets(
            game_root,
            &record,
            mutation_paths,
        ),
        None => MutationTargets::for_record(&record, [], mutation_paths),
    };

    let rolled_back_ids: Vec<_> = rollback_specs
        .iter()
        .map(|spec| spec.component_id().clone())
        .collect();

    Ok((
        UninstallApply {
            record,
            rollback_specs,
            next_components,
            current_components,
            has_catalog_claim,
            release_plans,
            rolled_back_ids,
        },
        targets,
    ))
}

fn receipt_file_intents(apply: &UninstallApply) -> Result<Vec<FileIntent>, ServiceError> {
    use std::collections::HashSet;

    let created = apply
        .record
        .created_files()
        .iter()
        .map(|path| PathBuf::from(path.as_str()))
        .collect::<Vec<_>>();
    let backed_up = apply
        .record
        .backed_up_files()
        .iter()
        .map(|path| PathBuf::from(path.as_str()))
        .collect::<Vec<_>>();
    let backed_keys = backed_up
        .iter()
        .map(PathBuf::as_path)
        .collect::<HashSet<_>>();

    let mut intents = Vec::new();
    for path in &created {
        if backed_keys.contains(path.as_path()) {
            continue;
        }
        if let Some(before) = read_regular_file_optional(path)? {
            intents.push(FileIntent {
                live_path: path.clone(),
                before: Some(before),
                after: None,
            });
        }
    }
    for path in backed_up {
        let backup = crate::fs::backup_path(&path).map_err(|error| {
            ServiceError::invalid_input(format!("Luma external backup receipt is invalid: {error}"))
        })?;
        let Some(restored) = read_regular_file_optional(&backup)? else {
            // The ordinary engine treats a missing backup as a no-op.
            continue;
        };
        let before = read_regular_file_optional(&path)?;
        intents.push(FileIntent {
            live_path: path,
            before,
            after: Some(restored.clone()),
        });
        intents.push(FileIntent {
            live_path: backup,
            before: Some(restored),
            after: None,
        });
    }

    for planned in &apply.release_plans {
        append_managed_release_intents(planned, &mut intents)?;
    }

    let mut seen = HashSet::new();
    for intent in &intents {
        if !seen.insert(crate::paths::normalized_key(&intent.live_path)) {
            return Err(ServiceError::invalid_input(format!(
                "Luma external release has overlapping file participants at {}",
                intent.live_path.display()
            )));
        }
    }
    Ok(intents)
}

fn append_managed_release_intents(
    planned: &PlannedDlss,
    intents: &mut Vec<FileIntent>,
) -> Result<(), ServiceError> {
    use crate::coordinated_files::CoordinatedFilePlan;

    match &planned.action {
        CoordinatedFilePlan::Keep => Ok(()),
        CoordinatedFilePlan::Reuse { path, sha256 } => {
            require_file_hash(path, std::slice::from_ref(sha256))
        }
        CoordinatedFilePlan::RestoreAndRelease {
            path,
            baseline_sha256,
            expected_live,
        } => {
            let before = read_regular_file_optional(path)?.ok_or_else(|| {
                ServiceError::invalid_input(format!(
                    "managed Luma release target disappeared: {}",
                    path.display()
                ))
            })?;
            require_bytes_hash(path, &before, expected_live)?;
            let backup = crate::fs::backup_path(path).map_err(|error| {
                ServiceError::invalid_input(format!(
                    "managed Luma release backup is invalid: {error}"
                ))
            })?;
            let restored = read_regular_file_optional(&backup)?.ok_or_else(|| {
                ServiceError::invalid_input(format!(
                    "managed Luma release baseline disappeared: {}",
                    backup.display()
                ))
            })?;
            require_bytes_hash(&backup, &restored, std::slice::from_ref(baseline_sha256))?;
            intents.push(FileIntent {
                live_path: path.clone(),
                before: Some(before),
                after: Some(restored.clone()),
            });
            intents.push(FileIntent {
                live_path: backup,
                before: Some(restored),
                after: None,
            });
            Ok(())
        }
        CoordinatedFilePlan::RemoveAndRelease {
            path,
            expected_live,
        } => {
            let before = read_regular_file_optional(path)?.ok_or_else(|| {
                ServiceError::invalid_input(format!(
                    "managed Luma release target disappeared: {}",
                    path.display()
                ))
            })?;
            require_bytes_hash(path, &before, expected_live)?;
            let backup = crate::fs::backup_path(path).map_err(|error| {
                ServiceError::invalid_input(format!(
                    "managed Luma release backup is invalid: {error}"
                ))
            })?;
            if read_regular_file_optional(&backup)?.is_some() {
                return Err(ServiceError::invalid_input(format!(
                    "managed Luma release unexpectedly has a baseline sidecar: {}",
                    backup.display()
                )));
            }
            intents.push(FileIntent {
                live_path: path.clone(),
                before: Some(before),
                after: None,
            });
            Ok(())
        }
        other => Err(ServiceError::invalid_input(format!(
            "Luma external release contains an unsupported coordinated-file operation: {other:?}"
        ))),
    }
}

fn read_regular_file_optional(path: &Path) -> Result<Option<Vec<u8>>, ServiceError> {
    use std::io::ErrorKind;

    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ServiceError::command_failed(format!(
                "cannot inspect Luma external release participant {}: {error}",
                path.display()
            )));
        }
    };
    if !metadata.file_type().is_file() {
        return Err(ServiceError::invalid_input(format!(
            "Luma external release participant is not a regular file: {}",
            path.display()
        )));
    }
    std::fs::read(path).map(Some).map_err(|error| {
        ServiceError::command_failed(format!(
            "cannot read Luma external release participant {}: {error}",
            path.display()
        ))
    })
}

fn require_file_hash(
    path: &Path,
    expected: &[renderpilot_domain::Sha256Hash],
) -> Result<(), ServiceError> {
    let bytes = read_regular_file_optional(path)?.ok_or_else(|| {
        ServiceError::invalid_input(format!(
            "managed Luma release target disappeared: {}",
            path.display()
        ))
    })?;
    require_bytes_hash(path, &bytes, expected)
}

fn require_bytes_hash(
    path: &Path,
    bytes: &[u8],
    expected: &[renderpilot_domain::Sha256Hash],
) -> Result<(), ServiceError> {
    let actual = renderpilot_detection::sha256_bytes(bytes)?;
    if expected.contains(&actual) {
        Ok(())
    } else {
        Err(ServiceError::invalid_input(format!(
            "Luma managed release participant changed after planning: {}",
            path.display()
        )))
    }
}
