//! Prepare / commit / rollback for a durable game-file transaction.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use renderpilot_domain::GameId;
use renderpilot_storage_sqlite::{BeginFileMutationPreparation, SqliteStorage};
use sha2::{Digest, Sha256};

use super::manifest::{
    FileBeforeSnapshot, FileMutationManifest, MANIFEST_FORMAT_VERSION, cleanup_manifest,
    restore_manifest, serialize_manifest, serialize_manifest_with_peer_program,
};
use super::scope::{MutationScope, require_path_in_scope};
use crate::addons::engine::PeerMutationPlan;
use crate::addons::engine::apply::PeerMutationPreflight;
use crate::addons::peer_lifecycle::PeerMutationPackage;
use crate::game_mutation_lock::GameMutationGuard;
use crate::peer_mutation_executor::{
    EndpointObservation, PeerAncestorPlan, PeerExecutionClass, build_peer_program,
    render_peer_roots,
};
use crate::{Context, ServiceError};

/// Durable before-state for one feature transaction over authorized file roots
/// (game directory and optional external add-on roots).
pub(crate) struct DurableFileTransaction {
    id: String,
    game_id: GameId,
    transaction_root: PathBuf,
    manifest: FileMutationManifest,
    manifest_json: String,
    /// Immutable native O1 observations retained from preparation through O3.
    /// The durable manifest still owns crash recovery; this in-process proof
    /// prevents a same-digest identity replacement from crossing the fence.
    before_identities: Vec<BeforeIdentity>,
    /// Immutable native peer O1 retained from preflight through O3.
    peer_preflight: Option<PeerMutationPreflight>,
    /// The exact ancestor plan retained for the later peer apply boundary.
    peer_ancestor_plan: Option<PeerAncestorPlan>,
}

