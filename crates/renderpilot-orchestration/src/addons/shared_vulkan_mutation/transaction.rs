//! SVAM-v1 forward transaction sequencing.

use std::path::{Path, PathBuf};

use renderpilot_application::SharedArtifactRepository;
use renderpilot_domain::{AddonKind, GameId, InstalledAddon};
use renderpilot_platform_windows::vulkan_layer::LayerRegistry;
use renderpilot_storage_sqlite::{
    BeginSharedVulkanMutation, InstalledAddonMutation, SharedArtifactMutation,
    SharedVulkanMutationCommit, SharedVulkanMutationReservation, SharedVulkanMutationScope,
};

use super::io;
use super::manifest::Scope;
use super::plan::Request as PlanRequest;
use super::{FileIntent, Manifest, MutationError, MutationPlan, RegistryIntent, TrustedRoots};

/// Scope and catalog effect for a durable mutation.
///
/// Construction keeps invalid scope/projection combinations unrepresentable:
/// shared-only work has no game owner or add-on mutation, while game-shared
/// work always carries a non-empty game projection.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ScopeSpec<'a> {
    SharedOnly,
    GameShared {
        game_id: &'a GameId,
        addon: GameAddonMutation<'a>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum GameAddonMutation<'a> {
    Upsert(&'a InstalledAddon),
    Delete(AddonKind),
}

/// Stable identity and ownership for one durable mutation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MutationIdentity<'a> {
    id: &'a str,
    scope: ScopeSpec<'a>,
    feature: &'a str,
}

/// Exact physical participants and the capabilities that authorize them.
pub(crate) struct PhysicalParticipants<'a> {
    roots: TrustedRoots,
    files: Vec<FileIntent>,
    registry: Vec<RegistryIntent>,
    registry_authority: Option<&'a dyn LayerRegistry>,
    created_dirs: Vec<PathBuf>,
}

/// Catalog projections committed in the same SQLite transaction as the
/// physical publication.  The physical plan remains authoritative; these
/// fields are only the durable projection of that plan.
pub(crate) struct CatalogProjection<'a> {
    shared_artifact: SharedArtifactMutation<'a>,
}

/// Inputs for one complete shared-layer transaction.  Construction is kept
/// behind typed aggregates so callers cannot silently mix scope, roots, and
/// catalog projections from different operations.
pub(crate) struct Request<'a> {
    context: &'a crate::Context,
    identity: MutationIdentity<'a>,
    physical: PhysicalParticipants<'a>,
    projection: CatalogProjection<'a>,
}

impl<'a> GameAddonMutation<'a> {
    fn storage(self) -> InstalledAddonMutation<'a> {
        match self {
            Self::Upsert(record) => InstalledAddonMutation::Upsert(record),
            Self::Delete(kind) => InstalledAddonMutation::Delete(kind),
        }
    }
}

impl<'a> ScopeSpec<'a> {
    pub(crate) fn shared_only() -> Self {
        Self::SharedOnly
    }

    pub(crate) fn game_upsert(game_id: &'a GameId, record: &'a InstalledAddon) -> Self {
        Self::GameShared {
            game_id,
            addon: GameAddonMutation::Upsert(record),
        }
    }

    pub(crate) fn game_delete(game_id: &'a GameId, kind: AddonKind) -> Self {
        Self::GameShared {
            game_id,
            addon: GameAddonMutation::Delete(kind),
        }
    }

    fn storage_parts(self) -> (Scope, Option<&'a GameId>, InstalledAddonMutation<'a>) {
        match self {
            Self::SharedOnly => (Scope::SharedOnly, None, InstalledAddonMutation::Keep),
            Self::GameShared { game_id, addon } => {
                (Scope::GameShared, Some(game_id), addon.storage())
            }
        }
    }
}

impl<'a> MutationIdentity<'a> {
    pub(crate) fn new(id: &'a str, scope: ScopeSpec<'a>, feature: &'a str) -> Self {
        Self { id, scope, feature }
    }
}

