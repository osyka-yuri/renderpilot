use std::path::{Path, PathBuf};

use crate::ServiceError;
use crate::dlss::installed::installed_dlls_from_components;
use renderpilot_application::{ComponentRepository, GameRepository};
use renderpilot_domain::GameId;
use renderpilot_nvapi::setting::SettingContext;

#[cfg(windows)]
use renderpilot_platform_windows::{ExecutableCandidate, detect_executable_candidates};

/// Loads a game from the catalog by its string id.
pub fn load_game(game_id: &str) -> Result<renderpilot_domain::GameInstallation, ServiceError> {
    let context = crate::Context::open()?;
    load_game_with_context(&context, game_id)
}

/// Loads a game from an already-open catalog storage by its string id.
pub fn load_game_with_context(
    context: &crate::Context,
    game_id: &str,
) -> Result<renderpilot_domain::GameInstallation, ServiceError> {
    let parsed =
        GameId::new(game_id).map_err(|_| ServiceError::GameNotFound(game_id.to_owned()))?;
    context
        .storage()
        .find_game(&parsed)?
        .ok_or_else(|| ServiceError::GameNotFound(game_id.to_owned()))
}

/// Pins an explicit executable override for `game_id`.
///
/// Validates that `absolute_path` resolves to a file inside the game's install
/// directory, then persists the canonicalized (forward-slash) path and basename.
pub fn set_executable_override(
    context: &crate::Context,
    game_id: &str,
    absolute_path: &str,
) -> Result<(), ServiceError> {
    let parsed =
        GameId::new(game_id).map_err(|_| ServiceError::GameNotFound(game_id.to_owned()))?;
    let _guard = crate::game_mutation_lock::blocking_lock(&parsed);
    let game = context
        .storage()
        .find_game(&parsed)?
        .ok_or_else(|| ServiceError::GameNotFound(game_id.to_owned()))?;
    let install_dir = Path::new(game.install_path().as_str());
    let exe_path = Path::new(absolute_path);

    let canonical_install = install_dir.canonicalize().map_err(|error| {
        ServiceError::command_failed(format!(
            "could not canonicalize install dir {}: {error}",
            install_dir.display()
        ))
    })?;
    let canonical_exe = exe_path.canonicalize().map_err(|error| {
        ServiceError::command_failed(format!(
            "could not canonicalize executable {}: {error}",
            exe_path.display()
        ))
    })?;
    if !canonical_exe.starts_with(&canonical_install) {
        return Err(ServiceError::command_failed(format!(
            "executable must be located inside the install directory ({})",
            install_dir.display()
        )));
    }
    let file_name = canonical_exe
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ServiceError::command_failed("executable path has no file name"))?;

    let normalized = canonical_exe.to_string_lossy().replace('\\', "/");
    require_d3d12_executable_binding(context, game.id(), Some(&normalized))?;
    context
        .storage()
        .upsert_nvapi_executable_override(game_id, &normalized, file_name)?;
    Ok(())
}

/// Clears any pinned executable override for `game_id`.
pub fn clear_executable_override(
    context: &crate::Context,
    game_id: &str,
) -> Result<(), ServiceError> {
    let parsed =
        GameId::new(game_id).map_err(|_| ServiceError::GameNotFound(game_id.to_owned()))?;
    let _guard = crate::game_mutation_lock::blocking_lock(&parsed);
    context
        .storage()
        .find_game(&parsed)?
        .ok_or_else(|| ServiceError::GameNotFound(game_id.to_owned()))?;
    require_d3d12_executable_binding(context, &parsed, None)?;
    context
        .storage()
        .delete_nvapi_executable_override(game_id)?;
    Ok(())
}

