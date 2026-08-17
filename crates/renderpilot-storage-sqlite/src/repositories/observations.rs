//! Typed catalog readiness and owner-scoped strong file observations.
//!
//! This module is deliberately the only persistence boundary that can change a
//! game's scan authority. Component replacement and durable file mutations call
//! the private helpers below; a scan can publish Complete only through its CAS
//! write unit.

use std::collections::HashMap;

use renderpilot_application::AppResult;
use renderpilot_domain::{ArtifactId, GameId, PathRef, Sha256Hash, Version};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, named_params};

use crate::{
    error::{invalid_row, storage_error},
    sqlite_clock,
};

use super::SqliteStorage;

/// The only readiness states a persisted catalog scan may expose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogReadiness {
    /// No complete scan has ever been published for this installation.
    NeverCompleted {
        /// Monotonic authority generation for this game.
        authority_epoch: u64,
    },
    /// A complete scan has published the authoritative component projection.
    Complete(CatalogReadyProjection),
    /// A filesystem or component mutation invalidated the prior projection.
    Invalidated {
        /// Monotonic authority generation after the invalidating event.
        authority_epoch: u64,
        /// Stable reason recorded for recovery and diagnostics.
        reason: String,
        /// Durable mutation id when invalidation is tied to filesystem work.
        mutation_token: Option<String>,
    },
}

impl CatalogReadiness {
    /// Returns the current monotonic authority generation.
    #[must_use]
    pub fn authority_epoch(&self) -> u64 {
        match self {
            Self::NeverCompleted { authority_epoch }
            | Self::Invalidated {
                authority_epoch, ..
            } => *authority_epoch,
            Self::Complete(ready) => ready.authority_epoch,
        }
    }

    /// Returns the storage-derived ready capability only when complete.
    #[must_use]
    pub fn ready_projection(&self) -> Option<&CatalogReadyProjection> {
        match self {
            Self::Complete(ready) => Some(ready),
            Self::NeverCompleted { .. } | Self::Invalidated { .. } => None,
        }
    }
}

/// Read-only proof that a particular game has a completed authoritative scan.
///
/// Fields are intentionally private: callers cannot manufacture a Complete
/// value and hand it to a consumer detached from storage authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogReadyProjection {
    game_id: GameId,
    authority_epoch: u64,
}

impl CatalogReadyProjection {
    /// Returns the game whose catalog projection is ready.
    #[must_use]
    pub fn game_id(&self) -> &GameId {
        &self.game_id
    }

    /// Returns the generation published by this complete scan.
    #[must_use]
    pub fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }
}

/// Compare-and-swap token captured before a scan reads the installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorityCas {
    expected_epoch: u64,
}

impl AuthorityCas {
    /// Builds a token from a readiness generation captured before scanning.
    #[must_use]
    pub fn new(expected_epoch: u64) -> Self {
        Self { expected_epoch }
    }

    /// Returns the generation that a publication must still observe.
    #[must_use]
    pub fn expected_epoch(&self) -> u64 {
        self.expected_epoch
    }
}

/// Which durable catalog object owns an observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationOwner {
    /// A game installation's reconciled scan owns the fact.
    Game(GameId),
    /// A downloaded/local artifact verifier owns the fact.
    Artifact(ArtifactId),
}

impl ObservationOwner {
    fn kind(&self) -> &'static str {
        match self {
            Self::Game(_) => "game",
            Self::Artifact(_) => "artifact",
        }
    }

    fn game_id(&self) -> Option<&str> {
        match self {
            Self::Game(id) => Some(id.as_str()),
            Self::Artifact(_) => None,
        }
    }

    fn owner_id(&self) -> &str {
        match self {
            Self::Game(id) => id.as_str(),
            Self::Artifact(id) => id.as_str(),
        }
    }

    fn artifact_id(&self) -> Option<&str> {
        match self {
            Self::Game(_) => None,
            Self::Artifact(id) => Some(id.as_str()),
        }
    }
}

/// Facts produced from one held, stable file object.
///
/// A *_observed flag distinguishes an observed absence, such as a DLL with no
/// version resource, from a field not covered by this algorithm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFileObservation {
    /// Namespace owner that prevents game/artifact cache collisions.
    pub owner: ObservationOwner,
    /// Normalized absolute observation path.
    pub normalized_path: PathRef,
    /// Platform strong-identity format.
    pub identity_kind: String,
    /// Platform object identity within its volume.
    pub object_identity: String,
    /// Stable change token for the observed object.
    pub change_token: String,
    /// Observed object byte length.
    pub size: u64,
    /// Revision of the fact-extraction algorithm.
    pub algorithm_revision: u32,
    /// SHA-256 read from the same stable object.
    pub sha256: Sha256Hash,
    /// Whether version extraction was performed.
    pub version_observed: bool,
    /// Observed version, or an observed absence when the flag is true.
    pub version: Option<Version>,
    /// Whether runtime extraction was performed.
    pub runtime_observed: bool,
    /// Serialized observed runtime fact.
    pub runtime_json: Option<String>,
    /// Whether PE compatibility extraction was performed.
    pub pe_observed: bool,
    /// Serialized observed PE compatibility fact.
    pub pe_json: Option<String>,
}

impl StoredFileObservation {
    fn validate(&self) -> AppResult<()> {
        if self.identity_kind.trim().is_empty()
            || self.object_identity.trim().is_empty()
            || self.change_token.trim().is_empty()
        {
            return Err(invalid_row(
                "file observation requires non-empty strong identity and change token",
            ));
        }
        if !self.version_observed && self.version.is_some() {
            return Err(invalid_row(
                "unobserved version fact cannot carry a version value",
            ));
        }
        if !self.runtime_observed && self.runtime_json.is_some() {
            return Err(invalid_row("unobserved runtime fact cannot carry a value"));
        }
        if !self.pe_observed && self.pe_json.is_some() {
            return Err(invalid_row("unobserved PE fact cannot carry a value"));
        }
        for (name, value) in [
            ("runtime", self.runtime_json.as_deref()),
            ("PE", self.pe_json.as_deref()),
        ] {
            if let Some(value) = value {
                let parsed: serde_json::Value = serde_json::from_str(value).map_err(|error| {
                    invalid_row(format!("{name} observation JSON is invalid: {error}"))
                })?;
                if !parsed.is_object() {
                    return Err(invalid_row(format!(
                        "{name} observation JSON must be an object"
                    )));
                }
            }
        }
        Ok(())
    }
}

mod artifacts;
mod catalog;
mod persistence;
#[cfg(feature = "test-instrumentation")]
mod test_support;
#[cfg(test)]
mod tests;

pub(super) use catalog::{
    assert_no_pending_file_mutations_within_transaction,
    invalidate_game_authority_within_transaction, readiness_within_transaction,
    replace_game_observations_within_transaction,
};
use persistence::{
    artifact_observation_from_row, delete_artifact_observations_within_transaction,
    ensure_only_artifact_owner, ensure_only_game_owner, observation_from_row,
    replace_observations_within_transaction,
};
