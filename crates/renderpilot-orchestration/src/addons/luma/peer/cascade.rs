//! Closed lowering for one catalog-backed Luma DLSS cascade endpoint.
//!
//! This module consumes only retained observations and catalog-provided
//! digests.  It deliberately has no filesystem or storage authority.

use std::{error::Error, fmt};

use renderpilot_domain::{PathRef, PeerTransitionError, Sha256Hash};

use crate::peer_mutation_executor::PeerPathSnapshot;

use super::effects::{
    LumaPeerEffectAccumulator, LumaPeerEffectError, LumaPeerEffectGroup, ensure_bytes_match_image,
};
use super::snapshot_input::{
    LumaSnapshotInputError, require_absent, require_file, require_managed_sidecar, snapshot_bytes,
};

/// One of the only physical shapes permitted for a catalog DLSS cascade.
///
/// Expected digests are supplied by the catalog projection, while snapshots
/// are retained observations from the same operation.  The lowerer verifies
/// both before adding any endpoint to the accumulator.
#[derive(Debug, Clone, Copy)]
pub(crate) enum LumaDlssCascadeDecision<'a> {
    /// Remove an active path which has no managed baseline sidecar.
    CurrentOnly {
        live_path: &'a PathRef,
        live_snapshot: &'a PeerPathSnapshot,
        expected_active_digest: &'a Sha256Hash,
    },
    /// Restore the baseline into an active path and remove its sidecar.
    RestorePresent {
        live_path: &'a PathRef,
        sidecar_path: &'a PathRef,
        live_snapshot: &'a PeerPathSnapshot,
        sidecar_snapshot: &'a PeerPathSnapshot,
        expected_active_digest: &'a Sha256Hash,
        expected_baseline_digest: &'a Sha256Hash,
    },
    /// Restore a missing active path from its present managed sidecar.
    RestoreMissingLive {
        live_path: &'a PathRef,
        sidecar_path: &'a PathRef,
        live_snapshot: &'a PeerPathSnapshot,
        sidecar_snapshot: &'a PeerPathSnapshot,
        expected_baseline_digest: &'a Sha256Hash,
    },
    /// The active path already equals its baseline, but the sidecar remains.
    BaselineAlreadyLive {
        live_path: &'a PathRef,
        sidecar_path: &'a PathRef,
        live_snapshot: &'a PeerPathSnapshot,
        sidecar_snapshot: &'a PeerPathSnapshot,
        expected_active_digest: &'a Sha256Hash,
        expected_baseline_digest: &'a Sha256Hash,
    },
    /// The active path already equals its baseline and no sidecar remains.
    BaselineAlreadyLiveWithoutSidecar {
        live_path: &'a PathRef,
        sidecar_path: &'a PathRef,
        live_snapshot: &'a PeerPathSnapshot,
        sidecar_snapshot: &'a PeerPathSnapshot,
        expected_active_digest: &'a Sha256Hash,
        expected_baseline_digest: &'a Sha256Hash,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LumaDlssCascadeLoweringError {
    ExpectedAbsent(PathRef),
    ExpectedFile(PathRef),
    SidecarPathMismatch {
        expected: PathRef,
        supplied: PathRef,
    },
    Domain(PeerTransitionError),
    Effects(LumaPeerEffectError),
    ExpectedDigestMismatch {
        path: PathRef,
        expected: Sha256Hash,
        observed: Sha256Hash,
    },
    RestoreWouldBeByteIdentical(PathRef),
    AlreadyLiveDigestMismatch(PathRef),
}

impl fmt::Display for LumaDlssCascadeLoweringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedAbsent(path) => {
                write!(formatter, "expected an absent DLSS endpoint: {path}")
            }
            Self::ExpectedFile(path) => {
                write!(formatter, "expected a present DLSS endpoint: {path}")
            }
            Self::SidecarPathMismatch { expected, supplied } => write!(
                formatter,
                "DLSS cascade sidecar path {} does not match managed path {}",
                supplied.as_str(),
                expected.as_str()
            ),
            Self::Domain(error) => error.fmt(formatter),
            Self::Effects(error) => error.fmt(formatter),
            Self::ExpectedDigestMismatch {
                path,
                expected,
                observed,
            } => write!(
                formatter,
                "catalog digest mismatch for {}: expected {}, observed {}",
                path.as_str(),
                expected.as_str(),
                observed.as_str()
            ),
            Self::RestoreWouldBeByteIdentical(path) => write!(
                formatter,
                "DLSS cascade restore would be byte-identical and is not a valid replace: {}",
                path.as_str()
            ),
            Self::AlreadyLiveDigestMismatch(path) => write!(
                formatter,
                "DLSS cascade already-live shape requires equal active and baseline digests: {}",
                path.as_str()
            ),
        }
    }
}

impl Error for LumaDlssCascadeLoweringError {}

impl From<LumaSnapshotInputError> for LumaDlssCascadeLoweringError {
    fn from(error: LumaSnapshotInputError) -> Self {
        match error {
            LumaSnapshotInputError::ExpectedAbsent(path) => Self::ExpectedAbsent(path),
            LumaSnapshotInputError::ExpectedFile(path) => Self::ExpectedFile(path),
            LumaSnapshotInputError::SidecarPathMismatch { expected, supplied } => {
                Self::SidecarPathMismatch { expected, supplied }
            }
            LumaSnapshotInputError::Domain(error) => Self::Domain(error),
        }
    }
}

