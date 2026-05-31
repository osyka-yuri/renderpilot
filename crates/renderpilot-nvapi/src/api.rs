//! Safe Rust wrappers over the raw NVAPI FFI.

use std::{iter, os::raw::c_void, ptr, sync::OnceLock};

use libloading::Library;

use crate::{
    error::{NvapiError, NVAPI_INVALID_USER_PRIVILEGE, NVAPI_SETTING_NOT_FOUND},
    ffi::{
        interface_ids, NvAPI_DRS_CreateSession_fn, NvAPI_DRS_DeleteProfileSetting_fn,
        NvAPI_DRS_DestroySession_fn, NvAPI_DRS_FindApplicationByName_fn,
        NvAPI_DRS_FindProfileByName_fn, NvAPI_DRS_GetProfileInfo_fn, NvAPI_DRS_GetSetting_fn,
        NvAPI_DRS_GetSetting_v2_fn, NvAPI_DRS_LoadSettings_fn, NvAPI_DRS_SaveSettings_fn,
        NvAPI_DRS_SetSetting_fn, NvAPI_DRS_SetSetting_v2_fn, NvAPI_Initialize_fn,
        NvAPI_QueryInterface_fn, NvDRSProfileHandle, NvDRSSessionHandle, NVAPI_UNICODE_STRING_MAX,
        NVDRS_APPLICATION, NVDRS_APPLICATION_VER, NVDRS_DWORD_TYPE, NVDRS_PROFILE,
        NVDRS_PROFILE_VER, NVDRS_SETTING, NVDRS_SETTING_VER,
    },
};

// ── FFI helpers ─────────────────────────────────────────────────────────────

/// Converts a Rust `&str` into a null-terminated UTF-16LE vector.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(iter::once(0)).collect()
}

/// Creates a zeroed C struct and sets its `version` field.
///
/// # Safety
/// The caller must ensure `T` has a `version: u32` field at offset 0.
unsafe fn zeroed_with_version<T>(ver: u32) -> T {
    let mut val: T = std::mem::zeroed();
    // SAFETY: `T` is guaranteed by the caller to have `version` at offset 0.
    ptr::addr_of_mut!(val).cast::<u32>().write(ver);
    val
}

/// Represents an immutable snapshot of a DWORD configuration state associated with a specific profile.
/// This encompasses both the authoritative predefined value configured by the driver and the
/// boolean state denoting equivalence between the current and predefined values.
///
/// Within the NVIDIA Driver Settings (DRS) architecture, profiles persist a dual-state schema:
/// the **current** value (the effective override utilized during execution) alongside the
/// **predefined** value (the factory default inherently baked into the driver for recognized applications).
/// The `get_dword_full` procedure surfaces this complete triad to enable deterministic behaviors:
///   - Identifying external overrides executed independently of the default driver specification.
///   - Facilitating definitive reversion to the manufacturer's baseline via [`Profile::delete_setting`].
///   - Establishing a forensic snapshot prior to the initial configuration write operation orchestrated by RenderPilot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DwordSettingState {
    /// The effective configuration value actively employed by the driver during execution.
    pub current: u32,
    /// The authoritative factory-default value encoded within the driver, populated conditionally when `isPredefinedValid` asserts true.
    pub predefined: Option<u32>,
    /// Yields `true` exclusively when the NVAPI architecture asserts `isCurrentPredefined`, explicitly indicating the absence of any overriding modifications.
    pub is_current_predefined: bool,
}

// ── NVAPI function table ──────────────────────────────────────────────────────

