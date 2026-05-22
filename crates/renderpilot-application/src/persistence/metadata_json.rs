use std::{fmt, ops::Deref, str::FromStr};

use crate::{AppError, AppResult};

/// Valid JSON metadata text persisted with operation headers or items.
///
/// The application layer requires the payload to be valid JSON before
/// persisting or rehydrating records. Generic adapters may keep using this as
/// opaque JSON, but metadata that participates in business logic should be
/// wrapped by a typed value object that owns its own validation rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetadataJson(String);

impl MetadataJson {
    /// Creates validated adapter-owned metadata JSON.
    pub fn new(value: impl Into<String>) -> AppResult<Self> {
        let value = value.into();

        serde_json::from_str::<serde_json::Value>(&value).map_err(|error| {
            AppError::invalid_input(format!("metadata json must be valid JSON: {error}"))
        })?;

        Ok(Self(value))
    }

    /// Returns the original JSON text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for MetadataJson {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for MetadataJson {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for MetadataJson {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for MetadataJson {
    type Error = AppError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::{AppErrorKind, MetadataJson};

    #[test]
    fn accepts_valid_json_object() {
        let metadata = MetadataJson::new("{\"driver\":\"531.79\"}").unwrap();

        assert_eq!(metadata.as_str(), "{\"driver\":\"531.79\"}");
        assert_eq!(metadata.to_string(), "{\"driver\":\"531.79\"}");
    }

    #[test]
    fn accepts_valid_json_scalar() {
        let metadata = MetadataJson::new("true").unwrap();

        assert_eq!(metadata.as_str(), "true");
    }

    #[test]
    fn rejects_invalid_json() {
        let error = MetadataJson::new("{").unwrap_err();

        assert_eq!(error.kind(), &AppErrorKind::InvalidInput);
        assert!(error.message().contains("metadata json must be valid JSON"));
    }
}
