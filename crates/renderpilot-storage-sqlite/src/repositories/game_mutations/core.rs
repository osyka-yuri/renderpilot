use super::*;

/// Typed mutation of the component rollback aggregate.
#[derive(Debug, Clone, Copy)]
pub enum ComponentBaselineMutation<'a> {
    /// Capture immutable original DLL/EXE identities once.
    Capture {
        /// Component receiving the baseline.
        component_id: &'a ComponentId,
        /// Exact pre-first-overlay rollback aggregate.
        baseline: &'a ComponentRollbackBaseline,
    },
    /// Capture the D3D12 executable original identity once for a DLL-only aggregate.
    CaptureD3d12Executable {
        /// Component receiving its first auxiliary baseline.
        component_id: &'a ComponentId,
        /// Immutable original and initial active executable identity.
        baseline: &'a D3d12ExecutableBaseline,
    },
    /// Change only the expected active D3D12 executable identity.
    UpdateD3d12ExecutableState {
        /// Component whose auxiliary state changed.
        component_id: &'a ComponentId,
        /// New active state; original path and identity remain storage-owned.
        expected_active: &'a D3d12ExecutableIdentity,
    },
    /// Update the last component-file identities committed by RenderPilot.
    ///
    /// This provenance lets cleanup validate an orphaned rollback aggregate
    /// after a later scan no longer detects the component row.
    UpdateExpectedActiveFiles {
        /// Component whose active identity changed.
        component_id: &'a ComponentId,
        /// Complete active file set produced by the committed replacement.
        files: &'a [renderpilot_domain::ComponentFile],
    },
    /// Delete the aggregate after a fully verified rollback.
    Delete {
        /// Component whose rollback aggregate was consumed.
        component_id: &'a ComponentId,
    },
}

/// Optional installed-add-on row change in the same transaction.
#[derive(Debug, Clone, Copy, Default)]
pub enum InstalledAddonMutation<'a> {
    /// Leave the installed-add-on table unchanged.
    #[default]
    Keep,
    /// Insert or replace one validated record.
    Upsert(&'a InstalledAddon),
    /// Delete the selected kind for the commit's game.
    Delete(AddonKind),
    /// Replace one exact inactive owner after the caller's prepared filesystem
    /// mutation has completed. This is the only admitted cross-kind file-row
    /// transition; standalone Engine.ini ownership is not changed.
    ReplaceExpected {
        /// Complete durable owner observed before native preparation.
        expected: &'a InstalledAddon,
        /// Replacement owner after the prepared native effects are verified.
        replacement: &'a InstalledAddon,
    },
    /// Closed OptiScaler aggregate transition. The peer receipt, when any,
    /// is nested in this variant and cannot be committed independently.
    OptiScaler(OptiScalerAggregateMutation<'a>),
}

/// Closed OptiScaler state/topology/filesystem transition.
#[derive(Debug, Clone, Copy)]
pub enum OptiScalerAggregateMutation<'a> {
    /// Metadata-only adoption. All filesystem, path, topology, and peer
    /// identity is captured exactly from the live game and no pending mutation
    /// row is involved.
    AdoptExactMetadata {
        /// Newly adopted state. No OptiScaler state may already exist.
        state: &'a OptiScalerInstallState,
        /// Newly adopted topology. No proxy topology may already exist.
        topology: &'a GameProxyTopology,
    },
    /// Filesystem-backed transition with an exact custody journal binding.
    Filesystem {
        /// State before the live operation, if one existed.
        before_state: Option<&'a OptiScalerInstallState>,
        /// State after the live operation, if one remains.
        after_state: Option<&'a OptiScalerInstallState>,
        /// Topology before the live operation, if one existed.
        before_topology: Option<&'a GameProxyTopology>,
        /// Topology after the live operation, if one remains.
        after_topology: Option<&'a GameProxyTopology>,
        /// Optional peer receipt transition owned by this aggregate.
        peer: OptiScalerPeerMutation<'a>,
        /// Exact durable filesystem transaction id.
        mutation_id: &'a str,
    },
    /// Filesystem transition with an explicit, typed set of app-owned
    /// preservation destinations.
    FilesystemWithAuxiliary {
        /// State before the live operation, if one existed.
        before_state: Option<&'a OptiScalerInstallState>,
        /// State after the live operation, if one remains.
        after_state: Option<&'a OptiScalerInstallState>,
        /// Topology before the live operation, if one existed.
        before_topology: Option<&'a GameProxyTopology>,
        /// Topology after the live operation, if one remains.
        after_topology: Option<&'a GameProxyTopology>,
        /// Optional peer receipt transition owned by this aggregate.
        peer: OptiScalerPeerMutation<'a>,
        /// Exact app-owned recovery copies made by the filesystem transition.
        auxiliary_preservations: &'a [OptiScalerAuxiliaryPreservation],
        /// Exact live files whose custody remains with another installed
        /// add-on while OptiScaler drops its overlapping receipt.
        retained_claims: &'a [OptiScalerRetainedClaim],
        /// Exact durable filesystem transaction id.
        mutation_id: &'a str,
    },
}

