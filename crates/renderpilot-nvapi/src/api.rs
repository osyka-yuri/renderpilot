//! Safe Rust wrappers over the raw NVAPI FFI.
#![expect(
    unsafe_code,
    reason = "loads nvapi.dll and calls driver entry points; each block documents SAFETY invariants"
)]

use std::{iter, mem::MaybeUninit, os::raw::c_void, ptr, sync::OnceLock};

use libloading::Library;

use crate::{
    error::{
        NVAPI_EXECUTABLE_AMBIGUOUS, NVAPI_EXECUTABLE_NOT_FOUND, NVAPI_INVALID_USER_PRIVILEGE,
        NVAPI_PROFILE_NAME_IN_USE, NVAPI_PROFILE_NOT_FOUND, NVAPI_SETTING_NOT_FOUND, NvapiError,
    },
    ffi::{
        NVAPI_BINARY_DATA_MAX, NVDRS_APPLICATION, NVDRS_APPLICATION_VER, NVDRS_BINARY_SETTING,
        NVDRS_CURRENT_PROFILE_LOCATION, NVDRS_DWORD_TYPE, NVDRS_PROFILE, NVDRS_PROFILE_VER,
        NVDRS_QWORD_TYPE, NVDRS_SETTING, NVDRS_SETTING_VER, NvAPI_DRS_CreateApplication_fn,
        NvAPI_DRS_CreateProfile_fn, NvAPI_DRS_CreateSession_fn, NvAPI_DRS_DeleteApplicationEx_fn,
        NvAPI_DRS_DeleteProfile_fn, NvAPI_DRS_DeleteProfileSetting_fn, NvAPI_DRS_DestroySession_fn,
        NvAPI_DRS_EnumApplications_fn, NvAPI_DRS_EnumSettings_fn,
        NvAPI_DRS_FindApplicationByName_fn, NvAPI_DRS_FindProfileByName_fn,
        NvAPI_DRS_GetBaseProfile_fn, NvAPI_DRS_GetProfileInfo_fn, NvAPI_DRS_GetSetting_fn,
        NvAPI_DRS_GetSetting_v2_fn, NvAPI_DRS_LoadSettings_fn, NvAPI_DRS_SaveSettings_fn,
        NvAPI_DRS_SetSetting_fn, NvAPI_DRS_SetSetting_v2_fn, NvAPI_Initialize_fn,
        NvAPI_QueryInterface_fn, NvDRSProfileHandle, NvDRSSessionHandle, interface_ids,
    },
};

// ── FFI helpers ─────────────────────────────────────────────────────────────

/// Converts a Rust `&str` into a null-terminated UTF-16LE vector.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(iter::once(0)).collect()
}

fn write_wide<const N: usize>(target: &mut [u16; N], value: &str) -> Result<(), NvapiError> {
    let encoded = value.encode_utf16().collect::<Vec<_>>();
    if encoded.len() >= N {
        return Err(NvapiError::StringConversion);
    }
    target[..encoded.len()].copy_from_slice(&encoded);
    target[encoded.len()] = 0;
    Ok(())
}

fn read_wide(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

/// Stable, serializable application identity returned by DRS for a checked
/// fully-qualified executable path. Mutation APIs consume this exact witness.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ApplicationIdentity {
    /// Full executable path passed to and returned by DRS.
    pub app_name: String,
    /// NVIDIA's user-facing application label.
    pub user_friendly_name: String,
    /// Launcher identity recorded by DRS.
    pub launcher: String,
    /// Additional file condition; this is not an executable directory.
    pub file_in_folder: String,
    /// Raw NVAPI application flags.
    pub flags: u32,
    /// Optional command-line condition recorded by DRS.
    pub command_line: String,
    /// Whether NVIDIA marks this application record predefined.
    pub is_predefined: bool,
}

impl From<&NVDRS_APPLICATION> for ApplicationIdentity {
    fn from(application: &NVDRS_APPLICATION) -> Self {
        Self {
            app_name: read_wide(&application.appName),
            user_friendly_name: read_wide(&application.userFriendlyName),
            launcher: read_wide(&application.launcher),
            file_in_folder: read_wide(&application.fileInFolder),
            flags: application.flags,
            command_line: read_wide(&application.commandLine),
            is_predefined: application.isPredefined != 0,
        }
    }
}

