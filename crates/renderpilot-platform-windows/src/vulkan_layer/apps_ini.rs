use std::io;
use std::path::{Path, PathBuf};

use super::util::same_path;
use super::{APPS_INI_NAME, APPS_KEY};

/// Reads the list of registered app executable paths from `ReShadeApps.ini`.
/// Returns an empty vector if the file does not exist.
///
/// # Errors
/// Propagates filesystem errors other than the file being absent — notably a
/// permission error is reported rather than silently treated as an empty list,
/// since callers use "empty" to justify registering a fresh list or deleting
/// the shared layer.
pub(crate) fn read_app_list(layer_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let ini_path = layer_dir.join(APPS_INI_NAME);
    let content = match std::fs::read_to_string(&ini_path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Apps=") {
            return Ok(value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .collect());
        }
    }
    Ok(Vec::new())
}

/// Adds `exe_path` to `ReShadeApps.ini` if not already present.
/// Creates the file (and directory) if needed.
///
/// # Errors
/// Propagates filesystem errors. In particular, an unreadable existing
/// `ReShadeApps.ini` fails the call instead of being treated as empty, which
/// would otherwise silently overwrite the list of apps sharing the layer.
pub fn register_app(layer_dir: &Path, exe_path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(layer_dir)?;
    let mut apps = dedupe_app_list(read_app_list(layer_dir)?);
    if !apps.iter().any(|p| same_path(p, exe_path)) {
        apps.push(exe_path.to_path_buf());
    }
    write_app_list(layer_dir, &apps)
}

/// Removes `exe_path` from `ReShadeApps.ini`. Returns `true` if the list is
/// now empty or no app-list exists (the caller should delete the standard shared
/// layer). Missing executables are pruned only when their disk/root appears
/// available; entries on offline drives are preserved.
///
/// # Errors
/// Propagates filesystem errors. In particular, an unreadable existing
/// `ReShadeApps.ini` fails the call instead of being treated as empty, which
/// would otherwise report `true` and have the caller delete a shared layer
/// that other games are still registered against.
pub fn unregister_app(layer_dir: &Path, exe_path: &Path) -> io::Result<bool> {
    let ini_path = layer_dir.join(APPS_INI_NAME);
    if !ini_path.is_file() {
        return Ok(true);
    }
    let mut apps = prune_stale_apps(read_app_list(layer_dir)?);
    apps.retain(|p| !same_path(p, exe_path));
    if apps.is_empty() {
        match std::fs::remove_file(&ini_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(true)
    } else {
        write_app_list(layer_dir, &apps)?;
        Ok(false)
    }
}

/// Writes `apps` to `ReShadeApps.ini` via a same-directory temp file plus
/// rename, so a crash or concurrent read never observes a truncated file.
///
/// Temp path is the fixed sibling basename `ReShadeApps.ini.tmp` (no extension
/// arithmetic), so the destination always keeps the `.ini` suffix.
pub(crate) fn write_app_list(layer_dir: &Path, apps: &[PathBuf]) -> io::Result<()> {
    let ini_path = layer_dir.join(APPS_INI_NAME);
    // Fixed basenames — do not derive via Path::with_extension / add_extension.
    let tmp_path = layer_dir.join(format!("{APPS_INI_NAME}.tmp"));
    std::fs::create_dir_all(layer_dir)?;
    let joined = apps
        .iter()
        .map(|p| p.to_string_lossy().replace('/', "\\"))
        .collect::<Vec<_>>()
        .join(",");
    let content = format!("{APPS_KEY}={joined}\n");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, &ini_path)
}

fn dedupe_app_list(apps: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut normalized: Vec<PathBuf> = Vec::with_capacity(apps.len());
    for app in apps {
        if app.as_os_str().is_empty() {
            continue;
        }
        if !normalized.iter().any(|existing| same_path(existing, &app)) {
            normalized.push(app);
        }
    }
    normalized
}

fn prune_stale_apps(apps: Vec<PathBuf>) -> Vec<PathBuf> {
    dedupe_app_list(apps)
        .into_iter()
        .filter(|app| app_is_present_or_on_unavailable_root(app))
        .collect()
}

fn app_is_present_or_on_unavailable_root(path: &Path) -> bool {
    path.is_file() || !path_root_is_available(path)
}

fn path_root_is_available(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};

        match path.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    let drive = format!("{}:\\", char::from(letter));
                    Path::new(&drive).exists()
                }
                Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                    PathBuf::from(format!(
                        r"\\{}\{}",
                        server.to_string_lossy(),
                        share.to_string_lossy()
                    ))
                    .exists()
                }
                _ => path.parent().is_some_and(Path::exists),
            },
            Some(Component::RootDir) => true,
            _ => path.parent().is_some_and(Path::exists),
        }
    }
    #[cfg(not(windows))]
    {
        path.parent().is_some_and(Path::exists)
    }
}

// -----------------------------------------------------------------------------
// Manifest generation (matches official ReShade format)
// -----------------------------------------------------------------------------
