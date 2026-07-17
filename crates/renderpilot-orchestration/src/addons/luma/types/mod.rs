//! Runtime types for the Luma Framework v1 manifest.
//!
//! Luma Framework (Filoppi) is a DirectX 11 ReShade add-on distributed as
//! per-game ZIP archives on GitHub Releases — there is no upstream exe/appid
//! mapping, so the catalogue here is hand-curated (dedicated per-game profiles
//! plus wiki-listed Generic-Mod games). The manifest is deliberately narrower
//! than RenoDX's: every asset is a GitHub Release file (no `external`/`native_hdr`
//! categories), and the host is **always nightly**.
//!
//! The tool-agnostic match and ReShade-host vocabularies are shared and
//! re-exported below, exactly as in [`crate::addons::renodx::types`]; this module
//! owns the Luma-shaped catalogue model (manifest, title, category) plus the
//! managed dependency model.

mod catalog;
mod managed;
mod wire_v1;

pub use crate::addons::matching::{MatchKind, MatchRule, Status};
#[cfg(test)]
pub(crate) use catalog::GENERIC_UNREAL_ASSET;
pub(crate) use catalog::is_generic_unreal_asset;
pub use catalog::{
    LumaCategory, LumaEngine, LumaFeatureStatus, LumaFeatures, LumaGuidance, LumaGuidanceKind,
    LumaManifest, LumaProfile, LumaTitle,
};
pub use managed::{
    ExternalConfigEntry, ExternalConfigSection, LumaExternalRequirement, ManagedArchiveSource,
    ManagedInstallMapEntry,
};
pub(crate) use wire_v1::WireManifestV1;
