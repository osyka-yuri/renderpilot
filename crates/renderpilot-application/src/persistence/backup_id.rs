use std::{fmt, str::FromStr};

use crate::{AppError, AppResult};

/// Stable backup identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BackupId(String);

impl BackupId {
    /// Creates a non-empty backup identifier.
    pub fn new(value: impl Into<String>) -> AppResult<Self> {
        let value = super::normalize_non_empty_text(value, "backup id")?;

        Ok(Self(value))
    }

    /// Returns the raw backup identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BackupId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for BackupId {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::{AppErrorKind, BackupId};

    #[test]
    fn creates_trimmed_backup_id() {
        let id = BackupId::new(" backup-1 ").unwrap();

        assert_eq!(id.as_str(), "backup-1");
        assert_eq!(id.to_string(), "backup-1");
    }

    #[test]
    fn rejects_empty_backup_id() {
        let error = BackupId::new("   ").unwrap_err();

        assert_eq!(error.kind(), &AppErrorKind::InvalidInput);
    }

    #[test]
    fn parses_backup_id_from_str() {
        let id = BackupId::from_str("backup-2").unwrap();

        assert_eq!(id.as_str(), "backup-2");
    }
}
