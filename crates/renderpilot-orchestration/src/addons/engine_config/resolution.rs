use std::path::{Path, PathBuf};

/// Bounded resolution result for the supported Unreal config locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineIniResolution {
    /// One existing platform directory has a usable target file.
    Ready(PathBuf),
    /// One existing platform directory exists but the game has not created its
    /// Engine.ini yet; creating the file is safe.
    ReadyToCreate(PathBuf),
    /// No supported directory exists yet, normally before first launch.
    PendingFirstLaunch,
    /// More than one plausible platform location exists and cannot be chosen.
    Conflict,
    /// Project identity was not proven.
    ManualOnly,
}

impl EngineIniResolution {
    /// Target path when resolution proved an existing or safe-to-create target.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Ready(path) | Self::ReadyToCreate(path) => Some(path.as_path()),
            _ => None,
        }
    }
}

/// Supported Windows Unreal config platform names, in stable order.
pub const UNREAL_CONFIG_PLATFORMS: &[&str] = &["Windows", "WindowsNoEditor", "WinGDK"];

/// Resolves only exact, bounded Unreal locations.  This function never scans
/// recursively and never creates a directory.
pub fn resolve_unreal_engine_ini(
    project_root: Option<&Path>,
    project_name: Option<&str>,
    local_app_data: Option<&Path>,
) -> EngineIniResolution {
    let Some(project_name) = project_name.filter(|value| is_safe_project_name(value)) else {
        return EngineIniResolution::ManualOnly;
    };
    let mut candidates = Vec::new();
    if let Some(app_data) = local_app_data {
        candidates.extend(platform_candidates(
            &app_data.join(project_name).join("Saved").join("Config"),
        ));
    }
    if let Some(root) = project_root {
        candidates.extend(platform_candidates(&root.join("Saved").join("Config")));
    }
    candidates.sort();
    let mut unique_candidates = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if !unique_candidates
            .iter()
            .any(|existing: &PlatformCandidate| {
                same_path_identity(&existing.platform_dir, &candidate.platform_dir)
            })
        {
            unique_candidates.push(candidate);
        }
    }
    let existing = unique_candidates
        .into_iter()
        .filter(|candidate| is_plain_directory(&candidate.platform_dir))
        .collect::<Vec<_>>();
    if existing.is_empty() {
        return EngineIniResolution::PendingFirstLaunch;
    }
    if existing.len() == 1 {
        let candidate = &existing[0];
        if is_plain_file(&candidate.engine_ini) {
            return EngineIniResolution::Ready(candidate.engine_ini.clone());
        }
        return match std::fs::symlink_metadata(&candidate.engine_ini) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                EngineIniResolution::ReadyToCreate(candidate.engine_ini.clone())
            }
            Ok(_) | Err(_) => EngineIniResolution::Conflict,
        };
    }
    let with_engine = existing
        .iter()
        .filter(|candidate| is_plain_file(&candidate.engine_ini))
        .collect::<Vec<_>>();
    if with_engine.len() == 1 {
        return EngineIniResolution::Ready(with_engine[0].engine_ini.clone());
    }
    let with_user_settings = existing
        .iter()
        .filter(|candidate| is_plain_file(&candidate.game_user_settings))
        .collect::<Vec<_>>();
    if with_user_settings.len() == 1 {
        return EngineIniResolution::ReadyToCreate(with_user_settings[0].engine_ini.clone());
    }
    EngineIniResolution::Conflict
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PlatformCandidate {
    platform_dir: PathBuf,
    engine_ini: PathBuf,
    game_user_settings: PathBuf,
}

fn platform_candidates(config_root: &Path) -> Vec<PlatformCandidate> {
    UNREAL_CONFIG_PLATFORMS
        .iter()
        .map(|platform| {
            let platform_dir = config_root.join(platform);
            PlatformCandidate {
                engine_ini: platform_dir.join("Engine.ini"),
                game_user_settings: platform_dir.join("GameUserSettings.ini"),
                platform_dir,
            }
        })
        .collect()
}

fn is_safe_project_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && !name.contains(['\r', '\n'])
}

pub(crate) fn same_path_identity(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .replace('/', "\\")
            .eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn is_plain_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && !is_reparse_point(&metadata))
}

fn is_plain_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !is_reparse_point(&metadata))
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