/// Optional peer receipt transition nested inside an OptiScaler aggregate.
#[derive(Debug, Clone, Copy, Default)]
pub enum OptiScalerPeerMutation<'a> {
    /// Leave the peer InstalledAddon receipt unchanged.
    #[default]
    Keep,
    /// Replace one peer receipt with an exact same-kind before/after fence.
    Replace {
        /// Persisted receipt before the filesystem transition.
        before: &'a InstalledAddon,
        /// Receipt after the filesystem transition.
        after: &'a InstalledAddon,
    },
}

/// Typed app-owned recovery copy participating in an OptiScaler aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptiScalerAuxiliaryPreservation {
    /// Exact owned source whose preimage is copied before custody is released.
    pub source: PathRef,
    /// Fresh recovery destination written by the custody journal.
    pub destination: PathRef,
    /// Exact current source receipt captured after the preservation copy.
    ///
    /// The identity must match the prior source receipt in `before_state`; the
    /// digest may differ when the user edited the file before uninstall.
    pub source_current: FileReceipt,
    /// Exact owned receipt of the copied recovery object.  Its digest must
    /// equal `source_current`, while its identity is independently verified.
    pub destination_receipt: FileReceipt,
}

/// One non-mutating custody handoff to another persisted add-on receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptiScalerRetainedClaim {
    /// Shared live path which OptiScaler must leave unchanged.
    pub path: PathRef,
    /// Exact reused receipt owned by the remaining add-on.
    pub receipt: FileReceipt,
}

/// Complete database half of one durable game-file transaction.
#[derive(Debug, Clone, Copy)]
pub struct GameMutationCommit<'a> {
    /// Game whose coordinated state is changing.
    pub game_id: &'a GameId,
    /// Optional full component-set replacement.
    pub component_set: Option<&'a [LibraryComponent]>,
    /// Typed rollback aggregate mutations.
    pub baseline_mutations: &'a [ComponentBaselineMutation<'a>],
    /// Optional installed-add-on mutation.
    pub addon: InstalledAddonMutation<'a>,
    /// Durable filesystem mutation row to mark committed with the feature rows.
    ///
    /// `None` is for metadata-only commits where no game-file root is reachable
    /// (orphaned install cleanup): feature rows still commit atomically, but no
    /// pending file-mutation row is required.
    pub mutation_id: Option<&'a str>,
}

