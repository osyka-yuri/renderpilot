//! Consuming peer commits and the single SQLite CAS boundary.

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::PeerEndpointEvidence;
use renderpilot_domain::PeerReadGuardEvidence;
use rusqlite::{Transaction, named_params};

use super::super::read_guards::ReadGuardPermitState;
use super::super::shared_roots;
use super::super::validation::{
    materialize_topology, validate_file_seal, validate_sealed_evidence, validate_shared_seal,
};
use super::fingerprint::{
    apply_peer_snapshots, ensure_runtime, read_file_fingerprint, read_shared_fingerprint,
    read_shared_root_capabilities,
};
use super::types::{PeerStorageRuntime, PreparedPeerCommitPermit, PreparedRowBinding};
use crate::repositories::{
    InstalledAddonMutation, SharedArtifactMutation, SharedVulkanMutationCommit,
    SharedVulkanMutationScope, installed_addons, pending_file_mutations, proxy_topologies,
};
use crate::{error::storage_error, sqlite_clock};

impl PeerStorageRuntime {
    /// Consumes an ordinary peer permit after strict O1/O2 evidence validation.
    pub fn seal_and_commit_ordinary_peer(
        &self,
        permit: PreparedPeerCommitPermit,
        evidence: Vec<PeerEndpointEvidence>,
        read_guard_evidence: Vec<PeerReadGuardEvidence>,
    ) -> AppResult<()> {
        self.seal_and_commit_file_peer(permit, evidence, read_guard_evidence)
    }

