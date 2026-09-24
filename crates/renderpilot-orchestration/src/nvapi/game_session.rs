//! Guard-owning NVAPI sessions for one game or the global driver profile.
//!
//! A per-game session is the only public path to the low-level operations. It
//! acquires the game mutation boundary before recovery, catalog readiness,
//! reconciled DLL projection, executable resolution, validation, DRS work, and
//! response assembly, preventing a file mutation from racing any of them.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};
#[cfg(windows)]
use std::{io, ptr};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_FAILED, WAIT_OBJECT_0},
    System::Threading::{INFINITE, WaitForSingleObject},
};

use renderpilot_domain::GameId;
use renderpilot_nvapi::NvapiSetting;

use super::dto::SettingStateResponse;
use super::ops::{
    SettingTarget, WriteOp, ensure_dll_setting_catalog_ready, read_all_setting_states,
    read_setting_state, resolve_revert_op, validate_value_supported, write_setting_value,
};
use super::profiles::NvapiProfileStatusDto;
use super::resolve::{
    build_setting_context_with_context, global_setting_context, load_game_with_context,
};
use crate::{Context, ServiceError, game_mutation_lock};

static DRS_OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// One cross-process lock orders every RenderPilot DRS operation, including
/// global-profile writes and operations belonging to different games.
pub(crate) fn lock_drs_operations() -> Result<DrsOperationGuard, ServiceError> {
    let in_process = DRS_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    #[cfg(windows)]
    let cross_process = acquire_drs_mutex()?;
    #[cfg(not(windows))]
    let cross_process = ();
    Ok(DrsOperationGuard {
        _in_process: in_process,
        _cross_process: cross_process,
    })
}

pub(crate) struct DrsOperationGuard {
    _in_process: MutexGuard<'static, ()>,
    #[cfg(windows)]
    _cross_process: WindowsDrsMutex,
    #[cfg(not(windows))]
    _cross_process: (),
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "the process-wide DRS authority uses one named Windows kernel mutex"
)]
unsafe extern "system" {
    fn CreateMutexW(
        attributes: *const core::ffi::c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> HANDLE;
    fn ReleaseMutex(mutex: HANDLE) -> i32;
}

#[cfg(windows)]
struct WindowsDrsMutex(HANDLE);

#[cfg(windows)]
impl Drop for WindowsDrsMutex {
    #[expect(
        unsafe_code,
        reason = "the DRS mutex guard releases only its acquired owned handle"
    )]
    fn drop(&mut self) {
        // SAFETY: the guard is constructed only after this handle was acquired.
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "a named Windows mutex serializes DRS writes across app processes"
)]
fn acquire_drs_mutex() -> Result<WindowsDrsMutex, ServiceError> {
    let name = "Local\\RenderPilot-NVAPI-DRS-Lock-v1"
        .encode_utf16()
        .chain(std::iter::once(0_u16))
        .collect::<Vec<_>>();
    // SAFETY: null attributes request a non-inheritable handle; name is NUL-terminated.
    let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(ServiceError::command_failed(format!(
            "could not create the cross-process NVIDIA DRS lock: {}",
            io::Error::last_os_error()
        )));
    }
    // SAFETY: handle is the live mutex returned by CreateMutexW.
    let wait = unsafe { WaitForSingleObject(handle, INFINITE) };
    if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
        return Ok(WindowsDrsMutex(handle));
    }
    // SAFETY: acquisition failed, so this unowned live handle is closed here.
    unsafe {
        let _ = CloseHandle(handle);
    }
    let detail = if wait == WAIT_FAILED {
        io::Error::last_os_error().to_string()
    } else {
        format!("unexpected wait result 0x{wait:08X}")
    };
    Err(ServiceError::command_failed(format!(
        "could not acquire the cross-process NVIDIA DRS lock: {detail}"
    )))
}

/// Exclusive per-game NVAPI operation session.
///
/// Its fields are private so neither the mutation guard nor a detached game
/// `SettingContext` can escape to callers that might use them after release.
pub struct GameNvapiSession<'context> {
    context: &'context Context,
    game_id: GameId,
    setting_context: renderpilot_nvapi::SettingContext,
    _guard: game_mutation_lock::GameMutationGuard,
    _drs_guard: DrsOperationGuard,
}

impl<'context> GameNvapiSession<'context> {
    /// Enters the game mutation boundary, recovers pending work, then captures
    /// the authoritative game projection used by this session.
    pub fn open(context: &'context Context, game_id: GameId) -> Result<Self, ServiceError> {
        let guard = crate::mutation_boundary::enter_game_mutation_boundary(context, &game_id)?;
        let drs_guard = lock_drs_operations()?;
        super::recovery::recover_game_pending(context, game_id.as_str())?;
        let game = load_game_with_context(context, game_id.as_str())?;
        let install_dir = Path::new(game.install_path().as_str());
        let setting_context =
            build_setting_context_with_context(context, install_dir, game_id.as_str())?;
        Ok(Self {
            context,
            game_id,
            setting_context,
            _guard: guard,
            _drs_guard: drs_guard,
        })
    }

    /// Reads every supplied setting while this game's mutation boundary remains held.
    pub fn read_all(
        &self,
        settings: &[Box<dyn NvapiSetting>],
    ) -> Result<Vec<SettingStateResponse>, ServiceError> {
        self.ensure_selected_exe_matches_owned_profile()?;
        read_all_setting_states(
            self.context,
            &self.target(),
            settings,
            &self.setting_context,
        )
    }

    /// Reads one setting while this game's mutation boundary remains held.
    pub fn read(&self, setting: &dyn NvapiSetting) -> Result<SettingStateResponse, ServiceError> {
        self.ensure_selected_exe_matches_owned_profile()?;
        read_setting_state(self.context, &self.target(), setting, &self.setting_context)
    }

