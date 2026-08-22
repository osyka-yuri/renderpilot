mod artifacts;
mod catalog_select_sql;
/// Projection aliases for row mappers (physical columns live in `schema::physical`).
pub(crate) mod columns;
mod component_backups;
mod components;
mod consolidation;
pub mod game_covers;
mod game_mutations;
pub mod game_ui_state;
mod games;
mod installed_addons;
pub mod nvapi;
mod observations;
mod operations;
mod pending_file_mutations;
mod pending_shared_vulkan_mutations;
mod profile_addon_capabilities;
mod row_mapping;
mod settings;
mod shared_artifacts;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use renderpilot_application::AppResult;
use renderpilot_domain::{GameInstallation, LibraryArtifact, LibraryComponent};
use rusqlite::{Connection, Params, Transaction};

use crate::error::storage_error;

pub use consolidation::{
    ComponentRekey, ConsolidatedScanWriteReport, ConsolidationConflictSummary, ConsolidationPlan,
    ConsolidationReport, ConsolidationSource,
};
pub use game_mutations::{ComponentBaselineMutation, GameMutationCommit, InstalledAddonMutation};
pub use observations::{
    AuthorityCas, CatalogReadiness, CatalogReadyProjection, ObservationOwner, StoredFileObservation,
};
pub use pending_file_mutations::{
    BeginFileMutationPreparation, PendingFileMutationRow, PendingFileMutationState,
    PreparedMutationResolutionFence,
};
pub use pending_shared_vulkan_mutations::{
    BeginSharedVulkanMutation, PendingSharedVulkanMutationRow, PendingSharedVulkanMutationState,
    PreparedSharedVulkanMutationResolutionFence, SharedArtifactMutation,
    SharedVulkanMutationCommit, SharedVulkanMutationReservation, SharedVulkanMutationScope,
};
pub use shared_artifacts::ConditionalSharedArtifactWrite;

/// SQLite-backed storage adapter implementing application repository ports.
#[derive(Debug)]
pub struct SqliteStorage {
    pub(crate) connection: Mutex<Connection>,
    pub(crate) catalog_generation: Arc<AtomicU64>,
}

/// Legacy test payload retained only to exercise atomic row rollback.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub struct ScanWriteUnit<'a> {
    /// Game installation row that anchors the write.
    pub game: &'a GameInstallation,
    /// Full replacement component set for the game.
    pub components: &'a [LibraryComponent],
    /// Artifact rows to upsert while persisting the scan.
    pub artifacts: &'a [LibraryArtifact],
    /// Remove operation headers left without items when a deliberate root
    /// correction prunes components outside the new installation boundary.
    pub prune_empty_operations: bool,
}

/// Complete, CAS-guarded publication of one reconciled installation scan.
///
/// This is the only scan path that may transition catalog authority to
/// Complete. The observation owner is validated as the same game in the
/// transaction, so callers cannot publish detached cache facts.
#[derive(Debug, Clone, Copy)]
pub struct CompleteScanWriteUnit<'a> {
    /// Reconciled installation that owns the resulting projection.
    pub game: &'a GameInstallation,
    /// Full component replacement produced from the stable traversal.
    pub components: &'a [LibraryComponent],
    /// Local artifacts derived from the same stable traversal.
    pub artifacts: &'a [LibraryArtifact],
    /// Same-game observations whose facts produced this component projection.
    pub observations: &'a [StoredFileObservation],
    /// Authority epoch captured before the traversal began.
    pub authority: AuthorityCas,
    /// Whether a proven root correction may prune empty operation headers.
    pub prune_empty_operations: bool,
}

/// Summary of rows written by [`SqliteStorage::save_complete_scan_write_unit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanWriteReport {
    game_rows_written: usize,
    components_written: usize,
    artifacts_written: usize,
}

impl ScanWriteReport {
    /// Number of game rows written.
    pub fn game_rows_written(&self) -> usize {
        self.game_rows_written
    }

    /// Number of component rows written.
    pub fn components_written(&self) -> usize {
        self.components_written
    }

    /// Number of artifact rows written.
    pub fn artifacts_written(&self) -> usize {
        self.artifacts_written
    }
}

