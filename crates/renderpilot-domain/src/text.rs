/// Error returned when required text is empty after trimming whitespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RequiredTextError {
    field: &'static str,
}

impl RequiredTextError {
    pub(crate) fn new(field: &'static str) -> Self {
        Self { field }
    }

    pub(crate) fn field(self) -> &'static str {
        self.field
    }
}

pub(crate) fn normalize_required_text(
    field: &'static str,
    value: impl Into<String>,
) -> Result<String, RequiredTextError> {
    let value = value.into();
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return Err(RequiredTextError::new(field));
    }

    Ok(trimmed.to_owned())
}