/// NVAPI function table loaded at runtime from `nvapi.dll`.
pub struct Nvapi {
    _library: Library,
    initialize: NvAPI_Initialize_fn,
    create_session: NvAPI_DRS_CreateSession_fn,
    destroy_session: NvAPI_DRS_DestroySession_fn,
    load_settings: NvAPI_DRS_LoadSettings_fn,
    save_settings: NvAPI_DRS_SaveSettings_fn,
    find_application: NvAPI_DRS_FindApplicationByName_fn,
    // Core setting accessors — always loaded; the v2 variants (used by
    // NVIDIA Inspector as primary) are preferred and fall back to v1 on old
    // drivers. Using the wrong generation of IDs causes reads/writes to land
    // in different DRS layers, which is why Inspector and RenderPilot would
    // disagree about the current value.
    get_setting: NvAPI_DRS_GetSetting_fn,
    get_setting_v2: Option<NvAPI_DRS_GetSetting_v2_fn>,
    set_setting: NvAPI_DRS_SetSetting_fn,
    set_setting_v2: Option<NvAPI_DRS_SetSetting_v2_fn>,
    delete_profile_setting: NvAPI_DRS_DeleteProfileSetting_fn,
    delete_profile_setting_v2: Option<NvAPI_DRS_DeleteProfileSetting_fn>,
    // Profile lookup (optional — present on any modern NVIDIA driver but
    // treated as optional so very old systems degrade gracefully). Used to
    // re-resolve an exe's profile by display name, matching Inspector.
    find_profile_by_name: Option<NvAPI_DRS_FindProfileByName_fn>,
    get_profile_info: Option<NvAPI_DRS_GetProfileInfo_fn>,
}

