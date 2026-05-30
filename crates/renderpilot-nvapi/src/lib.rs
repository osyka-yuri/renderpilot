//! NVAPI FFI bindings for RenderPilot.
//!
//! Provides safe Rust wrappers around the NVIDIA NVAPI driver-settings
//! interface. All interaction with `nvapi.dll` is deferred to runtime so
//! the library gracefully degrades on non-NVIDIA systems.

mod api;
pub mod dlss;
mod error;
mod ffi;
pub mod setting;

pub use api::{DrsSession, DwordSettingState, Nvapi, Profile};
pub use dlss::{DlssRenderPreset, DlssRenderPresetSetting};
pub use error::{NvapiError, NVAPI_SETTING_NOT_FOUND};
pub use setting::{
    BaselineSnapshot, DllInfo, DlssDllKind, DlssVersion, NvapiSetting, NvapiValueOption,
    NvapiValueType, SettingContext, SettingState,
};
