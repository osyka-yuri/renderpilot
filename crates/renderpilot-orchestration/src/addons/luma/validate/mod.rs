//! Validation for the Luma manifest.
//!
//! Like RenoDX, add-ons are fetched live from upstream, so validation enforces a
//! supported schema, well-formed match rules and risk metadata, and that every
//! curated asset name agrees with Luma's own release-naming convention (and with
//! the title's declared architecture) -- a manifest that passes can be resolved
//! and installed without further structural checks.
//!
//! ## Clusters
//!
//! - [`title`] -- per-title fields (asset, addon_file, features, guidance, ...)
//! - [`external`] -- dgVoodoo / managed external requirements
//! - [`cross_title`] -- uniqueness across the catalogue

mod cross_title;
mod external;
mod title;

#[cfg(test)]
mod tests;

use crate::ServiceError;

use super::types::LumaManifest;
use crate::addons::manifest_validate::{
    ensure_not_blank, ensure_schema_version, ensure_semver, ensure_unique_title_ids,
};

/// Schema version this build understands.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Validates an entire manifest.
pub(super) fn validate_manifest(manifest: &LumaManifest) -> Result<(), ServiceError> {
    ensure_schema_version("Luma", manifest.schema_version, SUPPORTED_SCHEMA_VERSION)?;
    ensure_not_blank("manifest generated_at", &manifest.generated_at)?;

    ensure_semver(
        "manifest",
        "min_reshade_version",
        &manifest.min_reshade_version,
    )?;

    for title in &manifest.titles {
        title::validate_title(title)?;
    }
    ensure_unique_title_ids(manifest.titles.iter().map(|title| title.id.as_str()))?;
    cross_title::ensure_unique_guidance_ids(&manifest.titles)?;
    cross_title::ensure_asset_payload_identity(&manifest.titles)?;

    Ok(())
}
