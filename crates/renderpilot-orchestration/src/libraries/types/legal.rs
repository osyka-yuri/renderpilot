use serde::{Deserialize, Serialize};

use super::LibraryContent;

/// Immutable legal document applicable to one or more catalog packages.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryLegalDocument {
    /// Stable content-addressed document identifier.
    pub legal_document_id: String,
    /// Whether this is the primary license or a supplemental notice.
    pub kind: LibraryLegalDocumentKind,
    /// User-facing upstream document title.
    pub title: String,
    /// Document representation.
    pub format: LibraryLegalDocumentFormat,
    /// Safe suggested file name.
    pub file_name: String,
    /// Raw document content identity.
    pub content: LibraryContent,
    /// CDN-relative, content-addressed object key.
    pub object_key: String,
}

/// Legal document role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryLegalDocumentKind {
    /// Primary license terms for the package.
    License,
    /// Attribution or supplemental third-party terms.
    Notice,
}

/// Legal document representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryLegalDocumentFormat {
    /// UTF-8-compatible text or Markdown served as plain text.
    Text,
    /// Portable Document Format.
    Pdf,
}

/// UI-facing metadata and validated link for a legal document.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryLegalDocumentLink {
    /// Stable document identity.
    pub legal_document_id: String,
    /// License or supplemental notice.
    pub kind: LibraryLegalDocumentKind,
    /// User-facing upstream title.
    pub title: String,
    /// Text or PDF representation.
    pub format: LibraryLegalDocumentFormat,
    /// Suggested file name.
    pub file_name: String,
    /// Public content-addressed CDN URL.
    pub content_url: String,
}