fn require_d3d12_executable_binding(
    context: &crate::Context,
    game_id: &GameId,
    requested_path: Option<&str>,
) -> Result<(), ServiceError> {
    let bound = context
        .storage()
        .component_backups_for_game(game_id)?
        .into_values()
        .find_map(|baseline| {
            baseline
                .d3d12_executable()
                .map(|executable| executable.executable_path().clone())
        });
    let Some(bound) = bound else {
        return Ok(());
    };
    if requested_path.is_some_and(|path| {
        renderpilot_domain::normalized_path_key(path)
            == renderpilot_domain::normalized_path_key(bound.as_str())
    }) {
        return Ok(());
    }
    Err(ServiceError::command_failed(format!(
        "executable selection is locked to {} until the D3D12 component is fully rolled back",
        bound.as_str()
    )))
}

/// The user's pinned executable override for `game_id` as an absolute path, or
/// `None` when none is set. The shared game-level override read by NVAPI (profile
/// target) and RenoDX (install renderer); the resolver checks the path still exists.
pub fn stored_override_path(
    context: &crate::Context,
    game_id: &str,
) -> Result<Option<PathBuf>, ServiceError> {
    Ok(context
        .storage()
        .get_nvapi_executable_override(game_id)?
        .map(|row| PathBuf::from(row.selected_path)))
}

/// Builds the NVAPI [`SettingContext`] for a game: detected DLSS DLLs and
/// effective executable, using an already-open storage connection.
pub fn build_setting_context_with_context(
    context: &crate::Context,
    install_dir: &Path,
    game_id: &str,
) -> Result<SettingContext, ServiceError> {
    // The shared game-level override; the resolver checks it still exists and falls
    // back to auto-detection when it does not.
    let override_path = stored_override_path(context, game_id)?;
    let effective_exe = pick_effective_exe(install_dir, override_path.as_deref());

    // Reuse the global catalog's scan instead of walking the install dir again:
    // detection already found every DLSS DLL (to depth 12) and stored its version.
    let game = GameId::new(game_id).map_err(|_| ServiceError::GameNotFound(game_id.to_owned()))?;
    let components = context.storage().list_components_for_game(&game)?;
    let dlls = installed_dlls_from_components(&components);

    Ok(SettingContext {
        game_install_dir: install_dir.to_path_buf(),
        dlls,
        effective_exe,
    })
}

/// Builds the [`SettingContext`] for the global/base driver profile.
///
/// There is no game, executable, or install directory, so DLL detection is
/// empty — which means per-version preset constraints are skipped and every
/// catalog value is offered (a global setting cannot be tied to one game's DLL).
pub fn global_setting_context() -> SettingContext {
    SettingContext {
        game_install_dir: std::path::PathBuf::new(),
        dlls: std::collections::HashMap::new(),
        effective_exe: None,
    }
}

/// Collects executable candidates from the game installation directory.
#[cfg(windows)]
pub fn collect_executable_candidates(install_dir: &Path) -> Vec<ExecutableCandidate> {
    detect_executable_candidates(install_dir)
}

/// Non-Windows stub: executable detection is only supported on Windows.
#[cfg(not(windows))]
pub fn collect_executable_candidates(_install_dir: &Path) -> Vec<()> {
    Vec::new()
}

/// Resolves the effective profile executable: the user override (if any), else the
/// shared resolver's pick — but biased toward a candidate that already has an
/// NVIDIA driver profile, so we read/write the profile the driver actually applies.
#[cfg(windows)]
fn pick_effective_exe(install_dir: &Path, override_path: Option<&Path>) -> Option<String> {
    use crate::game_executable::{self, ExeSource};
    use renderpilot_nvapi::Nvapi;

    // The shared resolver: override wins; NVAPI does not prefer DirectX (a Vulkan
    // game is still the profile target), so `prefer_directx` is false.
    let resolved = game_executable::resolve_primary_executable(install_dir, override_path, false)?;
    // An explicit override is authoritative — never second-guess it.
    if resolved.source == ExeSource::Override {
        return Some(resolved.file_name);
    }
    let default_pick = resolved.file_name;

    let Some(nvapi) = Nvapi::get() else {
        return Some(default_pick);
    };
    if nvapi.initialize().is_err() {
        return Some(default_pick);
    }
    let Ok(session) = nvapi.create_session() else {
        return Some(default_pick);
    };
    for candidate in detect_executable_candidates(install_dir)
        .into_iter()
        .filter(|c| c.rejection.is_none())
    {
        if session.find_profile_by_exe(&candidate.file_name).is_ok() {
            return Some(candidate.file_name);
        }
    }
    Some(default_pick)
}

