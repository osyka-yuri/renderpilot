use renderpilot_domain::{GameId, OperationId};

use crate::{AppResult, OperationJournalEntry, OperationRecord};

/// Repository port for the **operation journal** aggregate.
///
/// A journal entry is stored as one operation header plus its item rows. This port
/// deliberately exposes only aggregate-safe operations: callers can save or load a
/// complete [`OperationJournalEntry`], but cannot write a header or items separately.
///
/// Low-level persistence helpers such as “upsert header” or “replace items” should
/// stay private to concrete adapters and be used only inside the transaction that
/// implements [`save_operation_entry`](Self::save_operation_entry).
///
/// # Consistency guarantees
///
/// - [`save_operation_entry`](Self::save_operation_entry) must be atomic.
///   On success, the stored header and items exactly match the given entry.
///   On error, no partial change may remain visible.
/// - [`find_operation_entry`](Self::find_operation_entry) returns one logical
///   snapshot of the header and items.
/// - [`list_operation_headers_for_game`](Self::list_operation_headers_for_game)
///   returns headers only.
/// - Saved item rows replace the previous item set completely, including the empty
///   set, which clears all stored items.
/// - Read item order must be stable and match save order where the storage backend
///   supports it. SQLite currently relies on ascending surrogate item IDs; if that
///   invariant stops being reliable, the adapter schema should introduce an explicit
///   `position` column.
///
/// # Errors and return values
///
/// Writes return [`AppResult<()>`] because the canonical record is the input entry
/// and the current model has no server-generated identifiers.
///
/// Error classification belongs to [`crate::AppError`]. Optimistic locking,
/// versioning, and conflict detection are intentionally outside this port until the
/// domain model requires them.
pub trait OperationRepository: Send + Sync {
    /// Inserts or updates one full journal entry in a single transaction.
    ///
    /// Existing item rows for the operation are removed first, then replaced with
    /// `entry.items()` in order. An empty item list clears all persisted items for
    /// the operation.
    fn save_operation_entry(&self, entry: &OperationJournalEntry) -> AppResult<()>;

    /// Loads one full journal entry.
    ///
    /// Returns `None` when no operation header exists for `operation_id`.
    /// Implementations should read the header and items from the same logical
    /// snapshot, for example by using a read transaction.
    fn find_operation_entry(
        &self,
        operation_id: &OperationId,
    ) -> AppResult<Option<OperationJournalEntry>>;

    /// Lists operation headers for a game without loading item rows.
    ///
    /// Results must use a stable order: ascending `created_at`, then ascending
    /// operation id.
    fn list_operation_headers_for_game(&self, game_id: &GameId) -> AppResult<Vec<OperationRecord>>;

    /// Counts persisted item rows for an operation.
    ///
    /// Returns `0` when no item rows exist. This method is intended for summaries
    /// and must not load item payloads.
    fn count_operation_items(&self, operation_id: &OperationId) -> AppResult<usize>;
}