    /// Validates and applies a DLL-dependent-aware setting update, then returns
    /// a response assembled before releasing the game boundary.
    pub fn set(
        &self,
        setting: &dyn NvapiSetting,
        dword: u32,
    ) -> Result<SettingStateResponse, ServiceError> {
        self.ensure_selected_exe_matches_owned_profile()?;
        validate_value_supported(setting, dword, &self.setting_context)?;
        write_setting_value(
            self.context,
            &self.target(),
            setting,
            &self.setting_context,
            WriteOp::Set(dword),
        )?;
        self.read(setting)
    }

    /// Reverts a setting after rejecting unready DLL-dependent games before a
    /// claim lookup or DRS operation.
    pub fn revert(
        &self,
        setting: &dyn NvapiSetting,
        revert_target: &str,
    ) -> Result<SettingStateResponse, ServiceError> {
        self.ensure_selected_exe_matches_owned_profile()?;
        ensure_dll_setting_catalog_ready(setting, &self.setting_context)?;
        let target = self.target();
        let op = resolve_revert_op(&target, revert_target)?;
        write_setting_value(self.context, &target, setting, &self.setting_context, op)?;
        self.read(setting)
    }

    /// Returns the selected executable's DRS profile state without changing it.
    pub fn profile_status(&self) -> Result<NvapiProfileStatusDto, ServiceError> {
        super::profiles::profile_status(self.context, self.game_id.as_str(), &self.setting_context)
    }

    /// Explicitly creates the one RenderPilot-managed profile for this game.
    pub fn create_profile(&self) -> Result<(), ServiceError> {
        super::profiles::create_owned_profile(
            self.context,
            self.game_id.as_str(),
            &self.setting_context,
        )
    }

    /// Deletes only this game's profile after exact receipt and composition checks.
    pub fn delete_profile(&self) -> Result<(), ServiceError> {
        super::profiles::delete_owned_profile(
            self.context,
            self.game_id.as_str(),
            &self.setting_context,
        )
    }

    /// Moves the owned profile to the explicitly confirmed destination path.
    pub fn move_profile(
        &self,
        new_path: &str,
        select_automatically: bool,
    ) -> Result<(), ServiceError> {
        super::profiles::move_owned_profile(
            self.context,
            self.game_id.as_str(),
            new_path,
            select_automatically,
        )
    }

    fn target(&self) -> SettingTarget<'_> {
        SettingTarget::Game {
            game_id: self.game_id.as_str(),
        }
    }

    fn ensure_selected_exe_matches_owned_profile(&self) -> Result<(), ServiceError> {
        let Some(owned) = self
            .context
            .storage()
            .get_nvapi_owned_profile(self.game_id.as_str())?
        else {
            return Ok(());
        };
        let Some(selected) = self.setting_context.effective_exe_path.as_deref() else {
            return Err(ServiceError::command_failed(format!(
                "the RenderPilot NVIDIA profile is bound to {}; select that executable or move the profile explicitly",
                owned.binding_path
            )));
        };
        if renderpilot_domain::normalized_path_key(selected)
            != renderpilot_domain::normalized_path_key(&owned.binding_path)
        {
            return Err(ServiceError::command_failed(format!(
                "the RenderPilot NVIDIA profile is bound to {}; the selected executable is {}; move the profile explicitly before using NVIDIA settings",
                owned.binding_path, selected
            )));
        }
        Ok(())
    }
}

/// Global/base-profile NVAPI operation session. It deliberately owns no game
/// mutation guard because no per-game catalog or filesystem state participates.
pub struct GlobalNvapiSession<'context> {
    context: &'context Context,
    setting_context: renderpilot_nvapi::SettingContext,
    _drs_guard: DrsOperationGuard,
}

impl<'context> GlobalNvapiSession<'context> {
    /// Opens the global profile path without acquiring any per-game lock.
    pub fn open(context: &'context Context) -> Result<Self, ServiceError> {
        let drs_guard = lock_drs_operations()?;
        super::recovery::recover_global_pending(context)?;
        Ok(Self {
            context,
            setting_context: global_setting_context(),
            _drs_guard: drs_guard,
        })
    }

    /// Reads every supplied global setting through one DRS session.
    pub fn read_all(
        &self,
        settings: &[Box<dyn NvapiSetting>],
    ) -> Result<Vec<SettingStateResponse>, ServiceError> {
        read_all_setting_states(
            self.context,
            &SettingTarget::Global,
            settings,
            &self.setting_context,
        )
    }

    /// Applies a global setting update and reads its current state.
    pub fn set(
        &self,
        setting: &dyn NvapiSetting,
        dword: u32,
    ) -> Result<SettingStateResponse, ServiceError> {
        validate_value_supported(setting, dword, &self.setting_context)?;
        write_setting_value(
            self.context,
            &SettingTarget::Global,
            setting,
            &self.setting_context,
            WriteOp::Set(dword),
        )?;
        self.read(setting)
    }

    /// Reverts one global setting to the requested global-compatible target.
    pub fn revert(
        &self,
        setting: &dyn NvapiSetting,
        revert_target: &str,
    ) -> Result<SettingStateResponse, ServiceError> {
        let op = resolve_revert_op(&SettingTarget::Global, revert_target)?;
        write_setting_value(
            self.context,
            &SettingTarget::Global,
            setting,
            &self.setting_context,
            op,
        )?;
        self.read(setting)
    }

    fn read(&self, setting: &dyn NvapiSetting) -> Result<SettingStateResponse, ServiceError> {
        read_setting_state(
            self.context,
            &SettingTarget::Global,
            setting,
            &self.setting_context,
        )
    }
}
