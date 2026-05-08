use std::{fmt, str::FromStr};

use crate::{AppError, AppResult};

/// Persisted operation or operation-item status.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OperationStatus {
    /// The operation has been created but work has not started yet.
    Planned,

    /// The operation is currently in progress.
    Running,

    /// The operation finished successfully.
    Completed,

    /// The operation terminated with an error.
    Failed,

    /// The operation could not proceed because a required precondition failed.
    Blocked,

    /// The operation was successfully rolled back to the original file state.
    RolledBack,

    /// The operation was cancelled before completion.
    Cancelled,

    /// Forward-compatible value for statuses introduced by newer adapters.
    Other(String),
}

impl OperationStatus {
    /// Stable storage tag for planned operations.
    pub const PLANNED: &'static str = "planned";

    /// Stable storage tag for running operations.
    pub const RUNNING: &'static str = "running";

    /// Stable storage tag for successfully completed operations.
    pub const COMPLETED: &'static str = "completed";

    /// Stable storage tag for failed operations.
    pub const FAILED: &'static str = "failed";

    /// Stable storage tag for blocked operations.
    pub const BLOCKED: &'static str = "blocked";

    /// Stable storage tag for rolled-back operations.
    pub const ROLLED_BACK: &'static str = "rolled_back";

    /// Stable storage tag for cancelled operations.
    pub const CANCELLED: &'static str = "cancelled";

    /// Parses a stable storage value into an operation status.
    pub fn from_storage(value: impl Into<String>) -> AppResult<Self> {
        let value = super::normalize_non_empty_text(value, "operation status")?;

        match value.as_str() {
            Self::PLANNED => Ok(Self::Planned),
            Self::RUNNING => Ok(Self::Running),
            Self::COMPLETED => Ok(Self::Completed),
            Self::FAILED => Ok(Self::Failed),
            Self::BLOCKED => Ok(Self::Blocked),
            Self::ROLLED_BACK => Ok(Self::RolledBack),
            Self::CANCELLED => Ok(Self::Cancelled),
            _ => Ok(Self::Other(value)),
        }
    }

    /// Returns the stable storage representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Planned => Self::PLANNED,
            Self::Running => Self::RUNNING,
            Self::Completed => Self::COMPLETED,
            Self::Failed => Self::FAILED,
            Self::Blocked => Self::BLOCKED,
            Self::RolledBack => Self::ROLLED_BACK,
            Self::Cancelled => Self::CANCELLED,
            Self::Other(value) => value.as_str(),
        }
    }
}

impl fmt::Display for OperationStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for OperationStatus {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_storage(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::{AppErrorKind, OperationStatus};

    #[test]
    fn parses_known_operation_statuses() {
        assert_eq!(
            OperationStatus::from_storage("planned").unwrap(),
            OperationStatus::Planned
        );

        assert_eq!(
            OperationStatus::from_storage("running").unwrap(),
            OperationStatus::Running
        );

        assert_eq!(
            OperationStatus::from_storage("completed").unwrap(),
            OperationStatus::Completed
        );

        assert_eq!(
            OperationStatus::from_storage("failed").unwrap(),
            OperationStatus::Failed
        );

        assert_eq!(
            OperationStatus::from_storage("blocked").unwrap(),
            OperationStatus::Blocked
        );

        assert_eq!(
            OperationStatus::from_storage("rolled_back").unwrap(),
            OperationStatus::RolledBack
        );

        assert_eq!(
            OperationStatus::from_storage("cancelled").unwrap(),
            OperationStatus::Cancelled
        );
    }

    #[test]
    fn keeps_unknown_operation_status_for_forward_compatibility() {
        let status = OperationStatus::from_storage("paused").unwrap();

        assert_eq!(status, OperationStatus::Other("paused".to_owned()));
        assert_eq!(status.as_str(), "paused");
    }

    #[test]
    fn trims_operation_status_before_matching_or_storing() {
        assert_eq!(
            OperationStatus::from_storage(" running ").unwrap(),
            OperationStatus::Running
        );

        assert_eq!(
            OperationStatus::from_storage(" paused ").unwrap(),
            OperationStatus::Other("paused".to_owned())
        );
    }

    #[test]
    fn rejects_empty_operation_status() {
        let error = OperationStatus::from_storage("   ").unwrap_err();

        assert_eq!(error.kind(), &AppErrorKind::InvalidInput);
    }
}