impl SqliteStorage {
    /// Returns the actual file backing the live SQLite connection.
    ///
    /// This is connection-derived rather than environment-derived so custom
    /// contexts cannot publish recovery data beside an unrelated catalog.
    pub fn catalog_file_path(&self) -> AppResult<Option<PathBuf>> {
        self.with_connection(crate::schema::backup::main_database_file_path)
    }

    /// Copies a consistent file-backed catalog snapshot to `destination`.
    ///
    /// The connection mutex excludes concurrent adapter writes, and a WAL
    /// checkpoint makes the main database file self-contained before copying.
    /// Returns `false` for in-memory storage.
    pub fn copy_catalog_snapshot_to(&self, destination: &Path) -> AppResult<bool> {
        self.with_connection(|connection| {
            let Some(source) = crate::schema::backup::main_database_file_path(connection)? else {
                return Ok(false);
            };
            crate::schema::backup::checkpoint_wal(connection)?;
            std::fs::copy(&source, destination).map_err(|error| {
                crate::error::storage_context(
                    &format!(
                        "could not copy catalog recovery snapshot to {}",
                        destination.display()
                    ),
                    error,
                )
            })?;
            // Windows requires a write-capable handle for FlushFileBuffers,
            // which is what File::sync_all delegates to. The snapshot has
            // already been copied, so open it read/write without truncation.
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(destination)
                .map_err(|error| {
                    crate::error::storage_context(
                        &format!(
                            "could not reopen catalog recovery snapshot {}",
                            destination.display()
                        ),
                        error,
                    )
                })?;
            file.sync_all().map_err(|error| {
                crate::error::storage_context(
                    &format!(
                        "could not flush catalog recovery snapshot {}",
                        destination.display()
                    ),
                    error,
                )
            })?;
            Ok(true)
        })
    }

    /// Process-local generation of tables that contribute to catalog cards.
    /// Settings and owner-scoped file-observation writes intentionally do not
    /// advance it.
    #[must_use]
    pub fn catalog_generation(&self) -> u64 {
        self.catalog_generation.load(Ordering::Acquire)
    }

