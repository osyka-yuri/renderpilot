//! Structured, localizable text published by add-on catalogues.

use serde::{Deserialize, Serialize};

use crate::ServiceError;

use super::manifest_validate::ensure_not_blank;

/// A stable localization id paired with the catalogue's reviewed English text.
///
/// The id lets the desktop override a message locally. `fallback_text` remains
/// mandatory so publishing catalogue data never depends on a simultaneous app
/// translation release. Fixtures and sample manifests should use the same ids
/// the live catalogue emits (for RenoDX generics: `renodx.generic.universal`,
/// `renodx.generic.unity`); locales may lag behind and fall back to this text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogMessage {
    /// Stable localization identifier.
    pub id: String,
    /// Reviewed English fallback supplied by the catalogue.
    pub fallback_text: String,
}

impl CatalogMessage {
    /// Creates a structured catalogue message.
    #[must_use]
    pub fn new(id: impl Into<String>, fallback_text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            fallback_text: fallback_text.into(),
        }
    }

    pub(crate) fn validate(&self, field: &str) -> Result<(), ServiceError> {
        ensure_not_blank(&format!("{field}.id"), &self.id)?;
        ensure_not_blank(&format!("{field}.fallback_text"), &self.fallback_text)
    }
}

/// Private v1 wire counterpart. Runtime and DTO code only sees
/// [`CatalogMessage`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireCatalogMessage {
    pub(crate) id: String,
    pub(crate) fallback_text: String,
}

impl From<WireCatalogMessage> for CatalogMessage {
    fn from(value: WireCatalogMessage) -> Self {
        Self::new(value.id, value.fallback_text)
    }
}
