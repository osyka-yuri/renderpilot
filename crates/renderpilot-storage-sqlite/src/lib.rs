//! SQLite storage adapter for RenderPilot.
//!
//! This crate owns SQLite schema management, connection pragmas, and repository
//! implementations. Domain types remain SQLite-agnostic.

mod connection;
mod error;
mod mapping;
mod repositories;
mod schema;
mod sqlite_clock;

#[cfg(test)]
pub use repositories::ScanWriteUnit;
pub use repositories::game_covers::{DeletedGameInfo, GameCoverRecord};
pub use repositories::game_ui_state::GameUiStateRow;
pub use repositories::{
    AuthorityCas, BeginFileMutationPreparation, BeginSharedVulkanMutation, CatalogReadiness,
    CatalogReadyProjection, ComponentBaselineMutation, ComponentRekey,
    ConditionalSharedArtifactWrite, ConsolidatedScanWriteReport, ConsolidationConflictSummary,
    ConsolidationPlan, ConsolidationReport, ConsolidationSource, GameMutationCommit,
    InstalledAddonMutation, ObservationOwner, PendingFileMutationRow, PendingFileMutationState,
    PendingSharedVulkanMutationRow, PendingSharedVulkanMutationState,
    PreparedMutationResolutionFence, PreparedSharedVulkanMutationResolutionFence,
    SharedArtifactMutation, SharedVulkanMutationCommit, SharedVulkanMutationReservation,
    SharedVulkanMutationScope, StoredFileObservation,
};
pub use repositories::{CompleteScanWriteUnit, ScanWriteReport, SqliteStorage};
#[cfg(feature = "test-instrumentation")]
#[doc(hidden)]
pub use schema::portable_catalog::create_released_portable_catalog_fixture;
pub use schema::portable_catalog::{
    PortableCatalogSchemaError, PortableCatalogSchemaErrorKind, PortableCatalogSchemaReport,
    PortableCatalogSchemaTransition, inspect_portable_catalog_schema,
    transition_portable_catalog_schema,
};
pub use schema::{
    CURRENT_PORTABLE_SCHEMA_VERSION, MINIMUM_PORTABLE_SCHEMA_VERSION,
    PORTABLE_APP_SESSION_PROTOCOL, PORTABLE_RUNTIME_RELEASE_CONTRACT_VERSION,
    PORTABLE_SUPERVISOR_CAPABILITY,
};
