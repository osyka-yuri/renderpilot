//! Raw FFI definitions for NVAPI.
//!
//! All interaction with `nvapi.dll` happens through this module. Function
//! pointers are resolved at runtime so the crate does not require NVIDIA
//! drivers to be present at build time.

#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::os::raw::c_void;

/// Generic NVAPI status code. `0` means success.
pub type NvAPI_Status = i32;

/// Opaque DRS session handle.
pub type NvDRSSessionHandle = *mut c_void;

/// Opaque DRS profile handle.
pub type NvDRSProfileHandle = *mut c_void;

/// Length of `NvAPI_UnicodeString` (wide-char).
///
/// This value (2048) is empirically required by the DRS API for string
/// fields in `NVDRS_APPLICATION` and `NVDRS_SETTING`. It is not exposed
/// in the public NVAPI headers but matches the actual driver allocation.
pub const NVAPI_UNICODE_STRING_MAX: usize = 2048;
/// Maximum binary data length.
pub const NVAPI_BINARY_DATA_MAX: usize = 4096;

/// NVAPI struct version macro: `sizeof(T) | (ver << 16)`.
pub const fn nvapi_version(size: usize, ver: u32) -> u32 {
    (size as u32) | (ver << 16)
}

/// `NVDRS_SETTING_VER`
pub const NVDRS_SETTING_VER: u32 = nvapi_version(std::mem::size_of::<NVDRS_SETTING>(), 1);
/// `NVDRS_APPLICATION_VER`
pub const NVDRS_APPLICATION_VER: u32 = nvapi_version(std::mem::size_of::<NVDRS_APPLICATION>(), 4);

/// Setting type: integer (DWORD).
pub const NVDRS_DWORD_TYPE: u32 = 0;

/// Setting type: string.
pub const NVDRS_WSTRING_TYPE: u32 = 2;

/// DRS profile descriptor (V1).
///
/// `gpuSupport` is a bit-field packed into a `u32`: bit 0 = GeForce,
/// bit 1 = Quadro, bit 2 = NVS. Pass `1` (GeForce) when creating a user
/// profile for gaming applications.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NVDRS_PROFILE {
    pub version: u32,
    pub profileName: [u16; NVAPI_UNICODE_STRING_MAX],
    /// Bit 0 = GeForce, bit 1 = Quadro, bit 2 = NVS.
    pub gpuSupport: u32,
    pub isPredefined: u32,
    pub numOfApps: u32,
    pub numOfSettings: u32,
}

/// `NVDRS_PROFILE_VER`
pub const NVDRS_PROFILE_VER: u32 = nvapi_version(std::mem::size_of::<NVDRS_PROFILE>(), 1);

/// DRS application descriptor (V4). All string fields are wide-char (UTF-16LE).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NVDRS_APPLICATION {
    pub version: u32,
    pub isPredefined: u32,
    pub appName: [u16; NVAPI_UNICODE_STRING_MAX],
    pub userFriendlyName: [u16; NVAPI_UNICODE_STRING_MAX],
    pub launcher: [u16; NVAPI_UNICODE_STRING_MAX],
    pub fileInFolder: [u16; NVAPI_UNICODE_STRING_MAX],
    /// isMetro:1, isCommandLine:1, reserved:30
    pub flags: u32,
    pub commandLine: [u16; NVAPI_UNICODE_STRING_MAX],
}

/// DRS binary setting.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NVDRS_BINARY_SETTING {
    pub valueLength: u32,
    pub valueData: [u8; NVAPI_BINARY_DATA_MAX],
}

/// Anonymous union for predefined value in `NVDRS_SETTING`.
#[repr(C)]
#[derive(Clone, Copy)]
pub union NVDRS_SETTING_PREDEFINED {
    pub u32PredefinedValue: u32,
    pub binaryPredefinedValue: NVDRS_BINARY_SETTING,
    pub wszPredefinedValue: [u16; NVAPI_UNICODE_STRING_MAX],
}

/// Anonymous union for current value in `NVDRS_SETTING`.
#[repr(C)]
#[derive(Clone, Copy)]
pub union NVDRS_SETTING_CURRENT {
    pub u32CurrentValue: u32,
    pub binaryCurrentValue: NVDRS_BINARY_SETTING,
    pub wszCurrentValue: [u16; NVAPI_UNICODE_STRING_MAX],
}

