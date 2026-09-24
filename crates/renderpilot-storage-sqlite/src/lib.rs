//! SQLite storage adapter for RenderPilot.
//!
//! This crate owns SQLite schema management, connection pragmas, and repository
//! implementations. Domain types remain SQLite-agnostic.

mod connection;
mod error;
mod mapping;
mod peer_runtime;
mod repositories;
mod schema;
mod sqlite_clock;

pub use peer_runtime::{
    AggregateAfter, AggregateBefore, AggregateGeneration, CommittedAggregate,
    CommittedMetadataAggregate, CommittedOptiScalerJournalAggregate, GameAggregateMutation,
    MetadataAggregatePreparation, MetadataAggregateTransition, OptiScalerJournalAggregateBegin,
    OptiScalerJournalAggregateCommit, PeerRecoveryAncestor, PeerRecoveryEndpoint,
    PeerRecoveryExecutionClass, PeerRecoveryImage, PeerRecoveryProgram,
    PendingFileMutationRecoveryCandidate, PlannedAggregateAfter,
    PreparedMetadataAggregateCommitPermit, PreparedOptiScalerJournalAggregate,
    PreparingOptiScalerJournalAggregate, RecoveringOptiScalerJournalAggregate,
    validate_file_peer_program_manifest, validate_file_peer_program_manifest_with_renodx_dlss,
    validate_file_peer_program_manifest_with_renodx_optiscaler_config,
    validate_file_peer_program_manifest_with_renodx_reshade_ini, validate_peer_program_manifest,
    validate_shared_peer_program_manifest, validate_shared_peer_recovery_program,
    validated_file_peer_recovery_program, validated_file_peer_recovery_program_for_feature,
};
#[cfg(test)]
pub use repositories::ScanWriteUnit;
pub use repositories::game_covers::{DeletedGameInfo, GameCoverRecord};
pub use repositories::game_ui_state::GameUiStateRow;
pub use repositories::nvapi::{
    NvapiGameSettingPreparation, NvapiGlobalSettingPreparation, NvapiOwnedProfileRow,
    NvapiPendingOperationRow, NvapiProfileCreationCompletion, NvapiProfileMoveCompletion,
    NvapiSettingOperationCompletion, NvapiSettingOperationScope, NvapiSettingState,
    NvapiTargetSettingClaimRow, NvapiVerifiedProfileReceipt,
};
pub use repositories::{
    AuthorityCas, BeginFileMutationPreparation, BeginSharedVulkanMutation, CatalogReadiness,
    CatalogReadyProjection, ComponentBaselineMutation, ComponentRekey,
    ConditionalSharedArtifactWrite, ConsolidatedScanWriteReport, ConsolidationConflictSummary,
    ConsolidationPlan, ConsolidationReport, ConsolidationSource, GameMutationCommit,
    InstalledAddonMutation, ObservationOwner, OptiScalerAggregateMutation,
    OptiScalerAuxiliaryPreservation, OptiScalerPeerMutation, OptiScalerRetainedClaim,
    PendingFileMutationRow, PendingFileMutationState, PendingSharedVulkanMutationRow,
    PendingSharedVulkanMutationState, PreparedMutationResolutionFence,
    PreparedSharedVulkanMutationResolutionFence, SharedArtifactMutation,
    SharedVulkanMutationCommit, SharedVulkanMutationReservation, SharedVulkanMutationScope,
    StoredFileObservation,
};
pub use repositories::{CompleteScanWriteUnit, ScanWriteReport, SqliteStorage};
pub use repositories::{
    PeerCommitPreparation, PeerStorageRuntime, PreparedPeerCommitPermit,
    SharedPeerCommitPreparation,
};
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