    fn seal_and_commit_file_peer(
        &self,
        permit: PreparedPeerCommitPermit,
        evidence: Vec<PeerEndpointEvidence>,
        read_guard_evidence: Vec<PeerReadGuardEvidence>,
    ) -> AppResult<()> {
        ensure_runtime(&self.instance, &permit.instance)?;
        if permit.binding != PreparedRowBinding::File {
            return Err(AppError::storage_failed(
                "shared peer permit cannot commit a file mutation",
            ));
        }
        let ReadGuardPermitState::File {
            canonical_game_root,
            requirements,
            initial_evidence,
            catalog_claim,
        } = &permit.read_guard_state
        else {
            return Err(AppError::storage_failed(
                "file peer permit has no file read-guard state",
            ));
        };
        let companions = if let Some(companion) = permit.optiscaler_config.as_ref() {
            if permit.contract().renodx_dlss_projection().is_some() {
                return Err(AppError::storage_failed(
                    "OptiScaler configuration companion cannot share a RenoDX DLSS permit",
                ));
            }
            super::super::read_guards::ReadGuardCompanions {
                catalog: None,
                catalog_claim: catalog_claim.as_ref(),
                renodx_reshade_ini: permit.contract().renodx_reshade_ini_authority(),
                dlss: None,
                optiscaler_config: Some(companion.projection()),
            }
        } else if let Some(projection) = permit.contract().renodx_dlss_projection() {
            super::super::read_guards::ReadGuardCompanions {
                catalog: None,
                catalog_claim: catalog_claim.as_ref(),
                renodx_reshade_ini: permit.contract().renodx_reshade_ini_authority(),
                dlss: Some(projection),
                optiscaler_config: None,
            }
        } else {
            super::super::read_guards::ReadGuardCompanions {
                catalog: None,
                catalog_claim: catalog_claim.as_ref(),
                renodx_reshade_ini: permit.contract().renodx_reshade_ini_authority(),
                dlss: None,
                optiscaler_config: None,
            }
        };
        super::super::read_guards::validate_final(
            super::super::read_guards::FinalReadGuardInput {
                canonical_game_root,
                transition: super::super::read_guards::ReadGuardTransition {
                    sealed_roots: permit.program.roots(),
                    before_peer: permit.before_peer.as_ref(),
                    after_peer: permit.after_peer.as_ref(),
                    before_topology: permit.before_topology.as_ref(),
                    planned_after_topology: permit.planned_after_topology.as_ref(),
                    route: permit.route,
                    intents: permit.program.intents(),
                    program_before: permit.program.before(),
                },
                stored_requirements: requirements,
                initial_evidence,
                final_evidence: &read_guard_evidence,
                companions,
            },
        )?;
        let result = self.storage.with_transaction(|transaction| {
            let current = read_file_fingerprint(transaction, &permit.mutation_id)?
                .ok_or_else(|| AppError::storage_failed("peer Prepared row is missing"))?;
            if current != permit.fingerprint {
                return Err(AppError::storage_failed(
                    "peer Prepared row fingerprint changed before commit",
                ));
            }
            validate_file_seal(transaction, &permit)?;
            validate_sealed_evidence(&permit, &evidence)?;
            let topology = materialize_topology(&permit, &evidence)?;
            let current_peer =
                installed_addons::get_within_transaction(transaction, &permit.game_id)?;
            if current_peer.as_ref() != permit.before_peer.as_ref() {
                return Err(AppError::storage_failed(
                    "peer addon preimage changed before commit",
                ));
            }
            if let Some(projection) = permit.contract().renodx_dlss_projection() {
                projection
                    .validate_against_peers(current_peer.as_ref(), permit.after_peer.as_ref())
                    .map_err(super::domain_error)?;
            }
            let optiscaler_successor = permit
                .optiscaler_config
                .as_ref()
                .map(|companion| {
                    super::super::optiscaler_config::successor_from_evidence(
                        companion,
                        &permit.program,
                        &evidence,
                    )
                })
                .transpose()?;
            if proxy_topologies::get_within_transaction(transaction, &permit.game_id)?.as_ref()
                != permit.before_topology.as_ref()
            {
                return Err(AppError::storage_failed(
                    "peer topology preimage changed before commit",
                ));
            }
            if let Some(claim) = permit.catalog.catalog_claim() {
                super::super::catalog::validate_claim_preimage_within_transaction(
                    transaction,
                    &permit.game_id,
                    claim,
                )?;
            }
            let baseline_mutations = permit
                .catalog
                .baseline_mutations
                .iter()
                .map(super::super::catalog::OwnedBaselineMutation::as_borrowed)
                .collect::<Vec<_>>();
            pending_file_mutations::validate_prepared_mutation_commit_within_transaction(
                transaction,
                &permit.game_id,
                &permit.mutation_id,
                permit.catalog.component_set.as_ref().map(Vec::len),
                !baseline_mutations.is_empty(),
            )?;
            crate::repositories::apply_catalog_projection_within_transaction(
                transaction,
                &permit.game_id,
                permit.catalog.component_set.as_deref(),
                &baseline_mutations,
            )?;
            if let (Some(companion), Some(successor)) = (
                permit.optiscaler_config.as_ref(),
                optiscaler_successor.as_ref(),
            ) {
                crate::repositories::commit_optiscaler_config_companion_within_transaction(
                    transaction,
                    &permit.game_id,
                    companion.before_state(),
                    successor,
                )?;
            }
            apply_peer_snapshots(
                transaction,
                &permit.game_id,
                permit.after_peer.as_ref(),
                topology.as_ref(),
            )?;
            pending_file_mutations::mark_file_mutation_committed_within_transaction(
                transaction,
                &permit.mutation_id,
            )
        });
        drop(permit);
        drop(evidence);
        drop(read_guard_evidence);
        result
    }

