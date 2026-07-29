//! Windows platform adapter boundary for RenderPilot.
//!
//! Windows filesystem, registry, executable-format, and launcher adapters live
//! here so higher layers can depend on stable typed results instead of Windows
//! APIs. Format-only adapters remain cross-compilable; adapters that call
//! Windows APIs are explicitly target-gated.

mod developer_mode;
#[cfg(windows)]
pub mod dlss;
mod engine_layout;
pub mod executable_detection;
#[cfg(windows)]
pub mod game_libraries;
mod install_identity;
mod manual_folder;
mod path_normalize;
mod steam_appmanifest;
#[cfg(windows)]
pub mod vulkan_layer;

pub use developer_mode::{DeveloperModeStatus, developer_mode_status};
pub use engine_layout::{
    EngineKind, EngineLayoutDetector, EngineLayoutEvidence, EngineLayoutRequest, EngineLayoutRole,
    analyze_engine_layout,
};
pub use executable_detection::{
    ExecutableCandidate, ExecutableDetectionReport, RejectionReason, detect_executable_candidates,
    inspect_executable_candidates, inspect_executable_candidates_bounded,
    inspect_executable_candidates_complete, is_readable_windows_pe_executable,
};
#[cfg(windows)]
pub use game_libraries::launcher_launch_executable;
pub use install_identity::{InstallIdentityDetails, detect_install_identity};
pub use manual_folder::ManualFolderGameSource;
pub use path_normalize::canonicalize_install_path;
pub use steam_appmanifest::{
    SteamInstallDetails, SteamScanSourceFingerprint, steam_install_details,
    steam_install_dirs_in_steamapps,
};
