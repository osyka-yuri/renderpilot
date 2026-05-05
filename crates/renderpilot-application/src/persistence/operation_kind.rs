use std::{fmt, str::FromStr};

use crate::{AppError, AppResult};

/// Persisted operation kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OperationKind {
    /// Scans one game installation for supported graphics components.
    Scan,

    /// Replaces one detected component with a selected artifact.
    ReplaceComponent,

    /// Restores files from a previously created backup.
    RestoreBackup,

    /// Forward-compatible value for operation kinds introduced by newer adapters.
    Other(String),
}

impl OperationKind {
    /// Stable storage tag for scan operations.
    pub const SCAN: &'static str = "scan";

    /// Stable storage tag for component replacement operations.
    pub const REPLACE_COMPONENT: &'static str = "replace_component";

    /// Stable storage tag for backup restoration operations.
    pub const RESTORE_BACKUP: &'static str = "restore_backup";

    /// Parses a stable storage value into an operation kind.
    pub fn from_storage(value: impl Into<String>) -> AppResult<Self> {
        let value = super::normalize_non_empty_text(value, "operation kind")?;

        match value.as_str() {
            Self::SCAN => Ok(Self::Scan),
            Self::REPLACE_COMPONENT => Ok(Self::ReplaceComponent),
            Self::RESTORE_BACKUP => Ok(Self::RestoreBackup),
            _ => Ok(Self::Other(value)),
        }
    }

    /// Returns the stable storage representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Scan => Self::SCAN,
            Self::ReplaceComponent => Self::REPLACE_COMPONENT,
            Self::RestoreBackup => Self::RESTORE_BACKUP,
            Self::Other(value) => value.as_str(),
        }
    }
}

impl fmt::Display for OperationKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for OperationKind {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_storage(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::{AppErrorKind, OperationKind};

    #[test]
    fn parses_known_operation_kinds() {
        assert_eq!(
            OperationKind::from_storage("scan").unwrap(),
            OperationKind::Scan
        );

        assert_eq!(
            OperationKind::from_storage("replace_component").unwrap(),
            OperationKind::ReplaceComponent
        );

        assert_eq!(
            OperationKind::from_storage("restore_backup").unwrap(),
            OperationKind::RestoreBackup
        );
    }

    #[test]
    fn keeps_unknown_operation_kind_for_forward_compatibility() {
        let kind = OperationKind::from_storage("future_operation").unwrap();

        assert_eq!(kind, OperationKind::Other("future_operation".to_owned()));
        assert_eq!(kind.as_str(), "future_operation");
    }

    #[test]
    fn trims_operation_kind_before_matching_or_storing() {
        assert_eq!(OperationKind::from_storage(" scan ").unwrap(), OperationKind::Scan);

        assert_eq!(
            OperationKind::from_storage(" future_operation ").unwrap(),
            OperationKind::Other("future_operation".to_owned())
        );
    }

    #[test]
    fn rejects_empty_operation_kind() {
        let error = OperationKind::from_storage("   ").unwrap_err();

        assert_eq!(error.kind(), AppErrorKind::InvalidInput);
    }
}
