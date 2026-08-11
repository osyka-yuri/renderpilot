use std::{error::Error, fmt};

use renderpilot_application::{AppError, AppErrorKind, invalid_operation_state_display_message};
use renderpilot_detection::LibraryPatternError;

/// Stable reason why a selected directory cannot represent one game install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidInstallRootReason {
    /// A drive, filesystem, or UNC share root was selected.
    FilesystemRoot,
    /// A protected operating-system directory was selected.
    SystemDirectory,
    /// The selected parent contains a launcher- or catalog-proven install.
    ContainsProvenInstall,
}

impl InvalidInstallRootReason {
    /// Stable wire value used by desktop clients for precise remediation.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::FilesystemRoot => "filesystem_root",
            Self::SystemDirectory => "system_directory",
            Self::ContainsProvenInstall => "contains_proven_install",
        }
    }
}

/// Service-layer errors produced by orchestration feature modules.
///
/// These variants cover domain, infrastructure, and runtime failure modes.
/// Presentation concerns (id parsing, output serialisation) belong in the
/// consuming crates (`renderpilot-api` or `renderpilot-cli`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceError {
    /// The requested game was not found in the catalog.
    GameNotFound(String),
    /// The requested operation was not found in the catalog.
    OperationNotFound(String),
    /// The requested artifact was not found in the catalog.
    ArtifactNotFound(String),
    /// The requested component was not found for the given game.
    ComponentNotFound(String),
    /// Caller supplied malformed, incomplete, or inconsistent input.
    InvalidInput(String),
    /// A filesystem or protected system root was selected instead of one game
    /// installation directory.
    InvalidInstallRoot {
        /// Stable machine-readable reason.
        reason: InvalidInstallRootReason,
        /// Internal diagnostic detail, never exposed verbatim in release UI.
        detail: String,
    },
    /// The selected folder contains more than one independent installation.
    MultipleInstallsDetected(String),
    /// The filesystem or catalog facts changed after the caller inspected the
    /// selected installation. No catalog mutation was attempted.
    StaleInstallInspection {
        /// Canonical root that must be inspected again.
        selected_root: String,
        /// Current assessment token retained for diagnostics.
        current_fingerprint: String,
    },
    /// Managed inverse actions must complete before the root can change safely.
    RootCorrectionCleanupRequired {
        /// Existing game whose root would change.
        game_id: String,
        /// Components that still own rollback baselines outside the new root.
        component_ids: Vec<String>,
    },
    /// Managed state without an inline component rollback prevents narrowing.
    RootCorrectionBlocked {
        /// Existing game whose root would change.
        game_id: String,
        /// Stable blocker names used only for diagnostics.
        blockers: Vec<String>,
    },
    /// Managed inverse actions overlap without enough provenance to establish
    /// a lossless order. No inverse action was executed.
    ManagedCleanupAmbiguous {
        /// Game whose managed state remains unchanged.
        game_id: String,
        /// Conflicting action/target descriptions.
        targets: Vec<String>,
        /// Published recovery bundle containing the catalog and related files.
        recovery_bundle_path: String,
    },
    /// Legacy-card consolidation found ambiguous active managed state.
    CatalogConsolidationBlocked {
        /// Scoped tables whose rows cannot be merged losslessly.
        tables: Vec<String>,
        /// Published recovery bundle containing the preflight snapshot.
        recovery_bundle_path: String,
    },
    /// Removing a catalog card could not safely complete one managed inverse
    /// action. The card and remaining recovery metadata stay in the catalog.
    GameRemovalCleanupFailed {
        /// Game that remains in the catalog.
        game_id: String,
        /// Inverse action that failed.
        action: String,
        /// Technical cause retained for diagnostics only.
        reason: String,
    },
    /// Catalog replacement snapshot is missing or no longer matches its hash.
    StaleReplacementSource,
    /// A storage adapter (the catalog database) failed.
    StorageFailed(String),
    /// A game-source or remote provider failed.
    ProviderFailed(String),
    /// A graphics-component detector failed.
    DetectionFailed(String),
    /// A state-bound confirmation token did not match.
    ConfirmationTokenMismatch,
    /// The operation is in an invalid state for the requested action.
    InvalidOperationState {
        /// The identifier of the operation in the invalid state.
        operation_id: String,
        /// The name of the invalid state, e.g. "completed".
        state: String,
    },
    /// A command failed while running.
    CommandFailed(String),
    /// The feature mutation failed and restoring the pre-mutation filesystem
    /// before-state also failed. Both messages are preserved so neither side is
    /// lost in logs or UI.
    RollbackAlsoFailed {
        /// Original feature / work error.
        primary: String,
        /// Error from the durable before-state restore.
        rollback: String,
    },
    /// SteamGridDB API key is required for this cover lookup but is not configured.
    SteamGridDbApiKeyMissing,
    /// Cover bytes are not a supported raster image type.
    UnsupportedCoverImageType,
    /// Cover artwork could not be fetched over the network.
    CoverDownloadFailed(String),
    /// No cover artwork was available from providers.
    CoverNotFound,
    /// Local filesystem error while reading or writing cover files.
    CoverIo(String),
    /// The operating system denied an operation that requires additional access.
    AccessDenied {
        /// The attempted operation.
        operation: String,
        /// Backend-only diagnostic detail from the failing platform boundary.
        detail: String,
    },
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GameNotFound(id) => write!(formatter, "game not found: {id}"),
            Self::OperationNotFound(id) => write!(formatter, "operation not found: {id}"),
            Self::ArtifactNotFound(id) => write!(formatter, "artifact not found: {id}"),
            Self::ComponentNotFound(id) => write!(formatter, "component not found: {id}"),
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::InvalidInstallRoot { reason, detail } => {
                write!(
                    formatter,
                    "invalid game install root ({}): {detail}",
                    reason.code()
                )
            }
            Self::MultipleInstallsDetected(message) => {
                write!(formatter, "multiple game installations detected: {message}")
            }
            Self::StaleInstallInspection {
                selected_root,
                current_fingerprint,
            } => write!(
                formatter,
                "installation inspection for {selected_root} is stale; current fingerprint: {current_fingerprint}"
            ),
            Self::RootCorrectionCleanupRequired {
                game_id,
                component_ids,
            } => write!(
                formatter,
                "root correction for {game_id} requires managed cleanup of components: {}",
                component_ids.join(", ")
            ),
            Self::RootCorrectionBlocked { game_id, blockers } => write!(
                formatter,
                "root correction for {game_id} is blocked by: {}",
                blockers.join(", ")
            ),
            Self::ManagedCleanupAmbiguous {
                game_id,
                targets,
                recovery_bundle_path,
            } => write!(
                formatter,
                "managed cleanup for {game_id} is ambiguous at {}; recovery bundle: {recovery_bundle_path}",
                targets.join(", ")
            ),
            Self::CatalogConsolidationBlocked {
                tables,
                recovery_bundle_path,
            } => write!(
                formatter,
                "catalog consolidation is blocked by ambiguous state in {}; recovery bundle: {recovery_bundle_path}",
                tables.join(", ")
            ),
            Self::GameRemovalCleanupFailed {
                game_id,
                action,
                reason,
            } => write!(
                formatter,
                "cannot remove game {game_id}: {action} failed: {reason}"
            ),
            Self::StaleReplacementSource => formatter
                .write_str("replacement source is missing or was modified outside RenderPilot"),
            Self::StorageFailed(message) => write!(formatter, "storage failed: {message}"),
            Self::ProviderFailed(message) => write!(formatter, "provider failed: {message}"),
            Self::DetectionFailed(message) => write!(formatter, "detection failed: {message}"),
            Self::ConfirmationTokenMismatch => {
                formatter.write_str("confirmation token mismatch for operation")
            }
            Self::InvalidOperationState {
                operation_id,
                state,
            } => formatter.write_str(&invalid_operation_state_display_message(
                operation_id,
                state.as_str(),
            )),
            Self::CommandFailed(message) => formatter.write_str(message),
            Self::RollbackAlsoFailed { primary, rollback } => write!(
                formatter,
                "{primary}; restoring the pre-mutation filesystem state also failed: {rollback}"
            ),
            Self::SteamGridDbApiKeyMissing => {
                formatter.write_str("steamgriddb api key is not configured")
            }
            Self::UnsupportedCoverImageType => formatter.write_str("unsupported cover image type"),
            Self::CoverDownloadFailed(message) => {
                write!(formatter, "cover download failed: {message}")
            }
            Self::CoverNotFound => formatter.write_str("cover artwork was not found"),
            Self::CoverIo(message) => write!(formatter, "cover file error: {message}"),
            Self::AccessDenied { operation, detail } => {
                write!(formatter, "access denied while {operation}: {detail}")
            }
        }
    }
}

