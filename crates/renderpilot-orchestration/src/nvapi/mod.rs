//! NVAPI per-game driver settings: DTOs, registry, operations, and game/exe resolution.

/// Serializable DTOs for NVAPI setting state and metadata.
pub mod dto;
/// Read/write operations for NVAPI driver profile settings.
pub mod ops;
/// NVAPI setting registry backed by the DLSS settings catalog.
pub mod registry;
/// Game/exe resolution helpers for building a [`renderpilot_nvapi::setting::SettingContext`].
pub mod resolve;
