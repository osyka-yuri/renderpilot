use std::path::PathBuf;

use super::error::CommandError;

pub(super) fn require_non_empty_string(
    name: &'static str,
    value: impl Into<String>,
) -> Result<String, CommandError> {
    let value = value.into().trim().to_owned();

    if value.is_empty() {
        return Err(CommandError::invalid_argument(name, "must not be empty"));
    }

    Ok(value)
}

pub(super) fn require_non_empty_path(path: String) -> Result<PathBuf, CommandError> {
    let path = require_non_empty_string("path", path)?;
    Ok(PathBuf::from(path))
}

pub(super) fn trim_string(value: String) -> String {
    value.trim().to_owned()
}

pub(super) fn trim_string_vec(values: Vec<String>) -> Vec<String> {
    values.into_iter().map(trim_string).collect()
}

pub(super) fn reject_empty_items(
    name: &'static str,
    values: &[String],
) -> Result<(), CommandError> {
    if values.iter().any(|value| value.is_empty()) {
        return Err(CommandError::invalid_argument(
            name,
            "items must not be empty",
        ));
    }

    Ok(())
}
