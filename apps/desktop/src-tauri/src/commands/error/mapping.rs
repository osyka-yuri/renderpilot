use renderpilot_api::ApiError;
use renderpilot_orchestration::ServiceError;

use super::{CommandError, CommandErrorKind as Kind};

// Keep these matches exhaustive: a new typed backend error must receive a
// deliberate stable desktop code instead of silently becoming command_failed.
impl CommandError {
    pub(crate) fn from_api_error(error: ApiError) -> Self {
        match error {
            ApiError::InvalidGameId(id) => {
                Self::invalid_id(Kind::InvalidGameId, "invalid game id", id)
            }
            ApiError::InvalidComponentId(id) => {
                Self::invalid_id(Kind::InvalidComponentId, "invalid component id", id)
            }
            ApiError::InvalidArtifactId(id) => {
                Self::invalid_id(Kind::InvalidArtifactId, "invalid artifact id", id)
            }
            ApiError::InvalidOperationId(id) => {
                Self::invalid_id(Kind::InvalidOperationId, "invalid operation id", id)
            }
            ApiError::OutputSerializationFailed(message) => Self::with_diagnostic(
                Kind::SerializationFailed,
                format_args!("could not serialize command output: {message}"),
            ),
            ApiError::Service(error) => Self::from_service_error(error),
        }
    }

    pub(crate) fn from_service_error(error: ServiceError) -> Self {
        match error {
            ServiceError::ConfirmationTokenMismatch => Self::new(Kind::ConfirmationTokenMismatch),
            ServiceError::GameNotFound(game_id) => {
                Self::with_diagnostic(Kind::GameNotFound, format_args!("game not found: {game_id}"))
            }
            ServiceError::OperationNotFound(operation_id) => Self::with_diagnostic(
                Kind::OperationNotFound,
                format_args!("operation not found: {operation_id}"),
            ),
            ServiceError::ArtifactNotFound(artifact_id) => Self::with_diagnostic(
                Kind::ArtifactNotFound,
                format_args!("artifact not found: {artifact_id}"),
            ),
            ServiceError::ComponentNotFound(component_id) => Self::with_diagnostic(
                Kind::ComponentNotFound,
                format_args!("component not found: {component_id}"),
            ),
            ServiceError::InvalidOperationState {
                operation_id,
                state,
            } => Self::with_diagnostic(
                Kind::InvalidOperationState,
                format_args!("operation {operation_id} is in invalid state: {state}"),
            ),
            ServiceError::CommandFailed(message) => {
                Self::with_diagnostic(Kind::CommandFailed, message)
            }
            ServiceError::RollbackAlsoFailed { primary, rollback } => Self::with_diagnostic(
                Kind::RollbackAlsoFailed,
                format_args!(
                    "{primary}; restoring the pre-mutation filesystem state also failed: {rollback}"
                ),
            ),
            // `invalid_input` is internal terminology. `invalid_argument` is the
            // intentionally stable desktop boundary code.
            ServiceError::InvalidInput(message) => {
                Self::with_diagnostic(Kind::InvalidArgument, message)
            }
            ServiceError::InvalidInstallRoot { reason, detail } => {
                Self::with_diagnostic(Kind::InvalidInstallRoot, detail)
                    .with_reason_code(reason.code())
            }
            ServiceError::MultipleInstallsDetected(message) => {
                Self::with_diagnostic(Kind::MultipleInstallsDetected, message)
            }
            ServiceError::StaleInstallInspection {
                selected_root,
                current_fingerprint,
            } => Self::with_diagnostic(
                Kind::StaleInstallInspection,
                format_args!(
                    "installation inspection for {selected_root} is stale; current fingerprint: {current_fingerprint}"
                ),
            ),
            ServiceError::RootCorrectionCleanupRequired {
                game_id,
                component_ids,
            } => Self::with_diagnostic(
                Kind::RootCorrectionCleanupRequired,
                format_args!(
                    "root correction for {game_id} requires managed cleanup of components: {}",
                    component_ids.join(", ")
                ),
            ),
            ServiceError::RootCorrectionBlocked { game_id, blockers } => Self::with_diagnostic(
                Kind::RootCorrectionBlocked,
                format_args!(
                    "root correction for {game_id} is blocked by: {}",
                    blockers.join(", ")
                ),
            ),
            ServiceError::ManagedCleanupAmbiguous {
                game_id,
                targets,
                recovery_bundle_path,
            } => Self::with_diagnostic(
                Kind::ManagedCleanupAmbiguous,
                format_args!(
                    "managed cleanup for {game_id} is ambiguous at {}; recovery bundle: {recovery_bundle_path}",
                    targets.join(", ")
                ),
            )
            .with_recovery_bundle_path(recovery_bundle_path),
            ServiceError::CatalogConsolidationBlocked {
                tables,
                recovery_bundle_path,
            } => Self::with_diagnostic(
                Kind::CatalogConsolidationBlocked,
                format_args!(
                    "catalog consolidation is blocked by ambiguous state in {}; recovery bundle: {recovery_bundle_path}",
                    tables.join(", ")
                ),
            )
            .with_recovery_bundle_path(recovery_bundle_path),
            ServiceError::GameRemovalCleanupFailed {
                game_id,
                action,
                reason,
            } => Self::with_diagnostic(
                Kind::GameRemovalCleanupFailed,
                format_args!("removing game {game_id} could not complete {action}: {reason}"),
            ),
            ServiceError::StaleReplacementSource => Self::new(Kind::StaleReplacementSource),
            ServiceError::StorageFailed(message) => {
                Self::with_diagnostic(Kind::StorageFailed, message)
            }
            ServiceError::ProviderFailed(message) => {
                Self::with_diagnostic(Kind::ProviderFailed, message)
            }
            ServiceError::DetectionFailed(message) => {
                Self::with_diagnostic(Kind::DetectionFailed, message)
            }
            ServiceError::SteamGridDbApiKeyMissing => Self::new(Kind::SteamGridDbApiKeyMissing),
            ServiceError::UnsupportedCoverImageType => Self::new(Kind::UnsupportedCoverImageType),
            ServiceError::CoverDownloadFailed(message) => {
                Self::with_diagnostic(Kind::CoverDownloadFailed, message)
            }
            ServiceError::CoverNotFound => Self::new(Kind::CoverNotFound),
            ServiceError::CoverIo(message) => Self::with_diagnostic(Kind::CoverIoError, message),
            ServiceError::AccessDenied { operation, detail } => Self::with_diagnostic(
                Kind::AccessDenied,
                format_args!("access denied while {operation}: {detail}"),
            ),
        }
    }
}

impl From<ApiError> for CommandError {
    fn from(error: ApiError) -> Self {
        Self::from_api_error(error)
    }
}