pub(crate) fn apply_catalog_projection_within_transaction(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
    component_set: Option<&[LibraryComponent]>,
    baseline_mutations: &[ComponentBaselineMutation<'_>],
) -> AppResult<()> {
    if let Some(component_set) = component_set {
        components::replace_components_for_game_within_transaction(
            transaction,
            game_id,
            component_set,
        )?;
    }
    for mutation in baseline_mutations {
        match mutation {
            ComponentBaselineMutation::Capture {
                component_id,
                baseline,
            } => component_backups::capture_component_backup_within_transaction(
                transaction,
                game_id,
                component_id,
                baseline,
            )?,
            ComponentBaselineMutation::UpdateD3d12ExecutableState {
                component_id,
                expected_active,
            } => component_backups::update_component_d3d12_executable_state_within_transaction(
                transaction,
                component_id,
                expected_active,
            )?,
            ComponentBaselineMutation::UpdateExpectedActiveFiles {
                component_id,
                files,
            } => component_backups::update_component_expected_active_files_within_transaction(
                transaction,
                component_id,
                files,
            )?,
            ComponentBaselineMutation::CaptureD3d12Executable {
                component_id,
                baseline,
            } => component_backups::capture_component_d3d12_executable_within_transaction(
                transaction,
                component_id,
                baseline,
            )?,
            ComponentBaselineMutation::Delete { component_id } => {
                component_backups::delete_component_backup_within_transaction(
                    transaction,
                    component_id,
                )?;
            }
        }
    }
    Ok(())
}

impl SqliteStorage {
    /// Atomically commits feature rows and, when present, the durable filesystem phase.
    pub fn commit_game_mutation(&self, commit: GameMutationCommit<'_>) -> AppResult<()> {
        self.with_transaction(|transaction| {
            super::super::peer_aggregate_reservations::ensure_no_peer_aggregate_reservation_within_transaction(
                transaction,
                commit.game_id,
            )?;
            match commit.addon {
                InstalledAddonMutation::Upsert(addon) => {
                    installed_addons::ensure_independent_peer_mutation_allowed(
                        transaction,
                        commit.game_id,
                        addon.kind(),
                    )?;
                }
                InstalledAddonMutation::Delete(kind) => {
                    installed_addons::ensure_independent_peer_mutation_allowed(
                        transaction,
                        commit.game_id,
                        kind,
                    )?;
                }
                InstalledAddonMutation::ReplaceExpected {
                    expected,
                    replacement,
                } => {
                    installed_addons::ensure_independent_peer_mutation_allowed(
                        transaction,
                        commit.game_id,
                        expected.kind(),
                    )?;
                    installed_addons::ensure_independent_peer_mutation_allowed(
                        transaction,
                        commit.game_id,
                        replacement.kind(),
                    )?;
                    if commit.mutation_id.is_none() {
                        return Err(renderpilot_application::AppError::invalid_input(
                            "expected-owner replacement requires a prepared file mutation",
                        ));
                    }
                    if expected.game_id() != commit.game_id
                        || replacement.game_id() != commit.game_id
                    {
                        return Err(renderpilot_application::AppError::invalid_input(
                            "expected owner and replacement must belong to the commit game",
                        ));
                    }
                    if expected.kind() == AddonKind::OptiScaler
                        || replacement.kind() == AddonKind::OptiScaler
                    {
                        return Err(renderpilot_application::AppError::invalid_input(
                            "OptiScaler cannot use expected-owner add-on replacement",
                        ));
                    }
                }
                InstalledAddonMutation::Keep
                | InstalledAddonMutation::OptiScaler(_) => {}
            }
            let is_optiscaler_aggregate =
                matches!(commit.addon, InstalledAddonMutation::OptiScaler(_));
            let optiscaler_mutation_id = match commit.addon {
                InstalledAddonMutation::OptiScaler(mutation) => {
                    validate_optiscaler_transition(transaction, commit.game_id, mutation)?
                }
                _ => None,
            };
            if is_optiscaler_aggregate && (commit.mutation_id.is_some()
                || commit.component_set.is_some()
                || !commit.baseline_mutations.is_empty())
            {
                return Err(renderpilot_application::AppError::invalid_input(
                    "OptiScaler aggregate cannot be combined with independent component or file mutations",
                ));
            }
            let prepared_binding = commit.mutation_id.map(|mutation_id| {
                pending_file_mutations::validate_prepared_mutation_commit_within_transaction(
                    transaction,
                    commit.game_id,
                    mutation_id,
                    commit.component_set.map(<[LibraryComponent]>::len),
                    !commit.baseline_mutations.is_empty(),
                )
            }).transpose()?;
            if matches!(commit.addon, InstalledAddonMutation::ReplaceExpected { .. })
                && prepared_binding
                    != Some(pending_file_mutations::PreparedMutationCommitBinding::CatalogInvalidated)
            {
                return Err(renderpilot_application::AppError::storage_failed(
                    "expected-owner replacement requires an invalidated catalog prepared-file fence",
                ));
            }
            let skip_absent_empty_component_replacement = matches!(
                (prepared_binding, commit.component_set),
                (
                    Some(pending_file_mutations::PreparedMutationCommitBinding::CatalogAbsent),
                    Some(component_set)
                ) if component_set.is_empty()
            );
            if let Some(component_set) = commit.component_set
                && !skip_absent_empty_component_replacement
            {
                components::replace_components_for_game_within_transaction(
                    transaction,
                    commit.game_id,
                    component_set,
                )?;
                if commit.mutation_id.is_none() {
                    observations::invalidate_game_authority_within_transaction(
                        transaction,
                        commit.game_id,
                        "game_mutation_component_set",
                        None,
                    )?;
                }
            }
            for mutation in commit.baseline_mutations {
                    match mutation {
                        ComponentBaselineMutation::Capture {
                            component_id,
                            baseline,
                        } => component_backups::capture_component_backup_within_transaction(
                            transaction,
                            commit.game_id,
                            component_id,
                            baseline,
                        )?,
                        ComponentBaselineMutation::UpdateD3d12ExecutableState {
                            component_id,
                            expected_active,
                        } => {
                            component_backups::update_component_d3d12_executable_state_within_transaction(
                                transaction,
                                component_id,
                                expected_active,
                            )?;
                        }
                        ComponentBaselineMutation::UpdateExpectedActiveFiles {
                            component_id,
                            files,
                        } => component_backups::update_component_expected_active_files_within_transaction(
                            transaction,
                            component_id,
                            files,
                        )?,
                        ComponentBaselineMutation::CaptureD3d12Executable {
                            component_id,
                            baseline,
                        } => {
                            component_backups::capture_component_d3d12_executable_within_transaction(
                                transaction,
                                component_id,
                                baseline,
                            )?;
                        }
                        ComponentBaselineMutation::Delete { component_id } => {
                            component_backups::delete_component_backup_within_transaction(
                                transaction,
                                component_id,
                            )?;
                        }
                    }
            }
            match commit.addon {
                InstalledAddonMutation::Keep => {}
                InstalledAddonMutation::Upsert(addon) => {
                    installed_addons::upsert_within_transaction(transaction, addon)?;
                }
                InstalledAddonMutation::Delete(kind) => {
                    installed_addons::delete_within_transaction(transaction, commit.game_id, kind)?;
                }
                InstalledAddonMutation::ReplaceExpected {
                    expected,
                    replacement,
                } => {
                    installed_addons::replace_expected_within_transaction(
                        transaction,
                        commit.game_id,
                        expected,
                        replacement,
                    )?;
                }
                InstalledAddonMutation::OptiScaler(mutation) => {
                    apply_optiscaler_transition(transaction, commit.game_id, mutation)?;
                }
            }
            if let Some(mutation_id) = commit.mutation_id {
                pending_file_mutations::mark_file_mutation_committed_within_transaction(
                    transaction,
                    mutation_id,
                )?;
            }
            if let Some(mutation_id) = optiscaler_mutation_id {
                pending_file_mutations::mark_file_mutation_committed_within_transaction(
                    transaction,
                    mutation_id,
                )?;
            }
            Ok(())
        })
    }
}
