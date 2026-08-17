use std::str::FromStr;

use renderpilot_application::AppError;
use renderpilot_domain::GameId;

use super::super::observations::CatalogReadiness;

/// Durable phase of a filesystem transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingFileMutationState {
    /// The row reserves the transaction id; game files have not been touched.
    Preparing,
    /// Before-snapshots exist and must be restored after a crash.
    Prepared,
    /// The feature database commit succeeded; only snapshot cleanup remains.
    Committed,
}

impl PendingFileMutationState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Prepared => "prepared",
            Self::Committed => "committed",
        }
    }
}

impl FromStr for PendingFileMutationState {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "preparing" => Ok(Self::Preparing),
            "prepared" => Ok(Self::Prepared),
            "committed" => Ok(Self::Committed),
            _ => Err(AppError::storage_failed(format!(
                "invalid pending file mutation state `{value}`"
            ))),
        }
    }
}

/// One pending mutation row. The JSON manifest is interpreted by orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFileMutationRow {
    /// Stable transaction id and snapshot-directory name.
    pub id: String,
    /// Game whose paths are protected by this transaction.
    pub game_id: GameId,
    /// Feature label such as `catalog_swap` or `luma_uninstall`.
    pub feature: String,
    /// Optional component or add-on identity.
    pub subject_id: Option<String>,
    /// Current durable transaction phase.
    pub state: PendingFileMutationState,
    /// Serialized before-snapshot manifest.
    pub manifest_json: String,
}

/// Inputs accepted when reserving a durable file-mutation id.
///
/// The durable state is intentionally absent. Storage always writes literal
/// `Preparing`; callers cannot manufacture a `Prepared` row that bypasses the
/// invalidation-before-restore boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginFileMutationPreparation {
    /// Stable transaction id and app-owned snapshot-directory name.
    pub id: String,
    /// Game whose paths are protected by this transaction.
    pub game_id: GameId,
    /// Feature label such as `catalog_swap` or `luma_uninstall`.
    pub feature: String,
    /// Optional component or add-on identity.
    pub subject_id: Option<String>,
    /// Initial JSON-object manifest written before snapshot work starts.
    pub initial_manifest_json: String,
}

/// Opaque proof that a matching Prepared row is bound either to no catalog or
/// to a matching invalidated catalog authority. Only storage can mint this
/// value.
#[derive(Debug)]
pub struct PreparedRestoreFence {
    pub(super) game_id: GameId,
    pub(super) mutation_id: String,
    pub(super) catalog_binding: PreparedRestoreCatalogBinding,
}

/// Total relationship between a durable row and the catalog projection.
///
/// A pre-catalog add-on mutation legitimately has neither a `games` row nor
/// scan authority. Any half-present pair is corruption, not a weaker variant
/// of either state.
#[derive(Debug)]
pub(super) enum CatalogBinding {
    CatalogAbsent,
    CatalogPresent(CatalogReadiness),
}

/// Durable/catalog binding established for one prepared feature commit.
///
/// The catalog-absent variant is intentionally narrow: an empty component
/// replacement is a proven no-op there, so orphan add-on cleanup can remain
/// atomic without manufacturing a game or scan authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::repositories) enum PreparedMutationCommitBinding {
    CatalogAbsent,
    CatalogInvalidated,
}

/// The post-fence catalog state carried privately with a restore capability.
#[derive(Debug)]
pub(super) enum PreparedRestoreCatalogBinding {
    CatalogAbsent,
    CatalogInvalidated { authority_epoch: u64 },
}
