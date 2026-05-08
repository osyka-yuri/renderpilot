use std::{error::Error, fmt};

use renderpilot_application::AppError;

use crate::{PatternKind, PatternPlatform};

/// Error returned when library pattern loading fails.
#[derive(Debug)]
pub enum LibraryPatternError {
    /// Pattern JSON is invalid.
    Json(serde_json::Error),
    /// Pattern file could not be read.
    Io(std::io::Error),
    /// A pattern is empty after normalization.
    EmptyPattern,
    /// A pattern is declared more than once for the same platform and kind.
    DuplicatePattern {
        /// Normalized duplicate pattern text.
        pattern: String,
        /// Platform where the duplicate was found.
        platform: PatternPlatform,
        /// Matching strategy where the duplicate was found.
        kind: PatternKind,
    },
}

impl fmt::Display for LibraryPatternError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid library pattern JSON: {error}"),
            Self::Io(error) => write!(formatter, "could not read library pattern file: {error}"),
            Self::EmptyPattern => formatter.write_str("library pattern cannot be empty"),
            Self::DuplicatePattern {
                pattern,
                platform,
                kind,
            } => write!(
                formatter,
                "duplicate library pattern `{pattern}` for {platform:?}/{kind:?}"
            ),
        }
    }
}

impl Error for LibraryPatternError {}

pub(crate) fn detection_error(message: impl fmt::Display) -> AppError {
    AppError::detection_failed(message.to_string())
}

pub(crate) fn detection_context_error(
    context: impl fmt::Display,
    error: impl fmt::Display,
) -> AppError {
    detection_error(format_args!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use renderpilot_application::AppErrorKind;

    use super::{detection_context_error, detection_error};

    #[test]
    fn detection_error_uses_detection_failed_kind() {
        let error = detection_error("scan failed");

        assert_eq!(error.kind(), &AppErrorKind::DetectionFailed);
        assert_eq!(error.message(), "scan failed");
    }

    #[test]
    fn detection_context_error_prefixes_message() {
        let error = detection_context_error("could not read metadata", "access denied");

        assert_eq!(error.message(), "could not read metadata: access denied");
    }
}
