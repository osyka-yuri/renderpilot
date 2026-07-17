//! Cross-title uniqueness checks for a Luma manifest.

use std::collections::{HashMap, HashSet};

use crate::ServiceError;

use super::super::errors;
use super::super::types::LumaTitle;

pub(super) fn ensure_unique_guidance_ids(titles: &[LumaTitle]) -> Result<(), ServiceError> {
    let mut ids = HashSet::new();
    for title in titles {
        for guidance in &title.guidance {
            if !ids.insert(&guidance.id) {
                return Err(errors::failed(format!(
                    "Luma guidance id `{}` is duplicated",
                    guidance.id
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn ensure_asset_payload_identity(titles: &[LumaTitle]) -> Result<(), ServiceError> {
    let mut payload_by_asset = HashMap::with_capacity(titles.len());
    for title in titles {
        if let Some(existing) = payload_by_asset.insert(&title.asset, &title.addon_file)
            && existing != &title.addon_file
        {
            return Err(errors::failed(format!(
                "Luma asset `{}` maps to multiple root add-ons (`{existing}` and `{}`)",
                title.asset, title.addon_file
            )));
        }
    }
    Ok(())
}
