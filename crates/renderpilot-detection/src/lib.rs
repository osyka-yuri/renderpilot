//! Detection pipeline boundary for RenderPilot.
//!
//! The crate contains data-driven library classification and filesystem
//! detection helpers. It does not call platform APIs.

mod error;
mod file_metadata;
mod filesystem_detector;
mod glob;
mod normalize;
mod pattern;
mod pe_version;

pub use error::LibraryPatternError;
pub use file_metadata::{FileCacheKey, FileHashCache, VersionDetectionStatus};
pub use filesystem_detector::{
    DetectedLibraryFile, DetectionConfidence, LibraryPatternComponentDetector,
};
pub use pattern::{
    CandidateFileExtensions, LibraryPattern, LibraryPatternMatch, LibraryPatternSet, PatternKind,
    PatternPlatform,
};