/// Stable identity and counts for one NVIDIA DRS profile.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProfileIdentity {
    /// Exact display name stored by DRS.
    pub name: String,
    /// Whether NVIDIA marks this profile predefined.
    pub is_predefined: bool,
    /// Number of application records attached to the profile.
    pub application_count: u32,
    /// Number of setting records attached to the profile.
    pub setting_count: u32,
}

/// Stable identity and effective metadata for one DRS setting.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SettingIdentity {
    /// NVIDIA setting identifier.
    pub id: u32,
    /// Raw NVAPI data type.
    pub setting_type: u32,
    /// DRS layer that currently supplies the value.
    pub location: u32,
    /// NVIDIA's `isCurrentPredefined` source flag; this is not numeric equality.
    pub is_current_predefined: bool,
    /// Whether NVIDIA returned a valid predefined value.
    pub is_predefined_valid: bool,
    /// Current value rendered to a stable string representation.
    pub current_value: String,
    /// Predefined value when NVIDIA reports one.
    pub predefined_value: Option<String>,
}

/// Marks an NVAPI struct that [`zeroed_versioned`] can initialize from zeroed memory.
///
/// # Safety
/// `T` must be a `#[repr(C)]` NVAPI POD whose first field is `version: u32`, so
/// that the all-zero bit pattern is a valid pre-initialization state and writing
/// `VERSION` as a `u32` at offset 0 sets exactly that field. Every implementor is
/// guarded by `assert_version_at_offset_zero!` in [`crate::ffi`], which fails the
/// build if the field is ever reordered.
unsafe trait VersionedNvapiStruct: Sized {
    const VERSION: u32;
}

macro_rules! versioned_nvapi_struct {
    ($ty:ty, $version:expr) => {
        // SAFETY: `$ty` keeps `version: u32` at offset 0, proven at compile time by
        // the `assert_version_at_offset_zero!($ty, ...)` guard in `ffi`.
        unsafe impl VersionedNvapiStruct for $ty {
            const VERSION: u32 = $version;
        }
    };
}

versioned_nvapi_struct!(NVDRS_APPLICATION, NVDRS_APPLICATION_VER);
versioned_nvapi_struct!(NVDRS_PROFILE, NVDRS_PROFILE_VER);
versioned_nvapi_struct!(NVDRS_SETTING, NVDRS_SETTING_VER);

/// Creates a zeroed NVAPI struct and initializes its leading `version` word.
fn zeroed_versioned<T: VersionedNvapiStruct>() -> T {
    let mut val = MaybeUninit::<T>::zeroed();
    // SAFETY: implementing `VersionedNvapiStruct` is an unsafe promise that `T` is
    // a `#[repr(C)]` NVAPI POD with `version: u32` at offset 0. The all-zero bytes
    // are therefore a valid initial state, and writing `VERSION` as a `u32` at the
    // front initializes exactly that version word, leaving a fully valid `T`.
    unsafe {
        val.as_mut_ptr().cast::<u32>().write(T::VERSION);
        val.assume_init()
    }
}

/// Initializes the fixed binary output buffers required by DRS read calls.
/// `EnumSettings` and `GetSetting` treat each binary `valueLength` as the
/// caller-provided capacity, so zero is not a usable output buffer length.
fn zeroed_setting_for_read() -> NVDRS_SETTING {
    let mut setting: NVDRS_SETTING = zeroed_versioned();
    setting.predefinedValue = crate::ffi::NVDRS_SETTING_PREDEFINED {
        binaryPredefinedValue: NVDRS_BINARY_SETTING {
            valueLength: NVAPI_BINARY_DATA_MAX as u32,
            valueData: [0; NVAPI_BINARY_DATA_MAX],
        },
    };
    setting.currentValue = crate::ffi::NVDRS_SETTING_CURRENT {
        binaryCurrentValue: NVDRS_BINARY_SETTING {
            valueLength: NVAPI_BINARY_DATA_MAX as u32,
            valueData: [0; NVAPI_BINARY_DATA_MAX],
        },
    };
    setting
}