#[cfg(not(windows))]
fn pick_effective_exe(_install_dir: &Path, _override_path: Option<&Path>) -> Option<String> {
    None
}

/// The game's effective primary executable for the shared game-level UI: the
/// resolver's pick (a pinned override or the auto-detected renderer), independent
/// of NVAPI hardware so it works for any GPU.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EffectiveExecutable {
    /// Basename, e.g. `Game.exe`.
    pub file_name: String,
    /// Absolute path on disk (forward slashes).
    pub absolute_path: String,
    /// `"override"` when pinned by the user, `"auto"` when auto-detected.
    pub source: &'static str,
}

/// Resolves the effective executable for `game_id`'s install directory, honoring a
/// pinned override. Shared by the game-level executable UI; both NVAPI and RenoDX
/// read this same selection. `None` when the directory holds no game binary.
pub fn resolve_effective_executable(
    context: &crate::Context,
    install_dir: &Path,
    game_id: &str,
) -> Result<Option<EffectiveExecutable>, ServiceError> {
    let override_path = stored_override_path(context, game_id)?;
    Ok(crate::game_executable::resolve_primary_executable(
        install_dir,
        override_path.as_deref(),
        false,
    )
    .map(|resolved| EffectiveExecutable {
        file_name: resolved.file_name,
        absolute_path: resolved.path.as_str().to_owned(),
        source: match resolved.source {
            crate::game_executable::ExeSource::Override => "override",
            crate::game_executable::ExeSource::Auto => "auto",
        },
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};

    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };
    use renderpilot_storage_sqlite::SqliteStorage;

    use super::{global_setting_context, set_executable_override};

    #[test]
    fn global_context_has_no_game_dll_or_exe() {
        // These invariants are what make version-gating a no-op and offer every
        // catalog value on the global profile; assert them at the source.
        let ctx = global_setting_context();
        assert!(ctx.dlls.is_empty());
        assert!(ctx.effective_exe.is_none());
        assert_eq!(ctx.game_install_dir.as_os_str().len(), 0);
    }

    #[test]
    fn executable_override_waits_for_the_game_mutation_boundary() {
        let directory = tempfile::tempdir().expect("temporary game directory");
        let executable = directory.path().join("game.exe");
        std::fs::write(&executable, b"test executable").expect("executable fixture");
        let game_id =
            GameId::new(format!("manual:override-lock-{}", ulid::Ulid::generate())).expect("id");
        let identity = GameIdentity::new(game_id.clone(), "Override Lock", Launcher::Manual)
            .expect("identity");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(directory.path().to_string_lossy().into_owned()).expect("install path"),
        );
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("persist game");
        let context = Arc::new(crate::Context::from_storage(storage));

        let held = crate::game_mutation_lock::blocking_lock(&game_id);
        let (attempt_tx, attempt_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        crate::game_mutation_lock::set_lock_attempt_hook(&game_id, attempt_tx);

        let worker_context = Arc::clone(&context);
        let worker_game_id = game_id.as_str().to_owned();
        let worker_executable = executable.to_string_lossy().into_owned();
        let worker = std::thread::spawn(move || {
            done_tx
                .send(set_executable_override(
                    &worker_context,
                    &worker_game_id,
                    &worker_executable,
                ))
                .expect("report result");
        });

        attempt_rx
            .recv()
            .expect("override reached mutation boundary");
        assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert!(
            context
                .storage()
                .get_nvapi_executable_override(game_id.as_str())
                .expect("read override")
                .is_none(),
            "override must not be persisted before the game boundary is acquired"
        );

        drop(held);
        done_rx
            .recv()
            .expect("worker result")
            .expect("set override");
        worker.join().expect("worker");
        assert!(
            context
                .storage()
                .get_nvapi_executable_override(game_id.as_str())
                .expect("read override")
                .is_some()
        );
    }
}
