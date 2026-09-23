//! Storage-issued crash recovery for ordinary peer rows.
//!
//! The storage projection is the only input describing endpoint images,
//! snapshots, and cleanup ancestors.  This module performs the same
//! no-follow classification and exact compensation for the sealed ordinary
//! execution class; it never parses a peer manifest or uses generic file recovery.

use std::path::{Path, PathBuf};

use renderpilot_domain::{PathRef, Sha256Hash};
use renderpilot_storage_sqlite::{
    PeerRecoveryEndpoint, PeerRecoveryImage, PeerRecoveryProgram, PendingFileMutationRow,
    PendingFileMutationState,
};

use crate::game_mutation_lock::GameMutationGuard;
use crate::peer_mutation_executor::{PeerPathObservation, observe_peer_path_state};
use crate::{Context, ServiceError};

enum EndpointState {
    NotApplied,
    Applied(PeerPathObservation),
}

/// Resolves one storage-validated ordinary peer row. The caller performs the
/// Preparing abandon-only dispatch before invoking this function.
pub(crate) fn recover_pending_peer(
    context: &Context,
    guard: &GameMutationGuard,
    row: &PendingFileMutationRow,
    program: &PeerRecoveryProgram,
) -> Result<(), ServiceError> {
    if &row.game_id != guard.game_id() {
        return Err(requires_repair(
            row,
            "pending peer mutation belongs to a different game",
        ));
    }
    let transaction_dir = super::validate_transaction_directory_owner(
        context.file_mutation_root(),
        &row.id,
        Path::new(program.transaction_dir()),
    )?;
    match row.state {
        PendingFileMutationState::Preparing => Err(requires_repair(
            row,
            "peer recovery received a Preparing row after the abandon-only boundary",
        )),
        PendingFileMutationState::Committed => {
            super::remove_dir_if_exists(&transaction_dir)?;
            context.storage().cleanup_committed_file_mutation(&row.id)?;
            Ok(())
        }
        PendingFileMutationState::Prepared => {
            restore_prepared(context, guard, row, program, &transaction_dir)
                .map_err(|error| requires_repair(row, &error.to_string()))
        }
    }
}

fn restore_prepared(
    context: &Context,
    guard: &GameMutationGuard,
    row: &PendingFileMutationRow,
    program: &PeerRecoveryProgram,
    transaction_dir: &Path,
) -> Result<(), ServiceError> {
    let mut states = Vec::with_capacity(program.endpoints().len());
    for endpoint in program.endpoints() {
        let observed = observe_endpoint(endpoint)?;
        states.push(classify_endpoint(endpoint, observed)?);
    }

    let snapshots = read_snapshots(program, transaction_dir)?;
    let fence = context
        .storage()
        .fence_prepared_file_mutation_resolution(guard.game_id(), &row.id)?;

    for (endpoint, state) in program.endpoints().iter().zip(states).rev() {
        let EndpointState::Applied(observed) = state else {
            continue;
        };
        restore_endpoint(endpoint, observed, snapshots[endpoint.ordinal()].as_deref())?;
        let after_restore = observe_endpoint(endpoint)?;
        let restored = if endpoint.operation() == renderpilot_domain::PeerEndpointOperation::Remove
        {
            // A snapshot is a transaction artifact, so recreating a removed
            // file necessarily gives it a new native identity.  The retained
            // source/custody route will tighten this to identity-preserving
            // restoration when it is available to the recovery program.
            matches_snapshot_image(&after_restore, endpoint.before())
        } else {
            matches_before_image(&after_restore, endpoint.before())
        };
        if !restored {
            return Err(crate::failed(format!(
                "peer endpoint {} did not return to its durable before image",
                endpoint.ordinal()
            )));
        }
    }

    let roots = program
        .roots()
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    crate::peer_mutation_executor::ancestor_io::cleanup_recovery(
        program.declared_ancestors(),
        &roots,
    );
    context
        .storage()
        .complete_prepared_file_mutation_restored(fence)?;
    if let Err(error) = super::remove_dir_if_exists(transaction_dir) {
        tracing::warn!(
            "recovered peer transaction {} retained its directory; it is not auto-cleaned: {error}",
            row.id
        );
    }
    Ok(())
}

