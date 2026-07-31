//! Detection pipeline boundary for RenderPilot.
//!
//! The crate contains data-driven library classification and filesystem
//! detection helpers. It does not call platform APIs.

mod anticheat;
mod dlss_binary;
mod error;
mod file_metadata;
mod filesystem_detector;
mod glob;
mod normalize;
mod pattern;
mod pe;

pub use anticheat::{
    AntiCheatEngine, AntiCheatEvidence, AntiCheatEvidenceKind, AntiCheatScanOptions,
    AntiCheatScanReport, DEFAULT_ANTICHEAT_MAX_SCANNED_ENTRIES, scan_anticheat,
    scan_anticheat_with_options,
};
pub use dlss_binary::{DlssBinaryError, DlssBinaryInfo, NVNGX_DLSS_FILE_NAME};
pub use error::LibraryPatternError;
pub use file_metadata::{
    FileCacheKey, FileHashCache, VersionDetectionStatus, sha256_bytes, sha256_file,
};
pub use filesystem_detector::{
    DetectedLibraryFile, DetectionConfidence, InstallTreeCompleteness, InstallTreeReport,
    InstallTreeWalker, InstallWalkMode, LibraryPatternComponentDetector, WalkDiagnostic,
    WalkDiagnosticKind, group_into_artifacts, group_into_components,
};
pub use pattern::{
    CandidateFileExtensions, LibraryPattern, LibraryPatternMatch, LibraryPatternSet, PatternKind,
    PatternPlatform,
};
pub use pe::{
    PeExportedU32, PeImportError, PeInspection, VersionIdentityStrings, analyze_executable,
    analyze_executable_bytes, inspect_pe, inspect_pe_bytes, parse_windows_version_text,
    pe_exported_u32_from_bytes, read_pe_architecture_from_bytes, read_pe_export_names,
    read_pe_exported_u32, read_windows_file_version, read_windows_version_strings,
    replace_pe_exported_u32_in_bytes,
};
