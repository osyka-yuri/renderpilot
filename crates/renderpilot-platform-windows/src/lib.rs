//! Windows platform adapter boundary for RenderPilot.
//!
//! This crate currently contains only std-based Windows adapter scaffolding.
//! It does not call WinAPI, NVAPI, Restart Manager, or elevation APIs.

#[cfg(windows)]
pub mod dlss;
#[cfg(windows)]
pub mod executable_detection;
pub(crate) mod fs_walk;
#[cfg(windows)]
pub mod game_libraries;
mod manual_folder;
mod steam_appmanifest;

#[cfg(windows)]
pub use executable_detection::{
    detect_executable_candidates, ExecutableCandidate, RejectionReason,
};
pub use manual_folder::ManualFolderGameSource;
pub use steam_appmanifest::{
    steam_install_details, steam_install_dirs_in_steamapps, SteamInstallDetails,
};
