//! Windows platform adapter boundary for RenderPilot.
//!
//! Platform-specific filesystem, registry, and executable adapters live here
//! so higher layers can depend on stable typed results instead of Windows APIs.

mod developer_mode;
#[cfg(windows)]
pub mod dlss;
#[cfg(windows)]
pub mod executable_detection;
#[cfg(windows)]
pub(crate) mod fs_walk;
#[cfg(windows)]
pub mod game_libraries;
mod install_identity;
mod manual_folder;
mod path_normalize;
mod steam_appmanifest;
#[cfg(windows)]
pub mod vulkan_layer;

pub use developer_mode::{DeveloperModeStatus, developer_mode_status};
#[cfg(windows)]
pub use executable_detection::{
    ExecutableCandidate, RejectionReason, detect_executable_candidates,
};
#[cfg(windows)]
pub use game_libraries::launcher_launch_executable;
pub use install_identity::{InstallIdentityDetails, detect_install_identity};
pub use manual_folder::ManualFolderGameSource;
pub use steam_appmanifest::{
    SteamInstallDetails, SteamScanSourceFingerprint, steam_install_details,
    steam_install_dirs_in_steamapps,
};
