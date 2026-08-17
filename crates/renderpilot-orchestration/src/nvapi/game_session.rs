//! Guard-owning NVAPI sessions for one game or the global driver profile.
//!
//! A per-game session is the only public path to the low-level operations. It
//! acquires the game mutation boundary before recovery, catalog readiness,
//! reconciled DLL projection, executable resolution, validation, DRS work, and
//! response assembly, preventing a file mutation from racing any of them.

use std::path::Path;

use renderpilot_domain::GameId;
use renderpilot_nvapi::NvapiSetting;

use super::dto::SettingStateResponse;
use super::ops::{
    SettingTarget, WriteOp, ensure_dll_setting_catalog_ready, read_all_setting_states,
    read_setting_state, resolve_revert_op, validate_value_supported, write_setting_value,
};
use super::resolve::{
    build_setting_context_with_context, global_setting_context, load_game_with_context,
};
use crate::{Context, ServiceError, game_mutation_lock};

/// Exclusive per-game NVAPI operation session.
///
/// Its fields are private so neither the mutation guard nor a detached game
/// `SettingContext` can escape to callers that might use them after release.
pub struct GameNvapiSession<'context> {
    context: &'context Context,
    game_id: GameId,
    setting_context: renderpilot_nvapi::SettingContext,
    _guard: game_mutation_lock::GameMutationGuard,
}

impl<'context> GameNvapiSession<'context> {
    /// Enters the game mutation boundary, recovers pending work, then captures
    /// the authoritative game projection used by this session.
    pub fn open(context: &'context Context, game_id: GameId) -> Result<Self, ServiceError> {
        let guard = game_mutation_lock::enter_game_mutation_boundary(context, &game_id)?;
        let game = load_game_with_context(context, game_id.as_str())?;
        let install_dir = Path::new(game.install_path().as_str());
        let setting_context =
            build_setting_context_with_context(context, install_dir, game_id.as_str())?;
        Ok(Self {
            context,
            game_id,
            setting_context,
            _guard: guard,
        })
    }

    /// Reads every supplied setting while this game's mutation boundary remains held.
    pub fn read_all(
        &self,
        settings: &[Box<dyn NvapiSetting>],
    ) -> Result<Vec<SettingStateResponse>, ServiceError> {
        read_all_setting_states(
            self.context,
            &self.target(),
            settings,
            &self.setting_context,
        )
    }

    /// Reads one setting while this game's mutation boundary remains held.
    pub fn read(&self, setting: &dyn NvapiSetting) -> Result<SettingStateResponse, ServiceError> {
        read_setting_state(self.context, &self.target(), setting, &self.setting_context)
    }

    /// Validates and applies a DLL-dependent-aware setting update, then returns
    /// a response assembled before releasing the game boundary.
    pub fn set(
        &self,
        setting: &dyn NvapiSetting,
        dword: u32,
    ) -> Result<SettingStateResponse, ServiceError> {
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
    /// baseline lookup, DRS operation, or baseline mutation.
    pub fn revert(
        &self,
        setting: &dyn NvapiSetting,
        revert_target: &str,
    ) -> Result<SettingStateResponse, ServiceError> {
        ensure_dll_setting_catalog_ready(setting, &self.setting_context)?;
        let target = self.target();
        let op = resolve_revert_op(self.context, &target, setting, revert_target)?;
        write_setting_value(self.context, &target, setting, &self.setting_context, op)?;
        self.read(setting)
    }

    fn target(&self) -> SettingTarget<'_> {
        SettingTarget::Game {
            game_id: self.game_id.as_str(),
        }
    }
}

/// Global/base-profile NVAPI operation session. It deliberately owns no game
/// mutation guard because no per-game catalog or filesystem state participates.
pub struct GlobalNvapiSession<'context> {
    context: &'context Context,
    setting_context: renderpilot_nvapi::SettingContext,
}

impl<'context> GlobalNvapiSession<'context> {
    /// Opens the global profile path without acquiring any per-game lock.
    pub fn open(context: &'context Context) -> Self {
        Self {
            context,
            setting_context: global_setting_context(),
        }
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
        let op = resolve_revert_op(self.context, &SettingTarget::Global, setting, revert_target)?;
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