/// DRS setting descriptor (V1).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NVDRS_SETTING {
    pub version: u32,
    pub settingName: [u16; NVAPI_UNICODE_STRING_MAX],
    pub settingId: u32,
    pub settingType: u32,
    pub settingLocation: u32,
    pub isCurrentPredefined: u32,
    pub isPredefinedValid: u32,
    pub predefinedValue: NVDRS_SETTING_PREDEFINED,
    pub currentValue: NVDRS_SETTING_CURRENT,
}

/// `nvapi_QueryInterface` – entry point used to resolve every other function.
pub type NvAPI_QueryInterface_fn = unsafe extern "C" fn(id: u32) -> *const c_void;

/// `NvAPI_Initialize`
pub type NvAPI_Initialize_fn = unsafe extern "C" fn() -> NvAPI_Status;

/// `NvAPI_DRS_CreateSession`
pub type NvAPI_DRS_CreateSession_fn =
    unsafe extern "C" fn(phSession: *mut NvDRSSessionHandle) -> NvAPI_Status;

/// `NvAPI_DRS_DestroySession`
pub type NvAPI_DRS_DestroySession_fn =
    unsafe extern "C" fn(hSession: NvDRSSessionHandle) -> NvAPI_Status;

/// `NvAPI_DRS_LoadSettings`
pub type NvAPI_DRS_LoadSettings_fn =
    unsafe extern "C" fn(hSession: NvDRSSessionHandle) -> NvAPI_Status;

/// `NvAPI_DRS_SaveSettings`
pub type NvAPI_DRS_SaveSettings_fn =
    unsafe extern "C" fn(hSession: NvDRSSessionHandle) -> NvAPI_Status;

/// `NvAPI_DRS_FindProfileByName`
/// `profileName` must be a null-terminated UTF-16LE (wide) string.
pub type NvAPI_DRS_FindProfileByName_fn = unsafe extern "C" fn(
    hSession: NvDRSSessionHandle,
    profileName: *const u16,
    phProfile: *mut NvDRSProfileHandle,
) -> NvAPI_Status;

/// `NvAPI_DRS_GetProfileInfo` — reads profile metadata into `pProfileInfo`.
pub type NvAPI_DRS_GetProfileInfo_fn = unsafe extern "C" fn(
    hSession: NvDRSSessionHandle,
    hProfile: NvDRSProfileHandle,
    pProfileInfo: *mut NVDRS_PROFILE,
) -> NvAPI_Status;

/// `NvAPI_DRS_FindApplicationByName`
/// `appName` must be a null-terminated UTF-16LE (wide) string.
pub type NvAPI_DRS_FindApplicationByName_fn = unsafe extern "C" fn(
    hSession: NvDRSSessionHandle,
    appName: *const u16,
    phProfile: *mut NvDRSProfileHandle,
    pApplication: *mut NVDRS_APPLICATION,
) -> NvAPI_Status;

/// `NvAPI_DRS_GetSetting` (legacy, interface ID `0x73BF8338`).
pub type NvAPI_DRS_GetSetting_fn = unsafe extern "C" fn(
    hSession: NvDRSSessionHandle,
    hProfile: NvDRSProfileHandle,
    settingId: u32,
    pSetting: *mut NVDRS_SETTING,
) -> NvAPI_Status;

/// `NvAPI_DRS_GetSetting` v2 (interface ID `0xEA99498D`).
///
/// Same semantics as the legacy version but takes an extra output `u32`
/// (purpose undocumented; pass a pointer to a zero and ignore the result).
/// NVIDIA Inspector uses this ID as primary with the legacy ID as fallback.
pub type NvAPI_DRS_GetSetting_v2_fn = unsafe extern "C" fn(
    hSession: NvDRSSessionHandle,
    hProfile: NvDRSProfileHandle,
    settingId: u32,
    pSetting: *mut NVDRS_SETTING,
    pExtra: *mut u32,
) -> NvAPI_Status;

/// `NvAPI_DRS_SetSetting` (legacy, interface ID `0x577DD202`).
pub type NvAPI_DRS_SetSetting_fn = unsafe extern "C" fn(
    hSession: NvDRSSessionHandle,
    hProfile: NvDRSProfileHandle,
    pSetting: *mut NVDRS_SETTING,
) -> NvAPI_Status;

