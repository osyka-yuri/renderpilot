use std::collections::HashMap;

use crate::ServiceError;

use super::super::library_error;
use super::super::types::{
    LibraryLegalDocument, LibraryLegalDocumentFormat, LibraryLegalDocumentKind,
};
use super::fields::{ensure_id, ensure_sha256};

const MAX_LEGAL_DOCUMENT_SIZE: u64 = 16 * 1024 * 1024;
const MAX_LEGAL_DOCUMENT_TITLE_LENGTH: usize = 256;
const MAX_LEGAL_DOCUMENT_FILE_NAME_LENGTH: usize = 128;

pub(super) type LegalDocumentLookup<'a> = HashMap<&'a str, (usize, &'a LibraryLegalDocument)>;

pub(super) fn validate_legal_documents<'a>(
    vendor_id: &str,
    documents: &'a [LibraryLegalDocument],
) -> Result<LegalDocumentLookup<'a>, ServiceError> {
    let mut documents_by_id = HashMap::with_capacity(documents.len());
    let mut previous_id: Option<&str> = None;

    for (index, document) in documents.iter().enumerate() {
        ensure_id("legal document id", &document.legal_document_id)?;
        if previous_id.is_some_and(|previous| previous >= document.legal_document_id.as_str()) {
            return Err(library_error(format!(
                "legal documents for vendor `{vendor_id}` must be sorted by id and unique"
            )));
        }
        previous_id = Some(&document.legal_document_id);

        validate_legal_title(&document.title)?;
        validate_legal_file_name(&document.file_name, document.format)?;
        ensure_sha256("legal document sha256", &document.content.sha256)?;
        validate_legal_content_identity(document)?;

        documents_by_id.insert(document.legal_document_id.as_str(), (index, document));
    }

    Ok(documents_by_id)
}

fn validate_legal_content_identity(document: &LibraryLegalDocument) -> Result<(), ServiceError> {
    if document.content.size_bytes == 0 || document.content.size_bytes > MAX_LEGAL_DOCUMENT_SIZE {
        return Err(library_error(format!(
            "legal document `{}` size is outside 1..={MAX_LEGAL_DOCUMENT_SIZE}",
            document.legal_document_id
        )));
    }

    let kind = match document.kind {
        LibraryLegalDocumentKind::License => "license",
        LibraryLegalDocumentKind::Notice => "notice",
    };
    let expected_id = format!("{kind}.{}", document.content.sha256);
    if document.legal_document_id != expected_id {
        return Err(library_error(format!(
            "legal document id is not content-addressed for `{}`",
            document.legal_document_id
        )));
    }

    let extension = match document.format {
        LibraryLegalDocumentFormat::Text => "txt",
        LibraryLegalDocumentFormat::Pdf => "pdf",
    };
    let expected_key = format!(
        "libraries/legal/sha256/{}.{}",
        document.content.sha256, extension
    );
    if document.object_key != expected_key {
        return Err(library_error(format!(
            "legal document key is not canonical for `{}`",
            document.legal_document_id
        )));
    }

    Ok(())
}

fn validate_legal_title(value: &str) -> Result<(), ServiceError> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().count() > MAX_LEGAL_DOCUMENT_TITLE_LENGTH
        || value.chars().any(char::is_control)
    {
        return Err(library_error(
            "catalog legal document title must be concise, printable, and trimmed",
        ));
    }
    Ok(())
}

fn validate_legal_file_name(
    value: &str,
    format: LibraryLegalDocumentFormat,
) -> Result<(), ServiceError> {
    let lower = value.to_ascii_lowercase();
    if value.is_empty()
        || value.len() > MAX_LEGAL_DOCUMENT_FILE_NAME_LENGTH
        || ![".md", ".pdf", ".txt"]
            .iter()
            .any(|extension| lower.ends_with(extension))
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    {
        return Err(library_error(format!(
            "catalog legal document file name is unsafe: `{value}`"
        )));
    }

    let matches_format = match format {
        LibraryLegalDocumentFormat::Text => lower.ends_with(".md") || lower.ends_with(".txt"),
        LibraryLegalDocumentFormat::Pdf => lower.ends_with(".pdf"),
    };
    if !matches_format {
        return Err(library_error(format!(
            "catalog legal document file name does not match its format: `{value}`"
        )));
    }

    Ok(())
}
