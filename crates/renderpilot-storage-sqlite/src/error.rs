use renderpilot_application::AppError;

pub(crate) fn storage_error(error: impl std::fmt::Display) -> AppError {
    AppError::storage_failed(error.to_string())
}

pub(crate) fn storage_context(context: &str, error: impl std::fmt::Display) -> AppError {
    AppError::storage_failed(format!("{context}: {error}"))
}

pub(crate) fn invalid_row(error: impl std::fmt::Display) -> AppError {
    AppError::storage_failed(format!("invalid sqlite row: {error}"))
}

#[cfg(test)]
mod tests {
    use renderpilot_application::AppErrorKind;

    use super::{invalid_row, storage_context, storage_error};

    #[test]
    fn storage_error_preserves_message() {
        let error = storage_error("sqlite busy");

        assert_eq!(error.kind(), AppErrorKind::StorageFailed);
        assert_eq!(error.message(), "sqlite busy");
    }

    #[test]
    fn storage_context_prefixes_message() {
        let error = storage_context("failed to read setting", "database locked");

        assert_eq!(error.message(), "failed to read setting: database locked");
    }

    #[test]
    fn invalid_row_uses_stable_prefix() {
        let error = invalid_row("metadata json must be valid JSON");

        assert_eq!(error.message(), "invalid sqlite row: metadata json must be valid JSON");
    }
}