impl Nvapi {
    /// Attempts to load `nvapi.dll` (or `nvapi64.dll`) from the system path
    /// and resolve all required entry points via `nvapi_QueryInterface`.
    /// The loaded library is cached for the lifetime of the process.
    pub fn get() -> Option<&'static Self> {
        static INSTANCE: OnceLock<Option<Nvapi>> = OnceLock::new();
        INSTANCE.get_or_init(Self::load_inner).as_ref()
    }

    fn load_inner() -> Option<Self> {
        let library = unsafe { Library::new("nvapi64.dll") }
            .or_else(|_| unsafe { Library::new("nvapi.dll") })
            .or_else(|_| unsafe { Library::new(r"C:\Windows\System32\nvapi64.dll") })
            .ok()?;

        let query: NvAPI_QueryInterface_fn =
            *unsafe { library.get(b"nvapi_QueryInterface\0") }.ok()?;

        let resolve = |id: u32| -> Option<*const c_void> {
            let ptr = unsafe { (query)(id) };
            if ptr.is_null() {
                None
            } else {
                Some(ptr)
            }
        };

        macro_rules! resolve_fn {
            ($name:ident, $id:expr, $ty:ty) => {
                let $name: $ty = unsafe { std::mem::transmute(resolve($id)?) };
            };
        }

        // Optional functions: resolved with transmute if available, else None.
        macro_rules! resolve_fn_opt {
            ($name:ident, $id:expr, $ty:ty) => {
                let $name: Option<$ty> =
                    resolve($id).map(|ptr| unsafe { std::mem::transmute(ptr) });
            };
        }

        resolve_fn!(initialize, interface_ids::INITIALIZE, NvAPI_Initialize_fn);
        resolve_fn!(
            create_session,
            interface_ids::DRS_CREATE_SESSION,
            NvAPI_DRS_CreateSession_fn
        );
        resolve_fn!(
            destroy_session,
            interface_ids::DRS_DESTROY_SESSION,
            NvAPI_DRS_DestroySession_fn
        );
        resolve_fn!(
            load_settings,
            interface_ids::DRS_LOAD_SETTINGS,
            NvAPI_DRS_LoadSettings_fn
        );
        resolve_fn!(
            save_settings,
            interface_ids::DRS_SAVE_SETTINGS,
            NvAPI_DRS_SaveSettings_fn
        );
        resolve_fn!(
            find_application,
            interface_ids::DRS_FIND_APPLICATION_BY_NAME,
            NvAPI_DRS_FindApplicationByName_fn
        );
        resolve_fn!(
            get_setting,
            interface_ids::DRS_GET_SETTING,
            NvAPI_DRS_GetSetting_fn
        );
        resolve_fn_opt!(
            get_setting_v2,
            interface_ids::DRS_GET_SETTING_V2,
            NvAPI_DRS_GetSetting_v2_fn
        );
        resolve_fn!(
            set_setting,
            interface_ids::DRS_SET_SETTING,
            NvAPI_DRS_SetSetting_fn
        );
        resolve_fn_opt!(
            set_setting_v2,
            interface_ids::DRS_SET_SETTING_V2,
            NvAPI_DRS_SetSetting_v2_fn
        );
        resolve_fn!(
            delete_profile_setting,
            interface_ids::DRS_DELETE_PROFILE_SETTING,
            NvAPI_DRS_DeleteProfileSetting_fn
        );
        resolve_fn_opt!(
            delete_profile_setting_v2,
            interface_ids::DRS_DELETE_PROFILE_SETTING_V2,
            NvAPI_DRS_DeleteProfileSetting_fn
        );

        resolve_fn_opt!(
            find_profile_by_name,
            interface_ids::DRS_FIND_PROFILE_BY_NAME,
            NvAPI_DRS_FindProfileByName_fn
        );
        resolve_fn_opt!(
            get_profile_info,
            interface_ids::DRS_GET_PROFILE_INFO,
            NvAPI_DRS_GetProfileInfo_fn
        );

        Some(Self {
            _library: library,
            initialize,
            create_session,
            destroy_session,
            load_settings,
            save_settings,
            find_application,
            get_setting,
            get_setting_v2,
            set_setting,
            set_setting_v2,
            delete_profile_setting,
            delete_profile_setting_v2,
            find_profile_by_name,
            get_profile_info,
        })
    }

    /// Calls `NvAPI_Initialize`.
    pub fn initialize(&self) -> Result<(), NvapiError> {
        let status = unsafe { (self.initialize)() };
        if status == 0 {
            Ok(())
        } else {
            Err(NvapiError::InitializeFailed(status))
        }
    }

    /// Opens a new DRS session, loads settings, and returns a handle.
    pub fn create_session(&self) -> Result<DrsSession<'_>, NvapiError> {
        let mut handle: NvDRSSessionHandle = ptr::null_mut();
        let status = unsafe { (self.create_session)(&mut handle) };
        if status != 0 {
            return Err(NvapiError::SessionCreateFailed(status));
        }

        let status = unsafe { (self.load_settings)(handle) };
        if status != 0 {
            let _ = unsafe { (self.destroy_session)(handle) };
            return Err(NvapiError::LoadSettingsFailed(status));
        }

        Ok(DrsSession {
            nvapi: self,
            handle,
        })
    }

    fn find_profile_by_exe(
        &self,
        session: NvDRSSessionHandle,
        exe_name: &str,
    ) -> Result<NvDRSProfileHandle, NvapiError> {
        let wide_name = to_wide(exe_name);

        let mut profile: NvDRSProfileHandle = ptr::null_mut();
        let mut app: NVDRS_APPLICATION = unsafe { zeroed_with_version(NVDRS_APPLICATION_VER) };

        let status =
            unsafe { (self.find_application)(session, wide_name.as_ptr(), &mut profile, &mut app) };

        if status != 0 {
            return Err(NvapiError::ApplicationNotFound);
        }

        // NVIDIA Inspector locates profiles exclusively by **display name** via
        // `FindProfileByName` (e.g. "The Last of Us Part I"), not by exe.
        // Writing to a handle obtained from `FindApplicationByName` and writing
        // to one obtained from `FindProfileByName` can land in *different* DRS
        // storage buckets — in particular when a user-level profile with the
        // same name shadows the predefined one.  The fix: once we know which
        // profile owns the exe, re-resolve it through `GetProfileInfo` →
        // `FindProfileByName`.  That is identical to Inspector's lookup path,
        // so both tools always operate on the same handle.
        if let Some(info) = self.get_profile_info_raw(session, profile) {
            if let Some(by_name) = self.find_profile_by_name_raw(session, &info.profileName) {
                return Ok(by_name);
            }
        }

        // Fallback: the profile-name re-lookup failed (very old driver, or
        // optional functions unavailable).  Return the exe-based handle so the
        // operation still proceeds with the best available information.
        Ok(profile)
    }

    fn get_dword_setting_full(
        &self,
        session: NvDRSSessionHandle,
        profile: NvDRSProfileHandle,
        setting_id: u32,
    ) -> Result<DwordSettingState, NvapiError> {
        let mut setting: NVDRS_SETTING = unsafe { zeroed_with_version(NVDRS_SETTING_VER) };

        // Prefer the v2 function ID (0xEA99498D) — the same one NVIDIA
        // Inspector uses. Both IDs expose the same NVAPI function but may
        // operate on different DRS layers; using the wrong one means reads
        // from RenderPilot and writes from Inspector (or vice-versa) don't
        // see each other's changes.
        let status = if let Some(get_v2) = self.get_setting_v2 {
            let mut extra: u32 = 0;
            unsafe { (get_v2)(session, profile, setting_id, &mut setting, &mut extra) }
        } else {
            unsafe { (self.get_setting)(session, profile, setting_id, &mut setting) }
        };

        if status != 0 {
            return Err(NvapiError::GetSettingFailed(status));
        }

        if setting.settingType != NVDRS_DWORD_TYPE {
            return Err(NvapiError::UnexpectedSettingType);
        }

        let current = unsafe { setting.currentValue.u32CurrentValue };
        let predefined = if setting.isPredefinedValid != 0 {
            Some(unsafe { setting.predefinedValue.u32PredefinedValue })
        } else {
            None
        };

        Ok(DwordSettingState {
            current,
            predefined,
            is_current_predefined: setting.isCurrentPredefined != 0,
        })
    }

    fn delete_profile_setting(
        &self,
        session: NvDRSSessionHandle,
        profile: NvDRSProfileHandle,
        setting_id: u32,
    ) -> Result<(), NvapiError> {
        // Prefer the v2 ID (0xD20D29DF) — same signature as legacy, but the
        // same generation as the v2 Get/Set IDs used by NVIDIA Inspector.
        let status = if let Some(del_v2) = self.delete_profile_setting_v2 {
            unsafe { (del_v2)(session, profile, setting_id) }
        } else {
            unsafe { (self.delete_profile_setting)(session, profile, setting_id) }
        };
        if status != 0 {
            if status == NVAPI_INVALID_USER_PRIVILEGE {
                return Err(NvapiError::InvalidUserPrivilege);
            }
            // The setting is already absent — that is the desired post-delete
            // state, so treat it as success.
            if status == NVAPI_SETTING_NOT_FOUND {
                return Ok(());
            }
            return Err(NvapiError::DeleteSettingFailed(status));
        }
        Ok(())
    }

    // ── Optional profile-lookup helpers ─────────────────────────────────────

    /// Reads profile metadata. Returns `None` if the function is unavailable
    /// or the call fails.
    fn get_profile_info_raw(
        &self,
        session: NvDRSSessionHandle,
        profile: NvDRSProfileHandle,
    ) -> Option<NVDRS_PROFILE> {
        let func = self.get_profile_info?;
        let mut info: NVDRS_PROFILE = unsafe { zeroed_with_version(NVDRS_PROFILE_VER) };
        let status = unsafe { (func)(session, profile, &mut info) };
        if status == 0 {
            Some(info)
        } else {
            None
        }
    }

    /// Finds a profile by name. Returns `None` if the function is unavailable
    /// or no profile with that name exists.
    fn find_profile_by_name_raw(
        &self,
        session: NvDRSSessionHandle,
        profile_name: &[u16; NVAPI_UNICODE_STRING_MAX],
    ) -> Option<NvDRSProfileHandle> {
        let func = self.find_profile_by_name?;
        let mut handle: NvDRSProfileHandle = ptr::null_mut();
        let status = unsafe { (func)(session, profile_name.as_ptr(), &mut handle) };
        if status == 0 {
            Some(handle)
        } else {
            None
        }
    }

    fn set_dword_setting(
        &self,
        session: NvDRSSessionHandle,
        profile: NvDRSProfileHandle,
        setting_id: u32,
        value: u32,
    ) -> Result<(), NvapiError> {
        let mut setting: NVDRS_SETTING = unsafe { zeroed_with_version(NVDRS_SETTING_VER) };
        setting.settingId = setting_id;
        setting.settingType = NVDRS_DWORD_TYPE;
        setting.isCurrentPredefined = 0;
        setting.currentValue = crate::ffi::NVDRS_SETTING_CURRENT {
            u32CurrentValue: value,
        };

        // Prefer the v2 function ID (0x8A2CF5F5) — the same one NVIDIA
        // Inspector uses as its primary. The v2 signature takes two extra
        // reserved u32 params (both passed as 0, matching Inspector's usage).
        let status = if let Some(set_v2) = self.set_setting_v2 {
            unsafe { (set_v2)(session, profile, &mut setting, 0, 0) }
        } else {
            unsafe { (self.set_setting)(session, profile, &mut setting) }
        };

        if status != 0 {
            if status == NVAPI_INVALID_USER_PRIVILEGE {
                return Err(NvapiError::InvalidUserPrivilege);
            }
            return Err(NvapiError::SetSettingFailed(status));
        }

        Ok(())
    }
}