impl Error for ServiceError {}

pub(crate) fn failed(message: impl Into<String>) -> ServiceError {
    ServiceError::command_failed(message)
}

impl ServiceError {
    /// Constructs a [`ServiceError::CommandFailed`] from any string-like value.
    /// Feature modules and `addons::errors` route through it so construction
    /// stays in one place.
    #[must_use]
    pub fn command_failed(message: impl Into<String>) -> Self {
        Self::CommandFailed(message.into())
    }

    /// Constructs a [`ServiceError::InvalidInput`] from any string-like value.
    #[must_use]
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }

    /// Constructs a typed invalid-install-root error with stable reason data.
    #[must_use]
    pub fn invalid_install_root(
        reason: InvalidInstallRootReason,
        detail: impl Into<String>,
    ) -> Self {
        Self::InvalidInstallRoot {
            reason,
            detail: detail.into(),
        }
    }

    /// Combines a primary failure with a durable-transaction rollback failure.
    #[must_use]
    pub fn rollback_also_failed(primary: impl Into<String>, rollback: impl Into<String>) -> Self {
        Self::RollbackAlsoFailed {
            primary: primary.into(),
            rollback: rollback.into(),
        }
    }

    /// Returns true when both the feature work and the before-state restore failed.
    #[must_use]
    pub fn is_rollback_also_failed(&self) -> bool {
        matches!(self, Self::RollbackAlsoFailed { .. })
    }
}

