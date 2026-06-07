use std::path::Path;

use crate::dlss::installed::installed_dlls_from_components;
use crate::ServiceError;
use renderpilot_application::{ComponentRepository, GameRepository};
use renderpilot_domain::GameId;
use renderpilot_nvapi::setting::SettingContext;

#[cfg(windows)]
use renderpilot_platform_windows::{detect_executable_candidates, ExecutableCandidate};

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
    let game = load_game_with_context(context, game_id)?;
    let install_dir = Path::new(game.install_path().as_str());
    let exe_path = Path::new(absolute_path);

    let canonical_install = install_dir.canonicalize().map_err(|error| {
        ServiceError::CommandFailed(format!(
            "could not canonicalize install dir {}: {error}",
            install_dir.display()
        ))
    })?;
    let canonical_exe = exe_path.canonicalize().map_err(|error| {
        ServiceError::CommandFailed(format!(
            "could not canonicalize executable {}: {error}",
            exe_path.display()
        ))
    })?;
    if !canonical_exe.starts_with(&canonical_install) {
        return Err(ServiceError::CommandFailed(format!(
            "executable must be located inside the install directory ({})",
            install_dir.display()
        )));
    }
    let file_name = canonical_exe
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ServiceError::CommandFailed("executable path has no file name".to_owned())
        })?;

    let normalized = canonical_exe.to_string_lossy().replace('\\', "/");
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
    let _game = load_game_with_context(context, game_id)?;
    context
        .storage()
        .delete_nvapi_executable_override(game_id)?;
    Ok(())
}

/// Builds the NVAPI [`SettingContext`] for a game: detected DLSS DLLs and
/// effective executable, using an already-open storage connection.
pub fn build_setting_context_with_context(
    context: &crate::Context,
    install_dir: &Path,
    game_id: &str,
) -> Result<SettingContext, ServiceError> {
    let override_row = context.storage().get_nvapi_executable_override(game_id)?;

    let effective_exe = if let Some(row) = override_row {
        if Path::new(&row.selected_path).exists() {
            Some(row.selected_basename)
        } else {
            None
        }
    } else {
        None
    };

    let effective_exe = effective_exe.or_else(|| pick_exe_with_profile_fallback(install_dir));

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

#[cfg(windows)]
fn pick_exe_with_profile_fallback(install_dir: &Path) -> Option<String> {
    use renderpilot_nvapi::Nvapi;

    let supported: Vec<ExecutableCandidate> = detect_executable_candidates(install_dir)
        .into_iter()
        .filter(|c| c.rejection.is_none())
        .collect();
    if supported.is_empty() {
        return None;
    }

    let default_pick = supported[0].file_name.clone();

    let Some(nvapi) = Nvapi::get() else {
        return Some(default_pick);
    };
    if nvapi.initialize().is_err() {
        return Some(default_pick);
    }
    let Ok(session) = nvapi.create_session() else {
        return Some(default_pick);
    };
    for candidate in &supported {
        if session.find_profile_by_exe(&candidate.file_name).is_ok() {
            return Some(candidate.file_name.clone());
        }
    }
    Some(default_pick)
}

#[cfg(not(windows))]
fn pick_exe_with_profile_fallback(_install_dir: &Path) -> Option<String> {
    None
}