/// Full state of a DWORD DRS setting on a profile.
///
/// A DRS profile stores a **current** value and may report a **predefined**
/// value (the driver's default for known applications). Explicit profile
/// location determines whether the setting is stored on this profile;
/// `isCurrentPredefined` is preserved as NVIDIA's reported source flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DwordSettingState {
    /// The effective value the driver currently uses.
    pub current: u32,
    /// The driver's factory default; present only when `isPredefinedValid` is set.
    pub predefined: Option<u32>,
    /// NVIDIA's `isCurrentPredefined` source flag. It is not derived by comparing
    /// numeric values and does not determine whether this profile has an override.
    pub is_current_predefined: bool,
    /// Whether this setting is explicitly stored on the resolved profile.
    pub is_explicit_in_profile: bool,
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
    create_profile: Option<NvAPI_DRS_CreateProfile_fn>,
    delete_profile: Option<NvAPI_DRS_DeleteProfile_fn>,
    create_application: Option<NvAPI_DRS_CreateApplication_fn>,
    delete_application_ex: Option<NvAPI_DRS_DeleteApplicationEx_fn>,
    enum_applications: Option<NvAPI_DRS_EnumApplications_fn>,
    enum_settings: Option<NvAPI_DRS_EnumSettings_fn>,
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
    // Global/base profile lookup (optional — present on any modern NVIDIA
    // driver, treated as optional so old systems degrade gracefully without
    // disabling per-game NVAPI). Backs the application-wide DLSS settings.
    get_base_profile: Option<NvAPI_DRS_GetBaseProfile_fn>,
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
            if ptr.is_null() { None } else { Some(ptr) }
        };

        // SAFETY (resolve_fn! / resolve_fn_opt!): `resolve` returns either `None`
        // or a non-null function pointer that `nvapi_QueryInterface` produced for
        // the given interface id. Transmuting it to the matching `extern "C" fn`
        // type is sound because each `$id` is paired with the `$ty` signature taken
        // from NVIDIA's published `nvapi_interface.h`; a mismatched pair is a
        // programmer error caught in review, not a runtime condition.
        macro_rules! resolve_fn {
            ($name:ident, $id:path, $ty:ty) => {
                let $name: $ty = unsafe { std::mem::transmute(resolve($id)?) };
            };
        }

        // Optional functions: resolved with transmute if available, else None.
        macro_rules! resolve_fn_opt {
            ($name:ident, $id:path, $ty:ty) => {
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
        resolve_fn_opt!(
            create_profile,
            interface_ids::DRS_CREATE_PROFILE,
            NvAPI_DRS_CreateProfile_fn
        );
        resolve_fn_opt!(
            delete_profile,
            interface_ids::DRS_DELETE_PROFILE,
            NvAPI_DRS_DeleteProfile_fn
        );
        resolve_fn_opt!(
            create_application,
            interface_ids::DRS_CREATE_APPLICATION,
            NvAPI_DRS_CreateApplication_fn
        );
        resolve_fn_opt!(
            delete_application_ex,
            interface_ids::DRS_DELETE_APPLICATION_EX,
            NvAPI_DRS_DeleteApplicationEx_fn
        );
        resolve_fn_opt!(
            enum_applications,
            interface_ids::DRS_ENUM_APPLICATIONS,
            NvAPI_DRS_EnumApplications_fn
        );
        resolve_fn_opt!(
            enum_settings,
            interface_ids::DRS_ENUM_SETTINGS,
            NvAPI_DRS_EnumSettings_fn
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
        resolve_fn_opt!(
            get_base_profile,
            interface_ids::DRS_GET_BASE_PROFILE,
            NvAPI_DRS_GetBaseProfile_fn
        );

        Some(Self {
            _library: library,
            initialize,
            create_session,
            destroy_session,
            load_settings,
            save_settings,
            find_application,
            create_profile,
            delete_profile,
            create_application,
            delete_application_ex,
            enum_applications,
            enum_settings,
            get_setting,
            get_setting_v2,
            set_setting,
            set_setting_v2,
            delete_profile_setting,
            delete_profile_setting_v2,
            find_profile_by_name,
            get_profile_info,
            get_base_profile,
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
        let status = unsafe { (self.create_session)(&raw mut handle) };
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
        executable_path: &str,
    ) -> Result<(NvDRSProfileHandle, NVDRS_APPLICATION), NvapiError> {
        let wide_name = to_wide(executable_path);

        let mut profile: NvDRSProfileHandle = ptr::null_mut();
        let mut app: NVDRS_APPLICATION = zeroed_versioned();

        let status = unsafe {
            (self.find_application)(session, wide_name.as_ptr(), &raw mut profile, &raw mut app)
        };

        match status {
            0 if !profile.is_null() => Ok((profile, app)),
            NVAPI_EXECUTABLE_NOT_FOUND => Err(NvapiError::ExecutableNotFound),
            NVAPI_EXECUTABLE_AMBIGUOUS => Err(NvapiError::ExecutableAmbiguous),
            other => Err(NvapiError::DrsOperationFailed {
                operation: "find application by full path",
                status: other,
            }),
        }
    }

    fn find_profile_by_name(
        &self,
        session: NvDRSSessionHandle,
        profile_name: &str,
    ) -> Result<NvDRSProfileHandle, NvapiError> {
        let func = self
            .find_profile_by_name
            .ok_or(NvapiError::DrsApiUnavailable("FindProfileByName"))?;
        let wide_name = to_wide(profile_name);
        let mut profile = ptr::null_mut();
        let status = unsafe { (func)(session, wide_name.as_ptr(), &raw mut profile) };
        match status {
            0 if !profile.is_null() => Ok(profile),
            NVAPI_PROFILE_NOT_FOUND => Err(NvapiError::ProfileNotFound),
            other => Err(NvapiError::DrsOperationFailed {
                operation: "find profile by name",
                status: other,
            }),
        }
    }

    /// Resolves the handle of the global/base driver profile.
    fn get_base_profile(
        &self,
        session: NvDRSSessionHandle,
    ) -> Result<NvDRSProfileHandle, NvapiError> {
        let func = self
            .get_base_profile
            .ok_or(NvapiError::BaseProfileUnavailable)?;
        let mut profile: NvDRSProfileHandle = ptr::null_mut();
        let status = unsafe { (func)(session, &raw mut profile) };
        if status != 0 || profile.is_null() {
            return Err(NvapiError::BaseProfileUnavailable);
        }
        Ok(profile)
    }

    fn get_dword_setting_full(
        &self,
        session: NvDRSSessionHandle,
        profile: NvDRSProfileHandle,
        setting_id: u32,
    ) -> Result<DwordSettingState, NvapiError> {
        let mut setting = zeroed_setting_for_read();

        // Prefer the v2 function ID (0xEA99498D) — the same one NVIDIA
        // Inspector uses. Both IDs expose the same NVAPI function but may
        // operate on different DRS layers; using the wrong one means reads
        // from RenderPilot and writes from Inspector (or vice-versa) don't
        // see each other's changes.
        let status = if let Some(get_v2) = self.get_setting_v2 {
            let mut extra: u32 = 0;
            unsafe {
                (get_v2)(
                    session,
                    profile,
                    setting_id,
                    &raw mut setting,
                    &raw mut extra,
                )
            }
        } else {
            unsafe { (self.get_setting)(session, profile, setting_id, &raw mut setting) }
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
            is_explicit_in_profile: setting.settingLocation == NVDRS_CURRENT_PROFILE_LOCATION,
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

    fn set_dword_setting(
        &self,
        session: NvDRSSessionHandle,
        profile: NvDRSProfileHandle,
        setting_id: u32,
        value: u32,
    ) -> Result<(), NvapiError> {
        let mut setting: NVDRS_SETTING = zeroed_versioned();
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
            unsafe { (set_v2)(session, profile, &raw mut setting, 0, 0) }
        } else {
            unsafe { (self.set_setting)(session, profile, &raw mut setting) }
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

    /// Looks up the profile that owns the exact fully-qualified executable path.
    /// The returned handle is the one from `FindApplicationByName`; it is never
    /// re-resolved by display name, which can silently select a different DRS layer.
    pub fn find_profile_by_exe(&self, executable_path: &str) -> Result<Profile<'_>, NvapiError> {
        let (handle, application) = self
            .nvapi
            .find_profile_by_exe(self.handle, executable_path)?;
        Ok(Profile {
            session: self,
            handle,
            matched_application: Some(application),
        })
    }

    /// Finds a profile by its exact DRS profile name.
    pub fn find_profile_by_name(&self, profile_name: &str) -> Result<Profile<'_>, NvapiError> {
        let handle = self.nvapi.find_profile_by_name(self.handle, profile_name)?;
        Ok(Profile {
            session: self,
            handle,
            matched_application: None,
        })
    }

    /// Creates an empty user profile in this session. It is not durable until
    /// [`save`](Self::save) succeeds.
    pub fn create_profile(&self, profile_name: &str) -> Result<Profile<'_>, NvapiError> {
        let func = self
            .nvapi
            .create_profile
            .ok_or(NvapiError::DrsApiUnavailable("CreateProfile"))?;
        let mut info: NVDRS_PROFILE = zeroed_versioned();
        write_wide(&mut info.profileName, profile_name)?;
        let mut handle = ptr::null_mut();
        let status = unsafe { (func)(self.handle, &raw mut info, &raw mut handle) };
        if status != 0 {
            if status == NVAPI_PROFILE_NAME_IN_USE {
                return Err(NvapiError::ProfileNameInUse);
            }
            return Err(NvapiError::DrsOperationFailed {
                operation: "create profile",
                status,
            });
        }
        if handle.is_null() {
            return Err(NvapiError::UnexpectedStatus(-1));
        }
        Ok(Profile {
            session: self,
            handle,
            matched_application: None,
        })
    }

    /// Resolves the global/base driver profile (`_GLOBAL_DRIVER_PROFILE_`),
    /// whose settings apply to every application without its own profile.
    pub fn base_profile(&self) -> Result<Profile<'_>, NvapiError> {
        let handle = self.nvapi.get_base_profile(self.handle)?;
        Ok(Profile {
            session: self,
            handle,
            matched_application: None,
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

    /// Discards cached DRS state by reloading the driver's durable settings.
    pub fn reload(&self) -> Result<(), NvapiError> {
        let status = unsafe { (self.nvapi.load_settings)(self.handle) };
        if status != 0 {
            return Err(NvapiError::LoadSettingsFailed(status));
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
    matched_application: Option<NVDRS_APPLICATION>,
}

impl Profile<'_> {
    /// Reads the profile's exact user/predefined identity and its composition counts.
    pub fn identity(&self) -> Result<ProfileIdentity, NvapiError> {
        let func = self
            .session
            .nvapi
            .get_profile_info
            .ok_or(NvapiError::DrsApiUnavailable("GetProfileInfo"))?;
        let mut info: NVDRS_PROFILE = zeroed_versioned();
        let status = unsafe { (func)(self.session.handle(), self.handle, &raw mut info) };
        if status != 0 {
            return Err(NvapiError::DrsOperationFailed {
                operation: "get profile info",
                status,
            });
        }
        Ok(ProfileIdentity {
            name: read_wide(&info.profileName),
            is_predefined: info.isPredefined != 0,
            application_count: info.numOfApps,
            setting_count: info.numOfSettings,
        })
    }

    /// Returns the exact application witness returned by full-path lookup.
    pub fn matched_application(&self) -> Option<ApplicationIdentity> {
        self.matched_application
            .as_ref()
            .map(ApplicationIdentity::from)
    }

    /// Enumerates every application associated with this profile.
    pub fn applications(&self) -> Result<Vec<ApplicationIdentity>, NvapiError> {
        let func = self
            .session
            .nvapi
            .enum_applications
            .ok_or(NvapiError::DrsApiUnavailable("EnumApplications"))?;
        let count = self.identity()?.application_count;
        let mut applications = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut item: NVDRS_APPLICATION = zeroed_versioned();
            let mut requested = 1;
            let status = unsafe {
                (func)(
                    self.session.handle(),
                    self.handle,
                    index,
                    &raw mut requested,
                    &raw mut item,
                )
            };
            if status != 0 || requested != 1 {
                return Err(NvapiError::DrsOperationFailed {
                    operation: "enumerate applications",
                    status: if status != 0 { status } else { -1 },
                });
            }
            applications.push(ApplicationIdentity::from(&item));
        }
        Ok(applications)
    }

    /// Enumerates every explicitly stored setting, including type, source, and
    /// values. This is used as a deletion ownership witness.
    pub fn settings(&self) -> Result<Vec<SettingIdentity>, NvapiError> {
        let func = self
            .session
            .nvapi
            .enum_settings
            .ok_or(NvapiError::DrsApiUnavailable("EnumSettings"))?;
        let count = self.identity()?.setting_count;
        let mut settings = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut item = zeroed_setting_for_read();
            let mut requested = 1;
            let status = unsafe {
                (func)(
                    self.session.handle(),
                    self.handle,
                    index,
                    &raw mut requested,
                    &raw mut item,
                )
            };
            if status != 0 || requested != 1 {
                return Err(NvapiError::DrsOperationFailed {
                    operation: "enumerate settings",
                    status: if status != 0 { status } else { -1 },
                });
            }
            settings.push(setting_identity(&item)?);
        }
        Ok(settings)
    }

    /// Adds an executable with its full path in `appName` and basename as the
    /// user-friendly label. Callers must save and then verify with full-path
    /// `FindApplicationByName` before taking ownership of the profile.
    pub fn create_application(&self, executable_path: &str) -> Result<(), NvapiError> {
        let func = self
            .session
            .nvapi
            .create_application
            .ok_or(NvapiError::DrsApiUnavailable("CreateApplication"))?;
        let path = std::path::Path::new(executable_path);
        let file_name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .filter(|value| !value.is_empty())
            .ok_or(NvapiError::StringConversion)?;
        let full_path = executable_path.replace('\\', "/");
        let mut application: NVDRS_APPLICATION = zeroed_versioned();
        write_wide(&mut application.appName, &full_path)?;
        write_wide(&mut application.userFriendlyName, file_name)?;
        // NVIDIA defines fileInFolder as an additional file-name selector,
        // not as a directory. The full path lives in appName.
        let status = unsafe { (func)(self.session.handle(), self.handle, &raw mut application) };
        if status != 0 {
            return Err(NvapiError::DrsOperationFailed {
                operation: "create application",
                status,
            });
        }
        Ok(())
    }

    /// Removes the exact application returned for the selected full path.
    pub fn delete_application_exact(
        &self,
        expected: &ApplicationIdentity,
    ) -> Result<(), NvapiError> {
        let func = self
            .session
            .nvapi
            .delete_application_ex
            .ok_or(NvapiError::DrsApiUnavailable("DeleteApplicationEx"))?;
        let matching = self
            .applications()?
            .into_iter()
            .filter(|application| application == expected)
            .count();
        if matching != 1 {
            return Err(NvapiError::ApplicationWitnessMismatch);
        }
        let mut application = application_from_identity(expected)?;
        let status = unsafe { (func)(self.session.handle(), self.handle, &raw mut application) };
        if status != 0 {
            return Err(NvapiError::DrsOperationFailed {
                operation: "delete application",
                status,
            });
        }
        Ok(())
    }

    /// Deletes this profile only when NVAPI marks it as user-created. Callers
    /// must first compare the full application and setting composition with the
    /// durable RenderPilot receipt.
    pub fn delete_user_profile(&self) -> Result<(), NvapiError> {
        if self.identity()?.is_predefined {
            return Err(NvapiError::ProfileIsPredefined);
        }
        let func = self
            .session
            .nvapi
            .delete_profile
            .ok_or(NvapiError::DrsApiUnavailable("DeleteProfile"))?;
        let status = unsafe { (func)(self.session.handle(), self.handle) };
        if status != 0 {
            return Err(NvapiError::DrsOperationFailed {
                operation: "delete profile",
                status,
            });
        }
        Ok(())
    }

    /// Reads the current and predefined values, profile location, and NVIDIA's
    /// `isCurrentPredefined` source flag for `setting_id`.
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

    /// Removes `setting_id` from this profile.
    ///
    /// After the next [`DrsSession::save`], reading the setting returns the
    /// driver's predefined default (if any) or `NVAPI_SETTING_NOT_FOUND`. This is
    /// the primitive behind the UI's "revert to driver default".
    pub fn delete_setting(&self, setting_id: u32) -> Result<(), NvapiError> {
        self.session
            .nvapi
            .delete_profile_setting(self.session.handle(), self.handle, setting_id)
    }
}

fn application_from_identity(
    identity: &ApplicationIdentity,
) -> Result<NVDRS_APPLICATION, NvapiError> {
    let mut application: NVDRS_APPLICATION = zeroed_versioned();
    write_wide(&mut application.appName, &identity.app_name)?;
    write_wide(
        &mut application.userFriendlyName,
        &identity.user_friendly_name,
    )?;
    write_wide(&mut application.launcher, &identity.launcher)?;
    write_wide(&mut application.fileInFolder, &identity.file_in_folder)?;
    write_wide(&mut application.commandLine, &identity.command_line)?;
    application.flags = identity.flags;
    application.isPredefined = u32::from(identity.is_predefined);
    Ok(application)
}

fn setting_identity(setting: &NVDRS_SETTING) -> Result<SettingIdentity, NvapiError> {
    let current_value = setting_value(setting, false)?;
    let predefined_value = if setting.isPredefinedValid != 0 {
        Some(setting_value(setting, true)?)
    } else {
        None
    };
    Ok(SettingIdentity {
        id: setting.settingId,
        setting_type: setting.settingType,
        location: setting.settingLocation,
        is_current_predefined: setting.isCurrentPredefined != 0,
        is_predefined_valid: setting.isPredefinedValid != 0,
        current_value,
        predefined_value,
    })
}

fn setting_value(setting: &NVDRS_SETTING, predefined: bool) -> Result<String, NvapiError> {
    match setting.settingType {
        NVDRS_DWORD_TYPE => {
            // SAFETY: the DRS type is DWORD, so the matching union member is valid.
            let value = unsafe {
                if predefined {
                    setting.predefinedValue.u32PredefinedValue
                } else {
                    setting.currentValue.u32CurrentValue
                }
            };
            Ok(format!("dword:{value:08x}"))
        }
        NVDRS_QWORD_TYPE => {
            // SAFETY: the DRS type is QWORD, so the matching union member is valid.
            let value_bytes = unsafe {
                if predefined {
                    setting.predefinedValue.u64PredefinedValue
                } else {
                    setting.currentValue.u64CurrentValue
                }
            };
            let value = u64::from_ne_bytes(value_bytes);
            Ok(format!("qword:{value:016x}"))
        }
        1 => {
            // SAFETY: the DRS type is binary, so the matching union member is valid.
            let binary = unsafe {
                if predefined {
                    &setting.predefinedValue.binaryPredefinedValue
                } else {
                    &setting.currentValue.binaryCurrentValue
                }
            };
            let length = binary.valueLength as usize;
            if length > binary.valueData.len() {
                return Err(NvapiError::UnexpectedStatus(-1));
            }
            let value = hex::encode(&binary.valueData[..length]);
            Ok(format!("binary:{length}:{value}"))
        }
        2 | 3 => {
            // SAFETY: both DRS string types use the matching wide-string union member.
            let value = unsafe {
                if predefined {
                    &setting.predefinedValue.wszPredefinedValue
                } else {
                    &setting.currentValue.wszCurrentValue
                }
            };
            Ok(format!("string:{}", read_wide(value)))
        }
        _ => Err(NvapiError::UnexpectedSettingType),
    }
}

#[cfg(test)]
mod tests {
    use super::{setting_value, to_wide, zeroed_setting_for_read};
    use crate::ffi::{
        NVAPI_BINARY_DATA_MAX, NVDRS_BINARY_SETTING, NVDRS_QWORD_TYPE, NVDRS_SETTING,
        NVDRS_SETTING_CURRENT, NVDRS_SETTING_PREDEFINED, NVDRS_SETTING_VER,
    };

    #[test]
    fn qword_values_are_serialized_in_profile_witnesses() {
        let setting = NVDRS_SETTING {
            version: NVDRS_SETTING_VER,
            settingName: [0; crate::ffi::NVAPI_UNICODE_STRING_MAX],
            settingId: 7,
            settingType: NVDRS_QWORD_TYPE,
            settingLocation: 0,
            isCurrentPredefined: 0,
            isPredefinedValid: 1,
            predefinedValue: NVDRS_SETTING_PREDEFINED {
                u64PredefinedValue: 0x1122_3344_5566_7788_u64.to_ne_bytes(),
            },
            currentValue: NVDRS_SETTING_CURRENT {
                u64CurrentValue: 0x8877_6655_4433_2211_u64.to_ne_bytes(),
            },
        };
        assert_eq!(
            setting_value(&setting, false).expect("current QWORD"),
            "qword:8877665544332211"
        );
        assert_eq!(
            setting_value(&setting, true).expect("predefined QWORD"),
            "qword:1122334455667788"
        );
    }

    #[test]
    fn binary_read_constructor_sets_both_union_capacities() {
        let setting = zeroed_setting_for_read();
        // SAFETY: constructor initializes both binary union arms as binary buffers.
        let predefined = unsafe { setting.predefinedValue.binaryPredefinedValue.valueLength };
        // SAFETY: constructor initializes both binary union arms as binary buffers.
        let current = unsafe { setting.currentValue.binaryCurrentValue.valueLength };
        assert_eq!(predefined, NVAPI_BINARY_DATA_MAX as u32);
        assert_eq!(current, NVAPI_BINARY_DATA_MAX as u32);
        assert_eq!(setting.version, NVDRS_SETTING_VER);
    }

    #[test]
    fn binary_setting_values_keep_returned_length_and_bytes() {
        let mut setting = zeroed_setting_for_read();
        setting.settingType = 1;
        let mut data = [0; NVAPI_BINARY_DATA_MAX];
        data[..3].copy_from_slice(&[0x00, 0x7f, 0xff]);
        setting.currentValue = NVDRS_SETTING_CURRENT {
            binaryCurrentValue: NVDRS_BINARY_SETTING {
                valueLength: 3,
                valueData: data,
            },
        };
        assert_eq!(
            setting_value(&setting, false).expect("binary current value"),
            "binary:3:007fff"
        );
    }

    #[test]
    fn binary_setting_values_reject_lengths_over_the_buffer() {
        let mut setting = zeroed_setting_for_read();
        setting.settingType = 1;
        setting.currentValue = NVDRS_SETTING_CURRENT {
            binaryCurrentValue: NVDRS_BINARY_SETTING {
                valueLength: (NVAPI_BINARY_DATA_MAX + 1) as u32,
                valueData: [0; NVAPI_BINARY_DATA_MAX],
            },
        };
        assert!(setting_value(&setting, false).is_err());
    }

    #[test]
    fn to_wide_appends_a_nul_terminator() {
        assert_eq!(to_wide("ab"), vec![0x0061, 0x0062, 0x0000]);
        assert_eq!(to_wide(""), vec![0x0000]);
    }

    #[test]
    fn to_wide_encodes_non_ascii_as_utf16() {
        // 'é' = U+00E9, '☃' = U+2603 — both inside the BMP, one code unit each.
        assert_eq!(to_wide("é☃"), vec![0x00E9, 0x2603, 0x0000]);
    }

    #[test]
    fn to_wide_encodes_astral_chars_as_surrogate_pairs() {
        // '🎮' = U+1F3AE encodes as the surrogate pair D83C DFAE, then the NUL.
        assert_eq!(to_wide("🎮"), vec![0xD83C, 0xDFAE, 0x0000]);
    }
}
