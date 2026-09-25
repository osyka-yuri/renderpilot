//! One neutral SQLite commit boundary for coordinated game-file mutations.

use std::collections::BTreeMap;

use renderpilot_application::AppResult;
use renderpilot_domain::{
    AddonKind, ComponentId, ComponentRollbackBaseline, D3d12ExecutableBaseline,
    D3d12ExecutableIdentity, FileOwnership, FileReceipt, GameId, GameProxyTopology, InstalledAddon,
    LibraryComponent, ManagedFileBaseline, ManagedFileMode, OptiScalerFileBaseline,
    OptiScalerFileCleanup, OptiScalerFileRole, OptiScalerInstallState,
    OptiScalerReleaseFileBaseline, PathRef, ProxyImplementation, ProxyRootPrestate, Sha256Hash,
    normalized_path_key,
};
use rusqlite::OptionalExtension;

use super::{
    SqliteStorage, component_backups, components, installed_addons, observations,
    optiscaler_states, pending_file_mutations, proxy_topologies,
};

mod core;
mod optiscaler_validation;
mod peer_binding;
mod peer_conversion;
mod persistence;
mod projection;
#[cfg(test)]
mod tests;

pub(super) mod peer_topology;

pub(crate) use core::apply_catalog_projection_within_transaction;

/// Narrow aggregate-only seam for the peer runtime.  The ordinary public
/// commit entrypoint continues to own its reservation blocker; this wrapper
/// exposes only the already-validated participant primitive to the aggregate
/// runtime's enclosing transaction.
pub(crate) fn apply_optiscaler_transition_within_transaction(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
    mutation: OptiScalerAggregateMutation<'_>,
) -> AppResult<()> {
    apply_optiscaler_transition(transaction, game_id, mutation)
}

/// Narrow aggregate-only seam for the existing OptiScaler transition
/// validator.  It deliberately does not perform or bypass reservation checks.
pub(crate) fn validate_optiscaler_transition_within_transaction<'a>(
    transaction: &rusqlite::Transaction<'_>,
    game_id: &GameId,
    mutation: OptiScalerAggregateMutation<'a>,
) -> AppResult<Option<&'a str>> {
    validate_optiscaler_transition(transaction, game_id, mutation)
}
pub use core::{
    ComponentBaselineMutation, GameMutationCommit, InstalledAddonMutation,
    OptiScalerAggregateMutation, OptiScalerAuxiliaryPreservation, OptiScalerPeerMutation,
    OptiScalerRetainedClaim,
};
use optiscaler_validation::validate_optiscaler_transition;
use optiscaler_validation::{ExpectedPeerRelocation, PeerAggregateBinding};
#[cfg(test)]
use peer_binding::ensure_exact_peer_relocation;
use peer_binding::{
    build_optiscaler_binding, expected_peer_relocation, peer_receipt_claims_source,
    validate_peer_receipt_transition,
};
use persistence::apply_optiscaler_transition;
use persistence::{
    ensure_exact_before_peer, ensure_exact_before_state, ensure_exact_before_topology,
    ensure_peer_game, ensure_peer_paths_within_install, ensure_state_game,
    ensure_state_paths_within_install, ensure_state_topology, ensure_topology_game,
    ensure_topology_paths_within_install, persisted_install_root,
    validate_optiscaler_receipt_metadata,
};
use projection::{
    collect_optiscaler_directories, collect_optiscaler_receipts, directory_path_map, insert_path,
    is_owned, is_reused, optiscaler_transition_feature, optiscaler_transition_paths,
    peer_owned_sidecar_path_map, peer_path_map, release_path_map, runtime_path_map,
    same_receipt_identity, same_receipt_identity_digest_ownership, stable_topology_subject,
    topology_path_map,
};