/// `NvAPI_DRS_SetSetting` v2 (interface ID `0x8A2CF5F5`).
///
/// Same semantics as the legacy version but takes two extra reserved `u32`
/// parameters (pass both as `0`). NVIDIA Inspector uses this ID as primary
/// with the legacy ID as fallback.
pub type NvAPI_DRS_SetSetting_v2_fn = unsafe extern "C" fn(
    hSession: NvDRSSessionHandle,
    hProfile: NvDRSProfileHandle,
    pSetting: *mut NVDRS_SETTING,
    reserved1: u32,
    reserved2: u32,
) -> NvAPI_Status;

/// `NvAPI_DRS_DeleteProfileSetting`
///
/// Removes a specific setting from a profile. After this call (and a
/// subsequent `Save`), reading the setting via `GetSetting` returns the
/// predefined value (if any) or `NVAPI_SETTING_NOT_FOUND`.
pub type NvAPI_DRS_DeleteProfileSetting_fn = unsafe extern "C" fn(
    hSession: NvDRSSessionHandle,
    hProfile: NvDRSProfileHandle,
    settingId: u32,
) -> NvAPI_Status;

/// Known NVIDIA DRS setting identifiers.
pub mod setting_ids {
    /// DLSS Super Resolution render preset override.
    ///
    /// Value type: DWORD.
    ///   0          = Off / Game Controlled (default)
    ///   1..=15     = Preset A … Preset O
    ///   0x00ffffff = "Latest" (always use the newest preset NVIDIA ships)
    ///
    /// ID sourced from NVIDIA Inspector's `NvApiDriverSettings.cs`:
    ///   `NGX_DLSS_SR_OVERRIDE_RENDER_PRESET_SELECTION_ID = 0x10E41DF3`
    ///
    /// The old `0x10B3292C` value that appeared in some third-party lists is
    /// **incorrect** — it maps to an unrelated driver setting and is ignored by
    /// NVIDIA Inspector.
    pub const DLSS_SR_RENDER_PRESET: u32 = 0x10E41DF3;
}

/// Interface IDs used with `nvapi_QueryInterface`.
///
/// Sourced from the official NVIDIA nvapi repository:
/// https://github.com/NVIDIA/nvapi/blob/main/nvapi_interface.h
pub mod interface_ids {
    pub const INITIALIZE: u32 = 0x0150E828;
    pub const DRS_CREATE_SESSION: u32 = 0x0694D52E;
    pub const DRS_DESTROY_SESSION: u32 = 0xDAD9CFF8;
    pub const DRS_LOAD_SETTINGS: u32 = 0x375DBD6B;
    pub const DRS_SAVE_SETTINGS: u32 = 0xFCBC7E14;
    pub const DRS_FIND_APPLICATION_BY_NAME: u32 = 0xEEE566B2;
    pub const DRS_FIND_PROFILE_BY_NAME: u32 = 0x7E4A9A0B;
    pub const DRS_GET_PROFILE_INFO: u32 = 0x61CD6FD6;
    /// Legacy `NvAPI_DRS_GetSetting` (used as fallback when v2 is unavailable).
    pub const DRS_GET_SETTING: u32 = 0x73BF8338;
    /// Newer `NvAPI_DRS_GetSetting` used by NVIDIA Inspector as the primary ID.
    /// Falls back to `DRS_GET_SETTING` on very old drivers.
    pub const DRS_GET_SETTING_V2: u32 = 0xEA99498D;
    /// Legacy `NvAPI_DRS_SetSetting` (used as fallback when v2 is unavailable).
    pub const DRS_SET_SETTING: u32 = 0x577DD202;
    /// Newer `NvAPI_DRS_SetSetting` used by NVIDIA Inspector as the primary ID.
    /// Falls back to `DRS_SET_SETTING` on very old drivers.
    pub const DRS_SET_SETTING_V2: u32 = 0x8A2CF5F5;
    /// Legacy `NvAPI_DRS_DeleteProfileSetting` (fallback).
    pub const DRS_DELETE_PROFILE_SETTING: u32 = 0xE4A26362;
    /// Newer `NvAPI_DRS_DeleteProfileSetting` (same signature, different ID).
    pub const DRS_DELETE_PROFILE_SETTING_V2: u32 = 0xD20D29DF;
}