/// Active DRS session. Settings are applied when this value is dropped.
pub struct DrsSession<'a> {
    nvapi: &'a Nvapi,
    handle: NvDRSSessionHandle,
}

impl<'a> DrsSession<'a> {
    pub(crate) fn handle(&self) -> NvDRSSessionHandle {
        self.handle
    }

    /// Looks up the profile that owns `exe_name`.
    pub fn find_profile_by_exe(&self, exe_name: &str) -> Result<Profile<'_>, NvapiError> {
        let handle = self.nvapi.find_profile_by_exe(self.handle, exe_name)?;
        Ok(Profile {
            session: self,
            handle,
        })
    }

    /// Saves any pending DRS changes to the driver database.
    pub fn save(&self) -> Result<(), NvapiError> {
        let status = unsafe { (self.nvapi.save_settings)(self.handle) };
        if status != 0 {
            return Err(NvapiError::SaveSettingsFailed(status));
        }
        Ok(())
    }
}

impl Drop for DrsSession<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = (self.nvapi.destroy_session)(self.handle);
        }
    }
}

/// A resolved DRS profile.
pub struct Profile<'a> {
    session: &'a DrsSession<'a>,
    handle: NvDRSProfileHandle,
}

impl Profile<'_> {
    /// Executes a comprehensive read operation to retrieve the definitive state of a DWORD setting,
    /// encapsulating the active value, the optionally available predefined baseline, and the deterministic
    /// equivalency status between the two.
    ///
    /// This represents the primary interrogation mechanism utilized to orchestrate precise "user override vs. driver default"
    /// UI representations or to rigorously evaluate the prerequisite conditions for capturing an initial baseline snapshot.
    pub fn get_dword_full(&self, setting_id: u32) -> Result<DwordSettingState, NvapiError> {
        self.session
            .nvapi
            .get_dword_setting_full(self.session.handle(), self.handle, setting_id)
    }

    /// Writes a DWORD setting to this profile.
    pub fn set_dword(&self, setting_id: u32, value: u32) -> Result<(), NvapiError> {
        self.session
            .nvapi
            .set_dword_setting(self.session.handle(), self.handle, setting_id, value)
    }

    /// Systematically purges a designated setting from the active profile's configuration space.
    ///
    /// Following the execution of a subsequent [`DrsSession::save`] commit, a re-interrogation of the specified setting
    /// will deterministically yield either the authoritative predefined value (assuming the NVIDIA driver maintains one)
    /// or `NVAPI_SETTING_NOT_FOUND`. Consequently, this operation serves as the fundamental primitive for orchestrating
    /// reliable "revert to driver default" workflows within the user interface.
    pub fn delete_setting(&self, setting_id: u32) -> Result<(), NvapiError> {
        self.session
            .nvapi
            .delete_profile_setting(self.session.handle(), self.handle, setting_id)
    }
}
