//! Windows platform adapter boundary for RenderPilot.
//!
//! This crate currently contains only std-based Windows adapter scaffolding.
//! It does not call WinAPI, NVAPI, Restart Manager, or elevation APIs.

pub mod game_libraries;
mod manual_folder;
mod steam_appmanifest;

pub use manual_folder::ManualFolderGameSource;
pub use steam_appmanifest::{steam_install_details, SteamInstallDetails};
