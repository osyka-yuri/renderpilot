//! Normalized RenoDX catalogue types and the private v1 wire adapter.
//!
//! Runtime code consumes the compact values re-exported here. JSON presentation
//! stays isolated in `wire_v1`, so serde defaults cannot silently become runtime
//! policy.

mod catalog;
mod wire_v1;

pub use crate::addons::matching::{Engine, MatchKind, MatchRule, Status};
pub use crate::addons::reshade::types::{ReshadeChannel, ReshadeChannelParseError};
pub(crate) use catalog::renodx_ini_defaults;
pub use catalog::{
    RenoDxCategory, RenoDxCompatibility, RenoDxGeneric, RenoDxGenericProfile, RenoDxManifest,
    RenoDxTitle,
};
pub(crate) use wire_v1::WireManifestV1;