impl DurableFileTransaction {
    /// Reserves a row, snapshots every target, then publishes `Prepared` before
    /// the caller may perform its first game-file mutation.
    pub(crate) fn prepare(
        context: &Context,
        guard: &GameMutationGuard,
        scope: &MutationScope,
        feature: &str,
        subject_id: Option<&str>,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Self, ServiceError> {
        Self::prepare_with_topology_policy(
            context,
            guard,
            DurablePreparationInput {
                scope,
                feature,
                subject_id,
                paths: paths.into_iter().collect(),
                topology: DurablePreparationTopology::Ordinary,
            },
        )
    }

    /// Prepares a durable peer route after its exact endpoint program has been
    /// preflighted. The peer adapter is the only caller allowed to use this
    /// policy; ordinary durable mutations remain topology-fenced above.
    pub(crate) fn prepare_peer_mutation(
        context: &Context,
        guard: &GameMutationGuard,
        feature: &str,
        subject_id: Option<&str>,
        package: &PeerMutationPackage<'_>,
    ) -> Result<Self, ServiceError> {
        let paths = package
            .preflight()
            .paths()
            .map(|path| PathBuf::from(path.as_str()))
            .collect::<Vec<_>>();
        Self::prepare_with_topology_policy(
            context,
            guard,
            DurablePreparationInput {
                scope: package.scope(),
                feature,
                subject_id,
                paths,
                topology: DurablePreparationTopology::Peer {
                    plan: package.plan(),
                    preflight: package.preflight().clone(),
                    execution_class: PeerExecutionClass::Ordinary,
                    ancestor_plan: package.ancestor_plan().clone(),
                },
            },
        )
    }

    fn prepare_with_topology_policy(
        context: &Context,
        guard: &GameMutationGuard,
        input: DurablePreparationInput<'_>,
    ) -> Result<Self, ServiceError> {
        let DurablePreparationInput {
            scope,
            feature,
            subject_id,
            paths,
            topology,
        } = input;
        let (
            topology_policy,
            peer_plan,
            peer_preflight,
            peer_execution_class,
            supplied_peer_ancestor_plan,
        ) = match topology {
            DurablePreparationTopology::Ordinary => {
                (TopologyPolicy::Ordinary, None, None, None, None)
            }
            DurablePreparationTopology::Peer {
                plan,
                preflight,
                execution_class,
                ancestor_plan,
            } => (
                TopologyPolicy::PeerProgramPrepared,
                Some(plan),
                Some(preflight),
                Some(execution_class),
                Some(ancestor_plan),
            ),
        };
        super::recover_pending(context, guard)?;
        validate_required_text("feature", feature)?;
        if matches!(topology_policy, TopologyPolicy::Ordinary) {
            super::ensure_feature_allowed_with_proxy_topology(context, guard.game_id(), feature)?;
        }

        if let Some(preflight) = peer_preflight.as_ref() {
            let path_refs = paths
                .iter()
                .map(|path| {
                    renderpilot_domain::PathRef::new(path.to_string_lossy().into_owned())
                        .map_err(|error| crate::failed(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if !preflight.matches_paths(path_refs) {
                return Err(crate::failed(
                    "peer durable preparation paths differ from immutable preflight",
                ));
            }
            if let Some(peer_plan) = peer_plan
                && !preflight.matches_plan(peer_plan)
            {
                return Err(crate::failed(
                    "peer durable preparation plan differs from immutable preflight",
                ));
            }
            for path in &paths {
                require_path_in_scope(path, scope)?;
            }
        }

        let peer_ancestor_plan = match (peer_preflight.as_ref(), supplied_peer_ancestor_plan) {
            (Some(preflight), Some(ancestor_plan)) => {
                validate_supplied_peer_ancestor_plan(preflight, &ancestor_plan)?;
                Some(ancestor_plan)
            }
            (None, None) => None,
            _ => {
                return Err(crate::failed(
                    "peer ancestor plan must accompany immutable peer preflight",
                ));
            }
        };

        let id = ulid::Ulid::generate().to_string();
        let transaction_dir = context.file_mutation_root().join(&id);
        let roots = manifest_roots(scope, peer_preflight.is_some())?;
        let initial_manifest = FileMutationManifest {
            format_version: MANIFEST_FORMAT_VERSION,
            roots,
            transaction_dir: transaction_dir.to_string_lossy().into_owned(),
            snapshots: Vec::new(),
            peer_ancestors: Vec::new(),
        };
        let initial_json = serialize_manifest(&initial_manifest)?;
        context
            .storage()
            .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                id: id.clone(),
                game_id: guard.game_id().clone(),
                feature: feature.to_owned(),
                subject_id: subject_id.map(str::to_owned),
                initial_manifest_json: initial_json,
            })?;

        // Scoped so preparation errors run cleanup before returning.
        let prepared = (|| {
            fs::create_dir_all(&transaction_dir).map_err(|error| {
                crate::failed(format!(
                    "failed to create file transaction directory {}: {error}",
                    transaction_dir.display()
                ))
            })?;
            let (manifest, before_identities) = build_manifest(
                scope,
                &transaction_dir,
                paths.iter().cloned(),
                peer_preflight.as_ref(),
                peer_ancestor_plan.as_ref(),
            )?;
            let peer_program = match (peer_preflight.as_ref(), peer_plan, peer_execution_class) {
                (Some(preflight), Some(plan), Some(execution_class)) => {
                    let planned_lengths = plan
                        .payloads()
                        .iter()
                        .map(|payload| payload.as_ref().map(|bytes| bytes.len() as u64))
                        .collect::<Vec<_>>();
                    Some(
                        build_peer_program(
                            &id,
                            execution_class,
                            preflight.program(),
                            preflight.before(),
                            &planned_lengths,
                            peer_ancestor_plan.as_ref().ok_or_else(|| {
                                crate::failed("peer ancestor plan missing during preparation")
                            })?,
                            scope.roots(),
                        )
                        .map_err(|error| crate::failed(error.to_string()))?,
                    )
                }
                (None, None, None) => None,
                _ => {
                    return Err(crate::failed(
                        "peer durable preparation inputs are incomplete",
                    ));
                }
            };
            let manifest_json =
                serialize_manifest_with_peer_program(&manifest, peer_program.as_ref())?;
            if peer_program.is_none() {
                context
                    .storage()
                    .finish_preparing_file_mutation(&id, &manifest_json)?;
            }
            Ok(Self {
                id: id.clone(),
                game_id: guard.game_id().clone(),
                transaction_root: context.file_mutation_root().to_path_buf(),
                manifest,
                manifest_json,
                before_identities,
                peer_preflight,
                peer_ancestor_plan,
            })
        })();

        if let Err(error) = prepared {
            if let Err(cleanup_error) = cleanup_preparing(context.storage(), &id, &transaction_dir)
            {
                return Err(crate::failed(format!(
                    "{error}; abandoning the preparing file transaction also failed: {cleanup_error}"
                )));
            }
            return Err(error);
        }
        prepared
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn manifest_json(&self) -> &str {
        &self.manifest_json
    }

    /// Rechecks and creates ordinary peer ancestors after the storage permit
    /// has been sealed, immediately before endpoint publication.  The returned
    /// authorities bind nested endpoint creation to those exact directories.
    pub(crate) fn apply_peer_ancestors(
        &self,
        changes: &mut crate::addons::engine::InstallChanges,
    ) -> Result<crate::peer_mutation_executor::PeerAncestorAuthorities, ServiceError> {
        let Some(plan) = self.peer_ancestor_plan.as_ref() else {
            return Ok(crate::peer_mutation_executor::PeerAncestorAuthorities::default());
        };
        let roots = self
            .manifest
            .roots
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        crate::peer_mutation_executor::ancestor_io::create(plan, &roots, changes)
    }

    /// Abandons only a row that is still in `Preparing`.  This is used when
    /// ordinary peer permit minting fails; a `Prepared` row is intentionally
    /// never silently rolled back here and remains owned by recovery.
    pub(crate) fn abandon_preparing(self, context: &Context) -> Result<(), ServiceError> {
        cleanup_preparing(
            context.storage(),
            &self.id,
            Path::new(&self.manifest.transaction_dir),
        )
    }

    /// Rechecks every prepared target immediately before the first forward
    /// write.  This is the O3 fence: the live endpoint must still be absent or
    /// must still match its immutable before-snapshot by no-follow digest and
    /// length.  A mismatch fails closed and leaves the prepared row for the
    /// recovery boundary to resolve.
    pub(crate) fn verify_before_apply(&self) -> Result<(), ServiceError> {
        if let Some(preflight) = self.peer_preflight.as_ref() {
            preflight.verify_current()?;
        }
        if let Some(plan) = self.peer_ancestor_plan.as_ref() {
            let roots = self
                .manifest
                .roots
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            crate::peer_mutation_executor::ancestor_io::verify_before_create(plan, &roots)?;
        }
        for (before, expected_identity) in self
            .manifest
            .snapshots
            .iter()
            .zip(self.before_identities.iter())
        {
            let current = observe_current(Path::new(&before.path))?;
            match (&before.snapshot, expected_identity, current) {
                (None, BeforeIdentity::Absent, CurrentObservation::Absent) => {}
                (
                    Some(snapshot),
                    BeforeIdentity::File {
                        identity,
                        digest: expected_digest,
                        length: expected_length,
                    },
                    CurrentObservation::File {
                        identity: current_identity,
                        digest,
                        length,
                        ..
                    },
                ) if identity.as_str() == current_identity.as_str()
                    && expected_digest.as_str() == digest.as_str()
                    && *expected_length == length =>
                {
                    let bytes = fs::read(snapshot).map_err(|error| {
                        crate::failed(format!(
                            "failed to read before-snapshot {snapshot}: {error}"
                        ))
                    })?;
                    let snapshot_digest = hex::encode(Sha256::digest(&bytes));
                    if digest != snapshot_digest || length != bytes.len() as u64 {
                        return Err(crate::failed(format!(
                            "prepared target {} changed before apply",
                            before.path
                        )));
                    }
                }
                _ => {
                    return Err(crate::failed(format!(
                        "prepared target {} changed before apply",
                        before.path
                    )));
                }
            }
        }
        Ok(())
    }

    /// Restores the exact before-state after durably fencing catalog authority.
    /// A restore error deliberately retains both the Prepared row and matching
    /// invalidation token for retry/recovery.
    pub(crate) fn rollback(self, storage: &SqliteStorage) -> Result<(), ServiceError> {
        super::recover::validate_manifest_scope(&self.manifest, &self.transaction_root)?;
        let fence = storage.fence_prepared_file_mutation_resolution(&self.game_id, &self.id)?;
        restore_manifest(&self.manifest, &fence)?;
        let roots = self
            .manifest
            .roots
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        crate::peer_mutation_executor::ancestor_io::cleanup(&self.manifest.peer_ancestors, &roots);
        storage.complete_prepared_file_mutation_restored(fence)?;
        if let Err(error) = cleanup_manifest(&self.manifest) {
            tracing::warn!(
                "rolled-back file transaction retained its directory; it is not auto-cleaned: {error}"
            );
        }
        Ok(())
    }

    /// Cleans snapshots after the feature commit atomically marked this row
    /// `Committed`.
    pub(crate) fn cleanup_committed(self, storage: &SqliteStorage) -> Result<(), ServiceError> {
        super::recover::validate_manifest_scope(&self.manifest, &self.transaction_root)?;
        cleanup_manifest(&self.manifest)?;
        storage.cleanup_committed_file_mutation(&self.id)?;
        Ok(())
    }

    /// Runs `work` under the durable transaction. On success: calls
    /// `on_committed`, then [`cleanup_committed`]. On failure: calls
    /// [`rollback`], then `on_rolled_back` (if rollback succeeded) or
    /// [`combine_rollback_error`] (if rollback also failed).
    ///
    /// Every site that previously hand-rolled the Ok→cleanup / Err→rollback
    /// match should route through this so the cleanup/rollback contract cannot
    /// be partially skipped.
    pub(crate) fn commit_or_rollback<T, E: Into<crate::ServiceError>>(
        self,
        storage: &SqliteStorage,
        work: impl FnOnce() -> Result<T, E>,
        on_committed: impl FnOnce(&T),
        on_rolled_back: impl FnOnce(),
    ) -> Result<T, crate::ServiceError> {
        match work() {
            Ok(value) => {
                on_committed(&value);
                if let Err(error) = self.cleanup_committed(storage) {
                    tracing::warn!("transaction committed but cleanup is pending: {error}");
                }
                Ok(value)
            }
            Err(error) => {
                let error = error.into();
                match self.rollback(storage) {
                    Ok(()) => {
                        on_rolled_back();
                        Err(error)
                    }
                    Err(rollback_error) => Err(combine_rollback_error(&error, &rollback_error)),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopologyPolicy {
    Ordinary,
    PeerProgramPrepared,
}

/// Complete input for the one durable-preparation pipeline. The peer-only
/// values travel as one closed variant so preflight, endpoint order, and its
/// matching ancestor plan cannot be supplied independently.
struct DurablePreparationInput<'a> {
    scope: &'a MutationScope,
    feature: &'a str,
    subject_id: Option<&'a str>,
    paths: Vec<PathBuf>,
    topology: DurablePreparationTopology<'a>,
}

enum DurablePreparationTopology<'a> {
    Ordinary,
    Peer {
        plan: &'a PeerMutationPlan,
        preflight: PeerMutationPreflight,
        execution_class: PeerExecutionClass,
        ancestor_plan: PeerAncestorPlan,
    },
}

/// Combines a primary error with a rollback failure into a single error so the
/// caller does not silently lose either side. Used by every durable-transaction
/// site after [`DurableFileTransaction::rollback`] fails.
pub(super) fn combine_rollback_error(
    primary: &ServiceError,
    rollback_error: &ServiceError,
) -> ServiceError {
    ServiceError::rollback_also_failed(primary.to_string(), rollback_error.to_string())
}

/// Inputs for one crash-recoverable game-file mutation.
pub(crate) struct DurableMutation<'a> {
    pub(crate) context: &'a Context,
    pub(crate) guard: &'a GameMutationGuard,
    pub(crate) scope: &'a MutationScope,
    pub(crate) feature: &'a str,
    pub(crate) subject_id: Option<&'a str>,
    pub(crate) paths: Vec<PathBuf>,
}

/// Prepares a durable transaction, runs `work` with the reserved `mutation_id`,
/// then cleans up or rolls back. Feature commits (`commit_game_mutation`) stay
/// inside `work` so each command can assemble its own DB half.
pub(crate) fn run_durable_mutation<T, E: Into<ServiceError>>(
    mutation: DurableMutation<'_>,
    work: impl FnOnce(&str) -> Result<T, E>,
    on_committed: impl FnOnce(&T),
    on_rolled_back: impl FnOnce(),
) -> Result<T, ServiceError> {
    let prepared = DurableFileTransaction::prepare(
        mutation.context,
        mutation.guard,
        mutation.scope,
        mutation.feature,
        mutation.subject_id,
        mutation.paths,
    )?;
    prepared.verify_before_apply()?;
    let mutation_id = prepared.id().to_owned();
    prepared.commit_or_rollback(
        mutation.context.storage(),
        || work(&mutation_id),
        on_committed,
        on_rolled_back,
    )
}

fn build_manifest(
    scope: &MutationScope,
    transaction_dir: &Path,
    paths: impl IntoIterator<Item = PathBuf>,
    peer_preflight: Option<&PeerMutationPreflight>,
    peer_ancestor_plan: Option<&PeerAncestorPlan>,
) -> Result<(FileMutationManifest, Vec<BeforeIdentity>), ServiceError> {
    let mut seen = HashSet::new();
    let mut snapshots = Vec::new();
    let mut before_identities = Vec::new();
    for path in paths {
        require_path_in_scope(&path, scope)?;
        if !seen.insert(crate::paths::normalized_key(&path)) {
            continue;
        }
        let before = observe_current(&path)?;
        let expected_peer_before = peer_preflight.and_then(|preflight| {
            preflight
                .program()
                .endpoints()
                .iter()
                .zip(preflight.before().iter())
                .find(|(endpoint, _)| {
                    crate::paths::normalized_key(Path::new(endpoint.path().as_str()))
                        == crate::paths::normalized_key(&path)
                })
                .map(|(_, observation)| observation)
        });
        if let Some(expected) = expected_peer_before {
            if !matches_peer_preimage(expected, &before) {
                return Err(crate::failed(format!(
                    "peer target {} changed between preflight and durable snapshot",
                    path.display()
                )));
            }
            before_identities.push(BeforeIdentity::from_peer_observation(expected));
        } else {
            before_identities.push(BeforeIdentity::from_observation(&before));
        }
        let snapshot = match &before {
            CurrentObservation::Absent => {
                if !matches!(observe_current(&path)?, CurrentObservation::Absent) {
                    return Err(crate::failed(format!(
                        "target {} appeared while preparing its absent preimage",
                        path.display()
                    )));
                }
                None
            }
            CurrentObservation::File {
                identity,
                digest,
                length,
                bytes,
            } => {
                let snapshot_path = transaction_dir.join(format!("{}.before", snapshots.len()));
                crate::fs::write_file_atomically(&snapshot_path, bytes)?;
                let snapshot_bytes = fs::read(&snapshot_path).map_err(|error| {
                    crate::failed(format!(
                        "failed to read before-snapshot {}: {error}",
                        snapshot_path.display()
                    ))
                })?;
                let snapshot_digest = hex::encode(Sha256::digest(&snapshot_bytes));
                if snapshot_bytes.len() as u64 != *length || snapshot_digest != *digest {
                    return Err(crate::failed(format!(
                        "before-snapshot does not match {}",
                        path.display()
                    )));
                }
                let after = observe_current(&path)?;
                if !same_observation(&before, &after, identity, digest, *length) {
                    return Err(crate::failed(format!(
                        "target {} changed while preparing its before-snapshot",
                        path.display()
                    )));
                }
                Some(snapshot_path.to_string_lossy().into_owned())
            }
        };
        snapshots.push(FileBeforeSnapshot {
            path: path.to_string_lossy().into_owned(),
            snapshot,
        });
    }

    Ok((
        FileMutationManifest {
            format_version: MANIFEST_FORMAT_VERSION,
            roots: manifest_roots(scope, peer_preflight.is_some())?,
            transaction_dir: transaction_dir.to_string_lossy().into_owned(),
            snapshots,
            peer_ancestors: peer_ancestor_plan
                .map(PeerAncestorPlan::manifest_entries)
                .unwrap_or_default(),
        },
        before_identities,
    ))
}

fn manifest_roots(scope: &MutationScope, peer: bool) -> Result<Vec<String>, ServiceError> {
    if peer {
        return render_peer_roots(scope.roots())
            .map_err(|error| crate::failed(format!("invalid peer manifest root: {error}")));
    }
    Ok(scope
        .roots()
        .iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect())
}

fn validate_supplied_peer_ancestor_plan(
    preflight: &PeerMutationPreflight,
    ancestor_plan: &PeerAncestorPlan,
) -> Result<(), ServiceError> {
    let expected = preflight.program().endpoints().len();
    if preflight.before().len() != expected
        || (0..expected).any(|ordinal| ancestor_plan.endpoint_chain(ordinal).is_none())
        || ancestor_plan.endpoint_chain(expected).is_some()
    {
        return Err(crate::failed(
            "supplied peer ancestor plan does not match immutable endpoint cardinality",
        ));
    }
    Ok(())
}

fn matches_peer_preimage(expected: &EndpointObservation, actual: &CurrentObservation) -> bool {
    match (expected, actual) {
        (EndpointObservation::Absent, CurrentObservation::Absent) => true,
        (
            EndpointObservation::File(expected),
            CurrentObservation::File {
                identity,
                digest,
                length,
                ..
            },
        ) => {
            expected.identity == *identity
                && expected.digest.as_str() == digest
                && expected.length == *length
        }
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BeforeIdentity {
    Absent,
    File {
        identity: String,
        digest: String,
        length: u64,
    },
}

impl BeforeIdentity {
    fn from_observation(observation: &CurrentObservation) -> Self {
        match observation {
            CurrentObservation::Absent => Self::Absent,
            CurrentObservation::File {
                identity,
                digest,
                length,
                ..
            } => Self::File {
                identity: identity.clone(),
                digest: digest.clone(),
                length: *length,
            },
        }
    }

    fn from_peer_observation(observation: &EndpointObservation) -> Self {
        match observation {
            EndpointObservation::Absent => Self::Absent,
            EndpointObservation::File(file) => Self::File {
                identity: file.identity.clone(),
                digest: file.digest.as_str().to_owned(),
                length: file.length,
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum CurrentObservation {
    Absent,
    File {
        identity: String,
        digest: String,
        length: u64,
        bytes: Vec<u8>,
    },
}

fn observe_current(path: &Path) -> Result<CurrentObservation, ServiceError> {
    let Some(parent) = path.parent() else {
        return Err(crate::failed(format!(
            "path `{}` has no parent directory",
            path.display()
        )));
    };
    match fs::symlink_metadata(parent) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(CurrentObservation::Absent);
        }
        Ok(_) => {
            return Err(crate::failed(format!(
                "cannot inspect non-directory parent of {}",
                path.display()
            )));
        }
        Err(error) => {
            return Err(crate::failed(format!(
                "failed to inspect parent of {}: {error}",
                path.display()
            )));
        }
    }

    let (parent, leaf) = crate::fs::verified_parent(path)?;
    let Some(entry) = parent.observe_leaf(&leaf)? else {
        return Ok(CurrentObservation::Absent);
    };
    if entry.kind != crate::fs::EntryKind::File {
        return Err(crate::failed(format!(
            "cannot mutate non-file path {}",
            path.display()
        )));
    }
    let (bytes, observed) = parent.read_regular_file(&leaf, Some(&entry))?;
    let digest = observed.digest.ok_or_else(|| {
        crate::failed(format!("file digest is unavailable for {}", path.display()))
    })?;
    Ok(CurrentObservation::File {
        identity: observed.identity,
        digest,
        length: bytes.len() as u64,
        bytes,
    })
}

fn same_observation(
    before: &CurrentObservation,
    after: &CurrentObservation,
    identity: &str,
    digest: &str,
    length: u64,
) -> bool {
    matches!(
        (before, after),
        (
            CurrentObservation::File { .. },
            CurrentObservation::File {
                identity: after_identity,
                digest: after_digest,
                length: after_length,
                ..
            }
        ) if after_identity == identity
            && after_digest == digest
            && *after_length == length
    )
}

fn cleanup_preparing(
    storage: &SqliteStorage,
    id: &str,
    transaction_dir: &Path,
) -> Result<(), ServiceError> {
    super::remove_dir_if_exists(transaction_dir)?;
    storage.abandon_file_mutation_preparation(id)?;
    Ok(())
}

fn validate_required_text(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        Err(crate::failed(format!("{field} must not be empty")))
    } else {
        Ok(())
    }
}
