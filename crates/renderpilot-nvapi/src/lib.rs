//! NVAPI FFI bindings for RenderPilot.
//!
//! Provides safe Rust wrappers around the NVIDIA NVAPI driver-settings
//! interface. All interaction with `nvapi.dll` is deferred to runtime so
//! the library gracefully degrades on non-NVIDIA systems.

mod api;
mod error;
mod ffi;
pub mod setting;

pub use api::{
    ApplicationIdentity, DrsSession, DwordSettingState, Nvapi, Profile, ProfileIdentity,
    SettingIdentity,
};
pub use error::{
    NVAPI_EXECUTABLE_AMBIGUOUS, NVAPI_EXECUTABLE_NOT_FOUND, NVAPI_SETTING_NOT_FOUND, NvapiError,
};
pub use setting::{
    BaselineSnapshot, CatalogReadiness, DllInfo, DlssDllKind, DlssVersion, NvapiSetting,
    NvapiValueOption, NvapiValueType, SettingContext, SettingState,
};