impl From<AppError> for ServiceError {
    fn from(error: AppError) -> Self {
        let (kind, message) = error.into_parts();

        // Exhaustive on purpose: every `AppErrorKind` maps to a distinct
        // `ServiceError` so the stable error category survives all the way to the
        // frontend instead of collapsing into a generic `CommandFailed`. Adding a
        // new `AppErrorKind` must force a decision here.
        match kind {
            AppErrorKind::InvalidInput => Self::InvalidInput(message),
            AppErrorKind::StaleReplacementSource => Self::StaleReplacementSource,
            AppErrorKind::StorageFailed => Self::StorageFailed(message),
            AppErrorKind::ProviderFailed => Self::ProviderFailed(message),
            AppErrorKind::DetectionFailed => Self::DetectionFailed(message),
            AppErrorKind::ConfirmationTokenMismatch => Self::ConfirmationTokenMismatch,
            AppErrorKind::GameNotFound => Self::GameNotFound(message),
            AppErrorKind::OperationNotFound => Self::OperationNotFound(message),
            AppErrorKind::ArtifactNotFound => Self::ArtifactNotFound(message),
            AppErrorKind::ComponentNotFound => Self::ComponentNotFound(message),
            AppErrorKind::InvalidOperationState {
                operation_id,
                state,
            } => Self::InvalidOperationState {
                operation_id,
                state: state.as_str().to_owned(),
            },
        }
    }
}

