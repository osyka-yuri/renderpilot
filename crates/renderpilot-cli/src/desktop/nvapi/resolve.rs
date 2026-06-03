use std::path::Path;

use renderpilot_application::GameRepository;
use renderpilot_domain::GameId;
use renderpilot_nvapi::setting::{DllInfo, DlssDllKind, SettingContext};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::{catalog::open_catalog_storage, error::CliError};

#[cfg(windows)]
use renderpilot_platform_windows::{
    detect_executable_candidates, dlss as platform_dlss, ExecutableCandidate,
};

pub fn load_game(game_id: &str) -> Result<renderpilot_domain::GameInstallation, CliError> {
    let storage = open_catalog_storage()?;
    load_game_with_storage(&storage, game_id)
}

pub fn load_game_with_storage(
    storage: &SqliteStorage,
    game_id: &str,
) -> Result<renderpilot_domain::GameInstallation, CliError> {
    let parsed = GameId::new(game_id).map_err(|_| CliError::InvalidGameId(game_id.to_owned()))?;
    storage
        .find_game(&parsed)?
        .ok_or_else(|| CliError::GameNotFound(game_id.to_owned()))
}

pub fn build_setting_context_with_storage(
    storage: &SqliteStorage,
    install_dir: &Path,
    game_id: &str,
) -> Result<SettingContext, CliError> {
    let override_row = storage.get_nvapi_executable_override(game_id)?;

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

    #[cfg(windows)]
    let dlls = {
        let mut dlls: std::collections::HashMap<DlssDllKind, DllInfo> =
            std::collections::HashMap::new();
        for hit in platform_dlss::find_dlss_dlls(install_dir) {
            if let Ok(version) = platform_dlss::read_dll_version(&hit.path) {
                dlls.entry(hit.kind).or_insert(DllInfo {
                    path: hit.path,
                    version,
                });
            }
        }
        dlls
    };

    #[cfg(not(windows))]
    let dlls: std::collections::HashMap<DlssDllKind, DllInfo> = std::collections::HashMap::new();

    Ok(SettingContext {
        game_install_dir: install_dir.to_path_buf(),
        dlls,
        effective_exe,
    })
}

#[cfg(windows)]
pub fn collect_executable_candidates(install_dir: &Path) -> Vec<ExecutableCandidate> {
    detect_executable_candidates(install_dir)
}

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