impl From<LumaPeerEffectError> for LumaDlssCascadeLoweringError {
    fn from(error: LumaPeerEffectError) -> Self {
        Self::Effects(error)
    }
}

type LoweringResult<T> = Result<T, LumaDlssCascadeLoweringError>;

pub(crate) fn lower_dlss_cascade_decision(
    decision: LumaDlssCascadeDecision<'_>,
    accumulator: &mut LumaPeerEffectAccumulator,
) -> LoweringResult<()> {
    match decision {
        LumaDlssCascadeDecision::CurrentOnly {
            live_path,
            live_snapshot,
            expected_active_digest,
        } => {
            let live_before =
                require_present_digest(live_path, live_snapshot, expected_active_digest, false)?;
            accumulator.remove(
                LumaPeerEffectGroup::DlssCascade,
                live_path.clone(),
                live_before.0,
            )?;
            Ok(())
        }
        LumaDlssCascadeDecision::RestorePresent {
            live_path,
            sidecar_path,
            live_snapshot,
            sidecar_snapshot,
            expected_active_digest,
            expected_baseline_digest,
        } => {
            let managed_sidecar = require_managed_sidecar(live_path, sidecar_path)?;
            require_distinct_digests(live_path, expected_active_digest, expected_baseline_digest)?;
            let live_before =
                require_present_digest(live_path, live_snapshot, expected_active_digest, false)?;
            let sidecar_before = require_present_digest(
                &managed_sidecar,
                sidecar_snapshot,
                expected_baseline_digest,
                true,
            )?;
            accumulator.release_present(
                LumaPeerEffectGroup::DlssCascade,
                live_path.clone(),
                managed_sidecar,
                live_before.0,
                sidecar_before.0,
                sidecar_before.1,
            )?;
            Ok(())
        }
        LumaDlssCascadeDecision::RestoreMissingLive {
            live_path,
            sidecar_path,
            live_snapshot,
            sidecar_snapshot,
            expected_baseline_digest,
        } => {
            let managed_sidecar = require_managed_sidecar(live_path, sidecar_path)?;
            require_absent(live_path, live_snapshot)?;
            let sidecar_before = require_present_digest(
                &managed_sidecar,
                sidecar_snapshot,
                expected_baseline_digest,
                true,
            )?;
            accumulator.restore_absent(
                LumaPeerEffectGroup::DlssCascade,
                live_path.clone(),
                managed_sidecar,
                sidecar_before.0,
                sidecar_before.1,
            )?;
            Ok(())
        }
        LumaDlssCascadeDecision::BaselineAlreadyLive {
            live_path,
            sidecar_path,
            live_snapshot,
            sidecar_snapshot,
            expected_active_digest,
            expected_baseline_digest,
        } => {
            let managed_sidecar = require_managed_sidecar(live_path, sidecar_path)?;
            require_equal_digests(live_path, expected_active_digest, expected_baseline_digest)?;
            require_present_digest(live_path, live_snapshot, expected_active_digest, false)?;
            let sidecar_before = require_present_digest(
                &managed_sidecar,
                sidecar_snapshot,
                expected_baseline_digest,
                true,
            )?;
            // The live endpoint is intentionally untouched: it is already
            // the baseline.  Removing only the sidecar avoids a byte-identical
            // Replace and leaves the catalog physical guard to cover live.
            accumulator.remove(
                LumaPeerEffectGroup::DlssCascade,
                managed_sidecar,
                sidecar_before.0,
            )?;
            Ok(())
        }
        LumaDlssCascadeDecision::BaselineAlreadyLiveWithoutSidecar {
            live_path,
            sidecar_path,
            live_snapshot,
            sidecar_snapshot,
            expected_active_digest,
            expected_baseline_digest,
        } => {
            require_managed_sidecar(live_path, sidecar_path)?;
            require_equal_digests(live_path, expected_active_digest, expected_baseline_digest)?;
            require_present_digest(live_path, live_snapshot, expected_active_digest, false)?;
            require_absent(sidecar_path, sidecar_snapshot)?;
            Ok(())
        }
    }
}

fn require_present_digest<'a>(
    path: &PathRef,
    snapshot: &'a PeerPathSnapshot,
    expected: &Sha256Hash,
    baseline: bool,
) -> LoweringResult<(&'a crate::peer_mutation_executor::VerifiedPeerFile, Vec<u8>)> {
    let image = require_file(path, snapshot)?;
    let bytes = snapshot_bytes(path, snapshot)?;
    ensure_bytes_match_image(path, &bytes, image, baseline)?;
    if image.digest() != expected {
        return Err(LumaDlssCascadeLoweringError::ExpectedDigestMismatch {
            path: path.clone(),
            expected: expected.clone(),
            observed: image.digest().clone(),
        });
    }
    Ok((image, bytes))
}

fn require_distinct_digests(
    path: &PathRef,
    active: &Sha256Hash,
    baseline: &Sha256Hash,
) -> LoweringResult<()> {
    if active == baseline {
        return Err(LumaDlssCascadeLoweringError::RestoreWouldBeByteIdentical(
            path.clone(),
        ));
    }
    Ok(())
}

fn require_equal_digests(
    path: &PathRef,
    active: &Sha256Hash,
    baseline: &Sha256Hash,
) -> LoweringResult<()> {
    if active != baseline {
        return Err(LumaDlssCascadeLoweringError::AlreadyLiveDigestMismatch(
            path.clone(),
        ));
    }
    Ok(())
}
