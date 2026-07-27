//! Windows platform adapter boundary for RenderPilot.
//!
//! This crate currently contains only std-based Windows adapter scaffolding.
//! It does not call WinAPI, NVAPI, Restart Manager, or elevation APIs.

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