impl<'a> PhysicalParticipants<'a> {
    pub(crate) fn new(
        roots: TrustedRoots,
        participants: super::composer::ComposedParticipants,
        registry_authority: Option<&'a dyn LayerRegistry>,
    ) -> Self {
        Self {
            roots,
            files: participants.files,
            registry: participants.registry,
            registry_authority,
            created_dirs: participants.created_dirs,
        }
    }

    fn is_noop(&self) -> bool {
        self.files
            .iter()
            .all(|intent| intent.before == intent.after)
            && self
                .registry
                .iter()
                .all(|intent| intent.before == intent.after)
            && self.created_dirs.is_empty()
    }

    fn has_shared_delta(&self) -> Result<bool, MutationError> {
        for intent in &self.files {
            if intent.before != intent.after
                && self.roots.authorize(&intent.live_path)?.root_id() == "shared"
            {
                return Ok(true);
            }
        }
        for intent in &self.registry {
            if intent.before != intent.after
                && self.roots.authorize(&intent.manifest_path)?.root_id() == "shared"
            {
                return Ok(true);
            }
        }
        for path in &self.created_dirs {
            if self.roots.authorize(path)?.root_id() == "shared" {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl<'a> CatalogProjection<'a> {
    pub(crate) fn new(shared_artifact: SharedArtifactMutation<'a>) -> Self {
        Self { shared_artifact }
    }
}

impl<'a> Request<'a> {
    pub(crate) fn new(
        context: &'a crate::Context,
        identity: MutationIdentity<'a>,
        physical: PhysicalParticipants<'a>,
        projection: CatalogProjection<'a>,
    ) -> Self {
        Self {
            context,
            identity,
            physical,
            projection,
        }
    }
}

struct PreparedRequest<'a> {
    context: &'a crate::Context,
    id: &'a str,
    storage_scope: SharedVulkanMutationScope,
    game_id: Option<&'a GameId>,
    transaction_root: &'a Path,
    plan: &'a MutationPlan,
    registry: Option<&'a dyn LayerRegistry>,
    addon: InstalledAddonMutation<'a>,
    shared_artifact: SharedArtifactMutation<'a>,
}

/// Reserves, prepares, applies, verifies, and durably commits one singleton
/// shared Vulkan mutation. Any failure after `Prepared` deliberately leaves
/// the row and exact artifacts for [`super::recovery`] rather than guessing a
/// compensating write.
pub(crate) fn execute(request: Request<'_>) -> Result<(), crate::ServiceError> {
    let Request {
        context,
        identity,
        physical,
        projection,
    } = request;
    let (scope, game_id, addon) = identity.scope.storage_parts();
    if scope == Scope::SharedOnly && physical.is_noop() {
        apply_shared_only_projection(context, projection.shared_artifact)?;
        return Ok(());
    }
    if scope == Scope::GameShared && !physical.has_shared_delta()? {
        return Err(MutationError::conflict(
            "game-shared mutation has no shared physical participant",
        )
        .into());
    }
    let MutationIdentity {
        id,
        scope: _,
        feature,
    } = identity;
    let PhysicalParticipants {
        roots,
        files,
        registry,
        registry_authority,
        created_dirs,
    } = physical;
    let CatalogProjection { shared_artifact } = projection;
    let storage_scope = storage_scope(scope);
    let root_capabilities_json = roots.to_json()?;
    let transaction_namespace = super::ensure_transaction_namespace(context.file_mutation_root())?;
    let transaction_root = super::transaction_root(context.file_mutation_root(), id)?;
    debug_assert_eq!(
        transaction_root.parent(),
        Some(transaction_namespace.as_path())
    );
    match std::fs::symlink_metadata(&transaction_root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(crate::ServiceError::invalid_input(
                "shared Vulkan transaction root already exists",
            ));
        }
        Err(error) => return Err(crate::ServiceError::command_failed(error.to_string())),
    }
    let initial = Manifest::empty(scope, game_id.map(|id| id.as_str().to_owned()), feature)
        .to_json()
        .map_err(MutationError::manifest)?;
    match context
        .storage()
        .try_begin_shared_vulkan_mutation(&BeginSharedVulkanMutation {
            id: id.to_owned(),
            scope: storage_scope,
            game_id: game_id.cloned(),
            feature: feature.to_owned(),
            initial_manifest_json: initial,
            root_capabilities_json,
        })? {
        SharedVulkanMutationReservation::Reserved(_) => {}
        SharedVulkanMutationReservation::Occupied(row) => {
            return Err(crate::ServiceError::invalid_input(format!(
                "shared Vulkan mutation is already reserved by {}",
                row.id
            )));
        }
    }

    std::fs::create_dir(&transaction_root)
        .map_err(|error| crate::ServiceError::command_failed(error.to_string()))?;
    let plan = match MutationPlan::build(PlanRequest {
        transaction_root: transaction_root.clone(),
        mutation_id: id.to_owned(),
        roots,
        scope,
        game_id: game_id.map(|id| id.as_str().to_owned()),
        feature: feature.to_owned(),
        intents: files,
        registry,
        registry_authority,
        created_dirs,
    }) {
        Ok(plan) => plan,
        Err(error) => {
            if let Err(cleanup_error) =
                super::recovery::cleanup_preparing_artifacts(&transaction_root)
            {
                log::warn!(
                    "could not clean failed shared Vulkan preparation; recovery remains armed: {cleanup_error}"
                );
                return Err(error.into());
            }
            let _ = context
                .storage()
                .abandon_shared_vulkan_mutation_preparation(id);
            return Err(error.into());
        }
    };
    let manifest_json = match plan
        .manifest
        .validate_for_transaction(id)
        .and_then(|_| plan.manifest.to_json())
    {
        Ok(manifest_json) => manifest_json,
        Err(error) => {
            if let Err(cleanup_error) =
                super::recovery::cleanup_preparing_artifacts(&transaction_root)
            {
                log::warn!(
                    "could not clean failed shared Vulkan manifest; recovery remains armed: {cleanup_error}"
                );
                return Err(MutationError::manifest(error).into());
            }
            let _ = context
                .storage()
                .abandon_shared_vulkan_mutation_preparation(id);
            return Err(MutationError::manifest(error).into());
        }
    };
    io::sync_prepared_artifacts(&transaction_root);
    context.storage().finish_preparing_shared_vulkan_mutation(
        id,
        storage_scope,
        game_id,
        &manifest_json,
    )?;

    let prepared = PreparedRequest {
        context,
        id,
        storage_scope,
        game_id,
        transaction_root: &transaction_root,
        plan: &plan,
        registry: registry_authority,
        addon,
        shared_artifact,
    };
    if let Err(error) = execute_prepared(&prepared) {
        if let Err(recovery_error) =
            super::recovery::recover_pending_with_roots(context, &plan.roots, registry_authority)
        {
            log::warn!(
                "shared Vulkan transaction failed and whole-set recovery could not converge: {recovery_error}"
            );
            return Err(recovery_error);
        }
        return Err(error.into());
    }
    Ok(())
}

fn apply_shared_only_projection(
    context: &crate::Context,
    projection: SharedArtifactMutation<'_>,
) -> Result<(), crate::ServiceError> {
    match projection {
        SharedArtifactMutation::Keep => Ok(()),
        SharedArtifactMutation::Upsert(record) => context
            .storage()
            .upsert_shared_artifact(record)
            .map_err(Into::into),
        SharedArtifactMutation::Delete(kind) => context
            .storage()
            .delete_shared_artifact(kind)
            .map_err(Into::into),
    }
}

fn execute_prepared(request: &PreparedRequest<'_>) -> Result<(), MutationError> {
    let PreparedRequest {
        context,
        id,
        storage_scope,
        game_id,
        transaction_root,
        plan,
        registry,
        addon,
        shared_artifact,
    } = request;
    io::verify_all_before(transaction_root, &plan.manifest, &plan.roots, *registry)?;
    io::materialize_stages(plan)?;
    let registry_first = io::deactivates_registry(&plan.manifest);
    if registry_first {
        apply_registry(plan, *registry)?;
    }
    io::apply_files(
        transaction_root,
        &plan.manifest,
        &plan.payloads,
        &plan.roots,
    )?;
    if !registry_first {
        apply_registry(plan, *registry)?;
    }
    io::verify_all_after(transaction_root, &plan.manifest, &plan.roots, *registry)?;
    io::sync_published_directories(&plan.manifest, &plan.roots)?;

    context
        .storage()
        .commit_shared_vulkan_mutation(SharedVulkanMutationCommit {
            id,
            scope: *storage_scope,
            game_id: *game_id,
            addon: *addon,
            shared_artifact: *shared_artifact,
        })
        .map_err(|error| MutationError::Service(error.into()))?;

    if let Err(error) = io::cleanup_artifacts(
        transaction_root,
        &plan.manifest,
        &plan.roots,
        io::ParticipantState::After,
    ) {
        log::warn!(
            "shared Vulkan mutation committed; deferred transaction-artifact cleanup: {error}"
        );
        return Ok(());
    }
    if let Err(error) = context
        .storage()
        .cleanup_committed_shared_vulkan_mutation(id)
    {
        log::warn!("shared Vulkan mutation committed; deferred committed-fence cleanup: {error}");
    }
    Ok(())
}

fn apply_registry(
    plan: &MutationPlan,
    registry: Option<&dyn LayerRegistry>,
) -> Result<(), MutationError> {
    if plan.manifest.registry.is_empty() {
        return Ok(());
    }
    io::restore_registry(
        registry.ok_or_else(|| MutationError::conflict("registry authority missing"))?,
        &plan.manifest,
        &plan.roots,
        false,
    )
}

fn storage_scope(scope: Scope) -> SharedVulkanMutationScope {
    match scope {
        Scope::SharedOnly => SharedVulkanMutationScope::SharedOnly,
        Scope::GameShared => SharedVulkanMutationScope::GameShared,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_only_noop_projects_without_reserving_the_singleton() {
        let temp = tempfile::tempdir().expect("tempdir");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let live = temp.path().join("ReShade64.dll");
        std::fs::write(&live, b"unchanged").expect("seed shared file");
        let roots = TrustedRoots::shared_only(temp.path()).expect("roots");

        execute(Request::new(
            &context,
            MutationIdentity::new("shared-noop", ScopeSpec::shared_only(), "test"),
            PhysicalParticipants::new(
                roots,
                super::super::composer::ComposedParticipants {
                    files: vec![FileIntent {
                        live_path: live,
                        before: Some(b"unchanged".to_vec()),
                        after: Some(b"unchanged".to_vec()),
                    }],
                    registry: Vec::new(),
                    created_dirs: Vec::new(),
                },
                None,
            ),
            CatalogProjection::new(SharedArtifactMutation::Keep),
        ))
        .expect("no-op projection");

        assert!(
            context
                .storage()
                .pending_shared_vulkan_mutation()
                .expect("pending row")
                .is_none()
        );
    }

    #[test]
    fn game_shared_scope_always_carries_an_owner_and_projection() {
        let game = GameId::new("steam:123").expect("game id");
        let (scope, owner, addon) =
            ScopeSpec::game_delete(&game, AddonKind::RenoDx).storage_parts();
        assert_eq!(scope, Scope::GameShared);
        assert_eq!(owner, Some(&game));
        assert!(matches!(
            addon,
            InstalledAddonMutation::Delete(AddonKind::RenoDx)
        ));
    }
}