impl From<LibraryPatternError> for ServiceError {
    fn from(error: LibraryPatternError) -> Self {
        Self::CommandFailed(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use renderpilot_application::{AppError, AppErrorKind, OperationStatus};

    use super::{InvalidInstallRootReason, ServiceError};

    #[test]
    fn invalid_install_root_reasons_have_stable_unique_codes() {
        let reasons = [
            InvalidInstallRootReason::FilesystemRoot,
            InvalidInstallRootReason::SystemDirectory,
            InvalidInstallRootReason::ContainsProvenInstall,
        ];
        let codes = reasons.map(InvalidInstallRootReason::code);
        assert_eq!(
            codes,
            [
                "filesystem_root",
                "system_directory",
                "contains_proven_install",
            ]
        );
    }

    #[test]
    fn not_found_variants_are_usage_like() {
        let errors = [
            ServiceError::GameNotFound("g1".to_owned()),
            ServiceError::OperationNotFound("op1".to_owned()),
            ServiceError::ArtifactNotFound("a1".to_owned()),
            ServiceError::ComponentNotFound("c1".to_owned()),
            ServiceError::ConfirmationTokenMismatch,
            ServiceError::InvalidOperationState {
                operation_id: "op".to_owned(),
                state: "planned".to_owned(),
            },
            ServiceError::invalid_install_root(
                InvalidInstallRootReason::FilesystemRoot,
                "filesystem root",
            ),
            ServiceError::MultipleInstallsDetected("two roots".to_owned()),
            ServiceError::StaleInstallInspection {
                selected_root: "C:/Games/Test".to_owned(),
                current_fingerprint: "current".to_owned(),
            },
            ServiceError::RootCorrectionCleanupRequired {
                game_id: "g1".to_owned(),
                component_ids: vec!["c1".to_owned()],
            },
            ServiceError::RootCorrectionBlocked {
                game_id: "g1".to_owned(),
                blockers: vec!["pending_recovery".to_owned()],
            },
            ServiceError::GameRemovalCleanupFailed {
                game_id: "g1".to_owned(),
                action: "component rollback c1".to_owned(),
                reason: "backup is missing".to_owned(),
            },
            ServiceError::ManagedCleanupAmbiguous {
                game_id: "g1".to_owned(),
                targets: vec!["c1 <> addon: file.dll".to_owned()],
                recovery_bundle_path: "recovery/test.bundle".to_owned(),
            },
        ];

        for err in &errors {
            assert!(!err.to_string().is_empty(), "{err:?} has empty display");
        }
    }

    #[test]
    fn runtime_variants_display_correctly() {
        let errors = [
            ServiceError::CommandFailed("scan failed".to_owned()),
            ServiceError::RollbackAlsoFailed {
                primary: "install failed".to_owned(),
                rollback: "restore failed".to_owned(),
            },
            ServiceError::SteamGridDbApiKeyMissing,
            ServiceError::UnsupportedCoverImageType,
            ServiceError::CoverDownloadFailed("timeout".to_owned()),
            ServiceError::CoverNotFound,
            ServiceError::CoverIo("permission denied".to_owned()),
            ServiceError::AccessDenied {
                operation: "updating NVAPI DRS settings".to_owned(),
                detail: "NVAPI reported invalid user privilege".to_owned(),
            },
        ];

        for err in &errors {
            assert!(!err.to_string().is_empty(), "{err:?} has empty display");
        }
    }

    #[test]
    fn app_error_invalid_operation_state_maps_to_service_error() {
        let app_error = AppError::invalid_operation_state("op-123", OperationStatus::Completed);
        assert_matches!(
            app_error.kind(),
            &AppErrorKind::InvalidOperationState { .. }
        );

        assert_eq!(
            ServiceError::from(app_error),
            ServiceError::InvalidOperationState {
                operation_id: "op-123".to_owned(),
                state: "completed".to_owned(),
            }
        );
    }

    #[test]
    fn app_error_invalid_operation_state_preserves_colon_in_operation_id() {
        let app_error = AppError::invalid_operation_state("op:part", OperationStatus::Running);
        assert_eq!(
            ServiceError::from(app_error),
            ServiceError::InvalidOperationState {
                operation_id: "op:part".to_owned(),
                state: "running".to_owned(),
            }
        );
    }

    #[test]
    fn app_error_categories_preserve_their_kind_through_service_error() {
        // Each stable category must survive the conversion with its own variant,
        // not collapse into a generic CommandFailed.
        assert_eq!(
            ServiceError::from(AppError::storage_failed("database locked")),
            ServiceError::StorageFailed("database locked".to_owned()),
        );
        assert_eq!(
            ServiceError::from(AppError::invalid_input("game id is required")),
            ServiceError::InvalidInput("game id is required".to_owned()),
        );
        assert_eq!(
            ServiceError::from(AppError::provider_failed("failed to install file")),
            ServiceError::ProviderFailed("failed to install file".to_owned()),
        );
        assert_eq!(
            ServiceError::from(AppError::detection_failed("could not read PE header")),
            ServiceError::DetectionFailed("could not read PE header".to_owned()),
        );
        assert_eq!(
            ServiceError::from(AppError::stale_replacement_source()),
            ServiceError::StaleReplacementSource,
        );
    }
}
