//! Structural, referential, and content-integrity validation for catalog v1.

mod artifact;
mod catalog;
mod fields;
mod legal;
mod package;
mod xiph;

pub(super) use artifact::{validate_dll_hash, validate_exact_document, validate_transport};
pub(super) use catalog::{
    MAX_INDEX_SIZE, is_supported_vendor, validate_catalog, validate_index,
    validate_vendor_snapshot_envelope,
};