    /// Explicitly invalidates the projection for authoritative facts outside
    /// SQLite, such as an atomically activated remote library catalog file.
    pub fn invalidate_catalog_projection(&self) {
        self.catalog_generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Stores a complete scan result atomically in one database transaction.
    ///
    /// The scan result is persisted as one unit:
    ///
    /// - the game row is inserted or updated;
    /// - previous components for the game are replaced;
    /// - discovered artifacts are upserted.
    ///
    /// If any step fails, the whole scan result is rolled back. This prevents
    /// partially updated catalog state after failed scans.
    ///
    /// Row-level helpers in `games`, `components`, and `artifacts` intentionally
    /// accept an existing connection/transaction and must not start their own
    /// transactions. This keeps the scan write as one atomic unit.
    #[cfg(test)]
    pub fn save_scan_result(
        &self,
        game: &GameInstallation,
        components: &[LibraryComponent],
        artifacts: &[LibraryArtifact],
    ) -> AppResult<()> {
        self.save_scan_write_unit(ScanWriteUnit {
            game,
            components,
            artifacts,
            prune_empty_operations: false,
        })?;
        Ok(())
    }

    /// Persists one atomic scan write-unit and returns write counters.
    #[cfg(test)]
    pub fn save_scan_write_unit(&self, unit: ScanWriteUnit<'_>) -> AppResult<ScanWriteReport> {
        self.with_transaction(|transaction| {
            persist_scan_result_in_transaction(
                transaction,
                unit.game,
                unit.components,
                unit.artifacts,
                unit.prune_empty_operations,
            )
        })
    }

    /// Publishes one complete scan behind an authority epoch compare-and-swap.
    ///
    /// The component helper is deliberately private; this transaction is the
    /// scan-side component write authority. Any concurrent invalidation changes
    /// the epoch and rejects publication before it can resurrect stale facts.
    pub fn save_complete_scan_write_unit(
        &self,
        unit: CompleteScanWriteUnit<'_>,
    ) -> AppResult<CatalogReadyProjection> {
        self.with_transaction(|transaction| {
            games::upsert_game_within_transaction(transaction, unit.game)?;
            let current = observations::readiness_within_transaction(transaction, unit.game.id())?;
            if current.authority_epoch() != unit.authority.expected_epoch() {
                return Err(renderpilot_application::AppError::storage_failed(format!(
                    "scan authority changed for {}; expected epoch {}, found {}",
                    unit.game.id().as_str(),
                    unit.authority.expected_epoch(),
                    current.authority_epoch()
                )));
            }
            observations::assert_no_pending_file_mutations_within_transaction(
                transaction,
                unit.game.id(),
            )?;
            let report = persist_scan_result_in_transaction(
                transaction,
                unit.game,
                unit.components,
                unit.artifacts,
                unit.prune_empty_operations,
            )?;
            let _ = report;
            observations::replace_game_observations_within_transaction(
                transaction,
                unit.game.id(),
                unit.observations,
            )?;
            complete_authority_within_transaction(transaction, unit.game.id(), unit.authority)
        })
    }

    /// Complete scan publication with the catalog's proven consolidation plan.
    pub fn save_complete_install_scan_with_consolidation(
        &self,
        unit: CompleteScanWriteUnit<'_>,
        plan: &ConsolidationPlan,
        expected_conflicts: &ConsolidationConflictSummary,
    ) -> AppResult<ConsolidatedScanWriteReport> {
        self.with_transaction(|transaction| {
            transaction
                .pragma_update(None, "defer_foreign_keys", "ON")
                .map_err(storage_error)?;
            games::upsert_game_within_transaction(transaction, unit.game)?;
            let current = observations::readiness_within_transaction(transaction, unit.game.id())?;
            if current.authority_epoch() != unit.authority.expected_epoch() {
                return Err(renderpilot_application::AppError::storage_failed(
                    "scan authority changed before consolidation publication",
                ));
            }
            observations::assert_no_pending_file_mutations_within_transaction(
                transaction,
                unit.game.id(),
            )?;
            consolidation::ensure_conflicts_unchanged(transaction, plan, expected_conflicts)?;
            let scan = persist_scan_result_in_transaction(
                transaction,
                unit.game,
                unit.components,
                unit.artifacts,
                unit.prune_empty_operations,
            )?;
            observations::replace_game_observations_within_transaction(
                transaction,
                unit.game.id(),
                unit.observations,
            )?;
            let consolidation = consolidation::apply(transaction, plan)?;
            consolidation::verify_foreign_keys(transaction)?;
            let _ =
                complete_authority_within_transaction(transaction, unit.game.id(), unit.authority)?;
            Ok(ConsolidatedScanWriteReport {
                scan,
                consolidation,
            })
        })
    }

    /// Persists a full installation scan and a proven legacy-card
    /// consolidation as one SQLite transaction.
    ///
    /// Component identities are explicitly rekeyed before source games are
    /// removed. Every game/component-scoped table is handled by the
    /// consolidation module's schema contract.
    #[cfg(test)]
    pub fn save_install_scan_with_consolidation(
        &self,
        unit: ScanWriteUnit<'_>,
        plan: &ConsolidationPlan,
        expected_conflicts: &ConsolidationConflictSummary,
    ) -> AppResult<ConsolidatedScanWriteReport> {
        self.with_transaction(|transaction| {
            transaction
                .pragma_update(None, "defer_foreign_keys", "ON")
                .map_err(storage_error)?;

            consolidation::ensure_conflicts_unchanged(transaction, plan, expected_conflicts)?;
            let scan = persist_scan_result_in_transaction(
                transaction,
                unit.game,
                unit.components,
                unit.artifacts,
                unit.prune_empty_operations,
            )?;
            let consolidation = consolidation::apply(transaction, plan)?;
            consolidation::verify_foreign_keys(transaction)?;

            Ok(ConsolidatedScanWriteReport {
                scan,
                consolidation,
            })
        })
    }

    /// Read-only preview of destination-key conflicts that would require a
    /// recovery bundle before consolidation.
    pub fn inspect_consolidation_conflicts(
        &self,
        plan: &ConsolidationPlan,
    ) -> AppResult<ConsolidationConflictSummary> {
        self.with_connection(|connection| consolidation::inspect_conflicts(connection, plan))
    }

    /// Absolute files referenced by state that a consolidation may discard.
    pub fn list_consolidation_recovery_file_paths(
        &self,
        plan: &ConsolidationPlan,
    ) -> AppResult<Vec<PathBuf>> {
        self.with_connection(|connection| consolidation::recovery_file_paths(connection, plan))
    }

    /// `rusqlite::query_map` returns `rusqlite::Result<Rows>`, and each row
    /// accessor can fail with either a `rusqlite::Error` (driver) or an
    /// `AppError` (domain validation). The outer Result covers driver errors
    /// during iteration; the inner Result covers per-row domain mapping.
    pub(super) fn query_list<T, P, F>(&self, sql: &str, params: P, map: F) -> AppResult<Vec<T>>
    where
        P: Params,
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<AppResult<T>>,
    {
        self.with_connection(|connection| {
            let mut statement = connection.prepare_cached(sql).map_err(storage_error)?;
            let rows = statement.query_map(params, map).map_err(storage_error)?;

            row_mapping::collect_rows(rows)
        })
    }
}

fn persist_scan_result_in_transaction(
    transaction: &Transaction<'_>,
    game: &GameInstallation,
    components: &[LibraryComponent],
    artifacts: &[LibraryArtifact],
    prune_empty_operations: bool,
) -> AppResult<ScanWriteReport> {
    games::upsert_game_within_transaction(transaction, game)?;
    components::replace_components_for_game_within_transaction(transaction, game.id(), components)?;
    artifacts::upsert_artifacts_within_transaction(transaction, artifacts)?;
    // Drop LocalObserved rows from earlier scans of this game that no longer
    // match on-disk content (e.g. user restored originals outside RenderPilot).
    artifacts::prune_stale_local_observed_for_game_within_transaction(
        transaction,
        game.id(),
        artifacts,
    )?;
    if prune_empty_operations {
        transaction
            .execute(
                "DELETE FROM operations
                 WHERE game_id = ?1
                   AND NOT EXISTS (
                       SELECT 1
                       FROM operation_items
                       WHERE operation_items.operation_id = operations.id
                         AND operation_items.game_id = operations.game_id
                   )",
                [game.id().as_str()],
            )
            .map_err(storage_error)?;
    }

    Ok(ScanWriteReport {
        game_rows_written: 1,
        components_written: components.len(),
        artifacts_written: artifacts.len(),
    })
}

fn complete_authority_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &renderpilot_domain::GameId,
    authority: AuthorityCas,
) -> AppResult<CatalogReadyProjection> {
    let now_ms = crate::sqlite_clock::now_ms(transaction)?;
    let expected_epoch = i64::try_from(authority.expected_epoch())
        .map_err(|_| crate::error::invalid_row("scan authority epoch overflow"))?;
    let updated = transaction
        .execute(
            "UPDATE catalog_scan_authority
             SET readiness = 'complete',
                 authority_epoch = authority_epoch + 1,
                 invalidation_reason = NULL,
                 mutation_token = NULL,
                 completed_at = :completed_at,
                 updated_at = :updated_at
             WHERE game_id = :game_id AND authority_epoch = :expected_epoch",
            rusqlite::named_params! {
                ":game_id": game_id.as_str(),
                ":expected_epoch": expected_epoch,
                ":completed_at": now_ms,
                ":updated_at": now_ms,
            },
        )
        .map_err(crate::error::storage_error)?;
    if updated != 1 {
        return Err(renderpilot_application::AppError::storage_failed(format!(
            "scan authority CAS failed for {}",
            game_id.as_str()
        )));
    }
    match observations::readiness_within_transaction(transaction, game_id)? {
        CatalogReadiness::Complete(ready) => Ok(ready),
        _ => Err(renderpilot_application::AppError::storage_failed(
            "complete scan publication did not produce ready authority",
        )),
    }
}

#[cfg(test)]
mod tests;
