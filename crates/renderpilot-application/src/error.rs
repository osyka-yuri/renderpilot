use std::{error::Error, fmt};

/// Result type used by application use cases and ports.
pub type AppResult<T> = Result<T, AppError>;

/// Application-layer error with a stable category and a human-readable message.
///
/// `AppErrorKind` is intended for branching, metrics, logging and diagnostics.
/// `message` is intended for humans and should not be parsed programmatically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    kind: AppErrorKind,
    message: String,
}

impl AppError {
    /// Creates an application error.
    #[must_use]
    pub fn new(kind: AppErrorKind, message: impl Into<String>) -> Self {
        let message = message.into();

        debug_assert!(
            !message.trim().is_empty(),
            "application error message should not be empty"
        );

        Self { kind, message }
    }

    /// Creates an invalid-input error.
    #[must_use]
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(AppErrorKind::InvalidInput, message)
    }

    /// Creates a provider error.
    #[must_use]
    pub fn provider_failed(message: impl Into<String>) -> Self {
        Self::new(AppErrorKind::ProviderFailed, message)
    }

    /// Creates a detection error.
    #[must_use]
    pub fn detection_failed(message: impl Into<String>) -> Self {
        Self::new(AppErrorKind::DetectionFailed, message)
    }

    /// Creates a storage error.
    #[must_use]
    pub fn storage_failed(message: impl Into<String>) -> Self {
        Self::new(AppErrorKind::StorageFailed, message)
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn kind(&self) -> AppErrorKind {
        self.kind
    }

    /// Returns the human-readable error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns `true` when this error has the given category.
    #[must_use]
    pub fn is(&self, kind: AppErrorKind) -> bool {
        self.kind == kind
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl Error for AppError {}

/// Stable category for application-layer errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppErrorKind {
    /// Caller supplied malformed, incomplete or inconsistent input.
    InvalidInput,

    /// A game source provider failed.
    ProviderFailed,

    /// A graphics component detector failed.
    DetectionFailed,

    /// A storage adapter failed.
    StorageFailed,
}

impl AppErrorKind {
    /// Returns the stable string representation of this error category.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid input",
            Self::ProviderFailed => "provider failed",
            Self::DetectionFailed => "detection failed",
            Self::StorageFailed => "storage failed",
        }
    }
}

impl fmt::Display for AppErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{AppError, AppErrorKind};

    #[test]
    fn app_error_preserves_kind_and_message() {
        let error = AppError::invalid_input("game id is required");

        assert_eq!(error.kind(), AppErrorKind::InvalidInput);
        assert_eq!(error.message(), "game id is required");
        assert_eq!(error.to_string(), "invalid input: game id is required");
    }

    #[test]
    fn app_error_checks_kind() {
        let error = AppError::storage_failed("catalog database is unavailable");

        assert!(error.is(AppErrorKind::StorageFailed));
        assert!(!error.is(AppErrorKind::ProviderFailed));
    }

    #[test]
    fn app_error_kind_has_stable_display_text() {
        assert_eq!(AppErrorKind::InvalidInput.as_str(), "invalid input");
        assert_eq!(AppErrorKind::ProviderFailed.as_str(), "provider failed");
        assert_eq!(AppErrorKind::DetectionFailed.as_str(), "detection failed");
        assert_eq!(AppErrorKind::StorageFailed.as_str(), "storage failed");

        assert_eq!(
            AppErrorKind::DetectionFailed.to_string(),
            "detection failed"
        );
    }
}