    /// Consumes a game-shared peer permit and commits all shared projections
    /// in the same SQLite transaction as the row transition.
    pub fn seal_and_commit_shared_peer(
        &self,
        permit: PreparedPeerCommitPermit,
        evidence: Vec<PeerEndpointEvidence>,
        addon: InstalledAddonMutation<'_>,
        shared_artifact: SharedArtifactMutation<'_>,
    ) -> AppResult<()> {
        ensure_runtime(&self.instance, &permit.instance)?;
        if permit.binding != PreparedRowBinding::Shared {
            return Err(AppError::storage_failed(
                "file peer permit cannot commit a shared mutation",
            ));
        }
        let ReadGuardPermitState::Shared { roots } = &permit.read_guard_state else {
            return Err(AppError::storage_failed(
                "shared peer permit carries file read-guard state",
            ));
        };
        let result = self.storage.with_transaction(|transaction| {
            let current = read_shared_fingerprint(transaction, &permit.mutation_id)?
                .ok_or_else(|| AppError::storage_failed("shared peer Prepared row is missing"))?;
            if current != permit.fingerprint {
                return Err(AppError::storage_failed(
                    "shared peer Prepared row fingerprint changed before commit",
                ));
            }
            let root_capabilities = read_shared_root_capabilities(transaction, &permit.mutation_id)?;
            let current_roots = shared_roots::bind_persisted_json(
                &root_capabilities,
                permit.program.roots(),
            )?;
            if &current_roots != roots {
                return Err(AppError::storage_failed(
                    "shared peer root authority changed before commit",
                ));
            }
            validate_shared_seal(transaction, &permit)?;
            let commit = SharedVulkanMutationCommit {
                id: &permit.mutation_id,
                scope: SharedVulkanMutationScope::GameShared,
                game_id: Some(&permit.game_id),
                addon,
                shared_artifact,
            };
            crate::repositories::pending_shared_vulkan_mutations::
                validate_prepared_shared_vulkan_mutation_commit_within_transaction(
                    transaction,
                    &commit,
                )?;
            validate_sealed_evidence(&permit, &evidence)?;
            let topology = materialize_topology(&permit, &evidence)?;
            if installed_addons::get_within_transaction(transaction, &permit.game_id)?.as_ref()
                != permit.before_peer.as_ref()
            {
                return Err(AppError::storage_failed(
                    "shared peer addon preimage changed before commit",
                ));
            }
            if proxy_topologies::get_within_transaction(transaction, &permit.game_id)?.as_ref()
                != permit.before_topology.as_ref()
            {
                return Err(AppError::storage_failed(
                    "shared peer topology preimage changed before commit",
                ));
            }
            apply_peer_snapshots(
                transaction,
                &permit.game_id,
                permit.after_peer.as_ref(),
                topology.as_ref(),
            )?;
            apply_addon(transaction, &permit.game_id, addon)?;
            apply_shared_artifact(transaction, shared_artifact)?;
            let now_ms = sqlite_clock::now_ms(transaction)?;
            let updated = transaction
                .execute(
                    "UPDATE pending_shared_vulkan_mutations
                     SET state = 'committed', updated_at = :now_ms
                     WHERE resource_key = :resource_key AND id = :id
                       AND scope = 'game_shared' AND game_id = :game_id
                       AND feature = :feature AND state = 'prepared'",
                    named_params! {
                        ":resource_key": crate::repositories::pending_shared_vulkan_mutations::RESOURCE_KEY,
                        ":id": permit.mutation_id.as_str(),
                        ":game_id": permit.game_id.as_str(),
                        ":feature": permit.feature.as_str(),
                        ":now_ms": now_ms,
                    },
                )
                .map_err(storage_error)?;
            if updated != 1 {
                return Err(AppError::storage_failed(
                    "shared peer Prepared row changed before commit",
                ));
            }
            Ok(())
        });
        drop(permit);
        drop(evidence);
        result
    }
}

fn apply_addon(
    transaction: &Transaction<'_>,
    game_id: &renderpilot_domain::GameId,
    addon: InstalledAddonMutation<'_>,
) -> AppResult<()> {
    match addon {
        InstalledAddonMutation::Keep => Ok(()),
        InstalledAddonMutation::Upsert(addon) => {
            installed_addons::upsert_within_transaction(transaction, addon)
        }
        InstalledAddonMutation::Delete(kind) => {
            installed_addons::delete_within_transaction(transaction, game_id, kind)
        }
        InstalledAddonMutation::ReplaceExpected { .. } => Err(AppError::invalid_input(
            "expected-owner replacement is only valid through the prepared game file mutation commit",
        )),
        InstalledAddonMutation::OptiScaler(_) => Err(AppError::invalid_input(
            "shared peer commit cannot carry an OptiScaler aggregate",
        )),
    }
}

fn apply_shared_artifact(
    transaction: &Transaction<'_>,
    shared_artifact: SharedArtifactMutation<'_>,
) -> AppResult<()> {
    match shared_artifact {
        SharedArtifactMutation::Keep => Ok(()),
        SharedArtifactMutation::Upsert(record) => {
            crate::repositories::upsert_within_transaction(transaction, record)
        }
        SharedArtifactMutation::Delete(kind) => {
            crate::repositories::delete_within_transaction(transaction, kind)
        }
    }
}