fn observe_endpoint(endpoint: &PeerRecoveryEndpoint) -> Result<PeerPathObservation, ServiceError> {
    observe_peer_path_state(endpoint.path())
}

fn classify_endpoint(
    endpoint: &PeerRecoveryEndpoint,
    observed: PeerPathObservation,
) -> Result<EndpointState, ServiceError> {
    validate_endpoint_transition(endpoint)?;
    if matches_before_image(&observed, endpoint.before()) {
        return Ok(EndpointState::NotApplied);
    }
    if matches_after_image(&observed, endpoint.after()) {
        return Ok(EndpointState::Applied(observed));
    }
    Err(crate::failed(format!(
        "peer endpoint {} is a foreign conflict; recovery retained the Prepared row",
        endpoint.ordinal()
    )))
}

fn validate_endpoint_transition(endpoint: &PeerRecoveryEndpoint) -> Result<(), ServiceError> {
    let valid = match endpoint.operation() {
        renderpilot_domain::PeerEndpointOperation::Create => {
            endpoint.before().is_absent() && endpoint.after().is_file()
        }
        renderpilot_domain::PeerEndpointOperation::Replace => {
            endpoint.before().is_file() && endpoint.after().is_file()
        }
        renderpilot_domain::PeerEndpointOperation::Remove => {
            endpoint.before().is_file() && endpoint.after().is_absent()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(crate::failed(format!(
            "peer endpoint {} has an invalid typed recovery transition",
            endpoint.ordinal()
        )))
    }
}

fn matches_before_image(observed: &PeerPathObservation, image: &PeerRecoveryImage) -> bool {
    match (observed, image.is_absent()) {
        (PeerPathObservation::Absent { .. }, true) => true,
        (
            PeerPathObservation::File {
                bytes, observation, ..
            },
            false,
        ) => {
            image.identity() == Some(observation.identity.as_str())
                && observation.digest.as_deref() == image.sha256().map(Sha256Hash::as_str)
                && image
                    .length()
                    .is_some_and(|length| length == bytes.len() as u64)
        }
        _ => false,
    }
}

fn matches_after_image(observed: &PeerPathObservation, image: &PeerRecoveryImage) -> bool {
    match (observed, image.is_absent()) {
        (PeerPathObservation::Absent { .. }, true) => true,
        (
            PeerPathObservation::File {
                bytes, observation, ..
            },
            false,
        ) => {
            observation.digest.as_deref() == image.sha256().map(Sha256Hash::as_str)
                && image
                    .length()
                    .is_some_and(|length| length == bytes.len() as u64)
        }
        _ => false,
    }
}

/// Snapshot files are transaction artifacts, not the endpoint object whose
/// identity is recorded in the before image.  Their content must still be
/// authenticated, but their native identity is intentionally not compared.
fn matches_snapshot_image(observed: &PeerPathObservation, image: &PeerRecoveryImage) -> bool {
    matches_after_image(observed, image)
}

fn read_snapshots(
    program: &PeerRecoveryProgram,
    transaction_dir: &Path,
) -> Result<Vec<Option<Vec<u8>>>, ServiceError> {
    program
        .endpoints()
        .iter()
        .map(|endpoint| {
            let snapshot_path = endpoint.snapshot_path();
            if endpoint.before().is_absent() {
                if snapshot_path.is_some() {
                    return Err(crate::failed(format!(
                        "peer endpoint {} has a snapshot for an absent before image",
                        endpoint.ordinal()
                    )));
                }
                return Ok(None);
            }
            let snapshot_path = snapshot_path.ok_or_else(|| {
                crate::failed(format!(
                    "peer endpoint {} is missing its required before snapshot",
                    endpoint.ordinal()
                ))
            })?;
            let canonical = super::canonical_candidate(Path::new(snapshot_path))?;
            if !crate::paths::is_within(&canonical, transaction_dir) {
                return Err(crate::failed(format!(
                    "peer endpoint {} snapshot is outside its transaction directory",
                    endpoint.ordinal()
                )));
            }
            let path = path_ref(&canonical)?;
            let observed = observe_peer_path_state(&path)?;
            if !matches_snapshot_image(&observed, endpoint.before()) {
                return Err(crate::failed(format!(
                    "peer endpoint {} before snapshot does not match its durable image",
                    endpoint.ordinal()
                )));
            }
            match observed {
                PeerPathObservation::File { bytes, .. } => Ok(Some(bytes)),
                PeerPathObservation::Absent { .. } => Err(crate::failed(format!(
                    "peer endpoint {} before snapshot is absent",
                    endpoint.ordinal()
                ))),
            }
        })
        .collect()
}

fn restore_endpoint(
    endpoint: &PeerRecoveryEndpoint,
    observed: PeerPathObservation,
    snapshot: Option<&[u8]>,
) -> Result<(), ServiceError> {
    let path = Path::new(endpoint.path().as_str());
    match (endpoint.before().is_absent(), endpoint.after().is_absent()) {
        (true, false) => {
            let PeerPathObservation::File {
                parent,
                leaf,
                observation,
                ..
            } = observed
            else {
                return Err(crate::failed(format!(
                    "peer create endpoint {} lost its applied file",
                    endpoint.ordinal()
                )));
            };
            parent.remove_exact(
                &leaf,
                &observation,
                crate::fs::AuthorityMode::CooperativeSameUid,
            )?;
        }
        (false, true) => {
            if !matches!(observed, PeerPathObservation::Absent { .. }) {
                return Err(crate::failed(format!(
                    "peer remove endpoint {} is not absent in its applied state",
                    endpoint.ordinal()
                )));
            }
            let snapshot = snapshot.ok_or_else(|| {
                crate::failed(format!(
                    "peer remove endpoint {} has no restore snapshot",
                    endpoint.ordinal()
                ))
            })?;
            let (parent, leaf) = crate::fs::verified_parent(path)?;
            match parent.create_file_no_replace(
                &leaf,
                snapshot,
                crate::fs::AuthorityMode::CooperativeSameUid,
            )? {
                crate::fs::CreateFileNoReplace::Created { .. } => {}
                crate::fs::CreateFileNoReplace::Occupied => {
                    return Err(crate::failed(format!(
                        "peer remove endpoint {} became occupied during restore",
                        endpoint.ordinal()
                    )));
                }
            }
        }
        (false, false) => {
            let PeerPathObservation::File {
                parent,
                leaf,
                observation,
                ..
            } = observed
            else {
                return Err(crate::failed(format!(
                    "peer replace endpoint {} lost its applied file",
                    endpoint.ordinal()
                )));
            };
            let snapshot = snapshot.ok_or_else(|| {
                crate::failed(format!(
                    "peer replace endpoint {} has no restore snapshot",
                    endpoint.ordinal()
                ))
            })?;
            parent.overwrite_regular_file_with_stable_identity(
                &leaf,
                &observation,
                &observation.identity,
                snapshot,
            )?;
        }
        (true, true) => {
            return Err(crate::failed(format!(
                "peer endpoint {} has an invalid absent-to-absent transition",
                endpoint.ordinal()
            )));
        }
    }
    Ok(())
}

fn path_ref(path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().replace('\\', "/"))
        .map_err(|error| crate::failed(format!("invalid peer recovery path: {error}")))
}

fn requires_repair(row: &PendingFileMutationRow, detail: &str) -> ServiceError {
    crate::failed(format!(
        "peer mutation {} requires repair; recovery retained the {:?} row: {detail}",
        row.id, row.state
    ))
}
