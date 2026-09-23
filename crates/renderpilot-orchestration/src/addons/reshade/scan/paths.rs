//! `ReShade.ini` path resolution and reads: where the effective base/add-on
//! paths are, and the generic `[ADDON]` config state a tool's own add-on-state
//! derivation interprets.

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use renderpilot_domain::Sha256Hash;
use serde::Serialize;

use crate::addons::ini::Ini;
use crate::addons::reshade::ini_schema::{
    ADDON_PATH_KEY, ADDON_SECTION, BASE_PATH_KEY, DISABLED_ADDONS_KEY, INSTALL_SECTION,
    LOAD_FROM_DLL_MAIN_KEY,
};

/// Conventional ReShade configuration file name, used when creating one.
pub const RESHADE_INI_FILE_NAME: &str = "ReShade.ini";
const RESHADE_INI: &str = RESHADE_INI_FILE_NAME;

/// Environment override ReShade honours for its base path (config/log/add-on
/// search root) when no `[INSTALL] BasePath` is set.
const RESHADE_BASE_PATH_OVERRIDE_ENV: &str = "RESHADE_BASE_PATH_OVERRIDE";

/// Effective ReShade paths derived from `ReShade.ini` and the host location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReshadePaths {
    /// Existing INI path, if present.
    pub ini_path: Option<PathBuf>,
    /// Effective base path used by ReShade.
    pub effective_base_path: PathBuf,
    /// Effective add-on search path.
    pub effective_addon_path: PathBuf,
    /// Whether `[ADDON] AddonPath` came from an absolute path.
    pub addon_path_is_absolute: bool,
}

/// The configuration inputs captured at the start of an operation.
///
/// The parsed INI is intentionally private: callers can use the derived paths
/// and pass the snapshot to the content classifier, but cannot accidentally
/// retain a mutable configuration model beyond this boundary.
pub(crate) struct ReshadeConfigSnapshot {
    paths: ReshadePaths,
    ini: Option<Ini>,
    retained_ini: Option<RetainedReshadeIni>,
}

/// The exact configuration file image retained while resolving ReShade paths.
///
/// This is deliberately tool-neutral.  A caller that needs ownership semantics
/// can project it into its own authority type, but no caller needs to read or
/// parse `ReShade.ini` a second time.
pub(crate) struct RetainedReshadeIni {
    path: PathBuf,
    bytes: Vec<u8>,
    identity: String,
    digest: Sha256Hash,
    length: u64,
    raw_addon_path_token: Option<String>,
}

impl RetainedReshadeIni {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    pub(crate) fn digest(&self) -> &Sha256Hash {
        &self.digest
    }

    pub(crate) const fn length(&self) -> u64 {
        self.length
    }

    pub(crate) fn raw_addon_path_token(&self) -> Option<&str> {
        self.raw_addon_path_token.as_deref()
    }
}

impl ReshadeConfigSnapshot {
    pub(crate) fn paths(&self) -> &ReshadePaths {
        &self.paths
    }

    pub(crate) fn retained_ini(&self) -> Option<&RetainedReshadeIni> {
        self.retained_ini.as_ref()
    }

    /// Resolves a caller-supplied immutable `AddonPath` overlay using the same
    /// base-path rules as the parsed configuration.  The snapshot remains the
    /// sole source of the base path and no configuration read is performed.
    pub(crate) fn effective_addon_path_with_overlay(
        &self,
        raw_addon_path: Option<&str>,
    ) -> PathBuf {
        raw_addon_path.map_or_else(
            || self.paths.effective_addon_path.clone(),
            |raw| resolve_config_path(&self.paths.effective_base_path, raw),
        )
    }

    pub(crate) fn assess_content(
        &self,
        game_dir: &Path,
        allowed_addon_names: &[&str],
    ) -> super::effects::ReshadeContent {
        super::effects::assess_reshade_content_from_snapshot(
            game_dir,
            &self.paths,
            self.ini.as_ref(),
            allowed_addon_names,
        )
    }
}

#[derive(Debug)]
pub(crate) enum ReshadeConfigSnapshotError {
    ReadDirectory { path: PathBuf, error: io::Error },
    ReadEntry { path: PathBuf, error: io::Error },
    InvalidEntry { path: PathBuf, reason: &'static str },
    ReadRetainedFile { path: PathBuf, error: String },
    InvalidUtf8 { path: PathBuf },
    InvalidDigest { path: PathBuf },
    InvalidLength { path: PathBuf },
}

impl fmt::Display for ReshadeConfigSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadDirectory { path, error } => {
                write!(
                    formatter,
                    "failed to inspect ReShade config directory `{}`: {error}",
                    path.display()
                )
            }
            Self::ReadEntry { path, error } => {
                write!(
                    formatter,
                    "failed to inspect ReShade config entry near `{}`: {error}",
                    path.display()
                )
            }
            Self::InvalidEntry { path, reason } => {
                write!(
                    formatter,
                    "ReShade configuration `{}` is {reason}",
                    path.display()
                )
            }
            Self::ReadRetainedFile { path, error } => {
                write!(
                    formatter,
                    "failed to read retained ReShade configuration `{}`: {error}",
                    path.display()
                )
            }
            Self::InvalidUtf8 { path } => {
                write!(
                    formatter,
                    "ReShade configuration `{}` is not valid UTF-8",
                    path.display()
                )
            }
            Self::InvalidDigest { path } => {
                write!(
                    formatter,
                    "ReShade configuration `{}` did not yield a valid digest",
                    path.display()
                )
            }
            Self::InvalidLength { path } => {
                write!(
                    formatter,
                    "ReShade configuration `{}` has an invalid byte length",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ReshadeConfigSnapshotError {}

/// Returns the path to an existing `ReShade.ini` in `game_dir`, matched
/// case-insensitively.
#[must_use]
pub fn reshade_ini_path(game_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(game_dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(RESHADE_INI)
        {
            return Some(entry.path());
        }
    }
    None
}

/// Resolves ReShade's effective base and add-on paths.
#[must_use]
pub fn resolve_paths(game_dir: &Path, host_path: Option<&Path>) -> ReshadePaths {
    let default_base = default_base(game_dir, host_path);
    let ini_path = reshade_ini_path(&default_base).or_else(|| reshade_ini_path(game_dir));
    let ini = ini_path.as_deref().and_then(load_ini);

    resolve_paths_from_ini(default_base, ini_path, ini.as_ref())
}

/// Resolves the ReShade paths and keeps the exact parsed configuration used to
/// derive them. Existing configuration entries that are not regular files,
/// links, unreadable, or non-UTF-8 are rejected instead of being treated as an
/// absent configuration.
pub(crate) fn resolve_strict_snapshot(
    game_dir: &Path,
    host_path: Option<&Path>,
) -> Result<ReshadeConfigSnapshot, ReshadeConfigSnapshotError> {
    let default_base = default_base(game_dir, host_path);
    let ini_path = strict_reshade_ini_path(&default_base, game_dir)?;
    let (ini, retained_ini) = match ini_path.as_deref() {
        Some(path) => {
            let (bytes, observation) = read_strict_config_file(path)?;
            let text = String::from_utf8(bytes.clone()).map_err(|_| {
                ReshadeConfigSnapshotError::InvalidUtf8 {
                    path: path.to_path_buf(),
                }
            })?;
            let digest = observation
                .digest
                .ok_or_else(|| ReshadeConfigSnapshotError::InvalidDigest {
                    path: path.to_path_buf(),
                })
                .and_then(|digest| {
                    Sha256Hash::new(digest).map_err(|_| ReshadeConfigSnapshotError::InvalidDigest {
                        path: path.to_path_buf(),
                    })
                })?;
            let length = u64::try_from(bytes.len()).map_err(|_| {
                ReshadeConfigSnapshotError::InvalidLength {
                    path: path.to_path_buf(),
                }
            })?;
            let ini = Ini::parse(&text);
            let retained_ini = RetainedReshadeIni {
                path: path.to_path_buf(),
                bytes,
                identity: observation.identity,
                digest,
                length,
                raw_addon_path_token: ini.get(ADDON_SECTION, ADDON_PATH_KEY).map(str::to_owned),
            };
            (Some(ini), Some(retained_ini))
        }
        None => (None, None),
    };
    let paths = resolve_paths_from_ini(default_base, ini_path, ini.as_ref());
    Ok(ReshadeConfigSnapshot {
        paths,
        ini,
        retained_ini,
    })
}

fn read_strict_config_file(
    path: &Path,
) -> Result<(Vec<u8>, crate::fs::EntryObservation), ReshadeConfigSnapshotError> {
    let (parent, leaf) = crate::fs::verified_parent(path).map_err(|error| {
        ReshadeConfigSnapshotError::ReadRetainedFile {
            path: path.to_path_buf(),
            error: error.to_string(),
        }
    })?;
    let (bytes, observation) = parent.read_regular_file(&leaf, None).map_err(|error| {
        ReshadeConfigSnapshotError::ReadRetainedFile {
            path: path.to_path_buf(),
            error: error.to_string(),
        }
    })?;
    Ok((bytes, observation))
}

fn default_base(game_dir: &Path, host_path: Option<&Path>) -> PathBuf {
    host_path
        .and_then(Path::parent)
        .unwrap_or(game_dir)
        .to_path_buf()
}

fn resolve_paths_from_ini(
    default_base: PathBuf,
    ini_path: Option<PathBuf>,
    ini: Option<&Ini>,
) -> ReshadePaths {
    let base_raw = ini
        .and_then(|ini| ini.get(INSTALL_SECTION, BASE_PATH_KEY))
        .map(str::to_owned)
        .or_else(|| env::var(RESHADE_BASE_PATH_OVERRIDE_ENV).ok());
    let effective_base_path = base_raw
        .as_deref()
        .map(|raw| resolve_config_path(&default_base, raw))
        .unwrap_or(default_base);

    // `[ADDON] AddonPath` is config-only — ReShade has no environment override for it.
    let addon_raw = ini
        .and_then(|ini| ini.get(ADDON_SECTION, ADDON_PATH_KEY))
        .map(str::to_owned);
    let addon_path_is_absolute = addon_raw
        .as_deref()
        .is_some_and(|raw| Path::new(raw.trim().trim_matches('"')).is_absolute());
    let effective_addon_path = addon_raw.as_deref().map_or_else(
        || effective_base_path.clone(),
        |raw| resolve_config_path(&effective_base_path, raw),
    );

    ReshadePaths {
        ini_path,
        effective_base_path,
        effective_addon_path,
        addon_path_is_absolute,
    }
}

fn strict_reshade_ini_path(
    default_base: &Path,
    game_dir: &Path,
) -> Result<Option<PathBuf>, ReshadeConfigSnapshotError> {
    let mut directories = vec![default_base];
    if !crate::paths::same_path(default_base, game_dir) {
        directories.push(game_dir);
    }

    for directory in directories {
        if let Some(path) = strict_ini_in_directory(directory)? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn strict_ini_in_directory(
    directory: &Path,
) -> Result<Option<PathBuf>, ReshadeConfigSnapshotError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ReshadeConfigSnapshotError::ReadDirectory {
                path: directory.to_path_buf(),
                error,
            });
        }
    };

    let mut found = None;
    for entry in entries {
        let entry = entry.map_err(|error| ReshadeConfigSnapshotError::ReadEntry {
            path: directory.to_path_buf(),
            error,
        })?;
        if !entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(RESHADE_INI)
        {
            continue;
        }

        let path = entry.path();
        let file_type =
            entry
                .file_type()
                .map_err(|error| ReshadeConfigSnapshotError::ReadEntry {
                    path: path.clone(),
                    error,
                })?;
        let reason = if file_type.is_symlink() {
            Some("a symbolic link")
        } else if !file_type.is_file() {
            Some("not a regular file")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ReshadeConfigSnapshotError::InvalidEntry { path, reason });
        }
        if found.is_some() {
            return Err(ReshadeConfigSnapshotError::InvalidEntry {
                path,
                reason: "ambiguous because multiple case variants exist",
            });
        }
        found = Some(path);
    }
    Ok(found)
}

/// Deletes `ReShade.log` and rotated `ReShade.log1..N` files near `base_path`.
pub fn remove_reshade_logs_best_effort(base_path: &Path) {
    for path in reshade_log_paths(base_path) {
        if let Err(error) = fs::remove_file(&path) {
            tracing::warn!(
                "failed to remove ReShade log `{}` during add-on cleanup: {error}",
                path.display()
            );
        }
    }
}

pub(super) fn reshade_log_paths(base_path: &Path) -> impl Iterator<Item = PathBuf> {
    let entries = fs::read_dir(base_path).ok();
    entries
        .into_iter()
        .flat_map(Iterator::flatten)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            is_reshade_log_name(&name).then(|| entry.path())
        })
}

fn is_reshade_log_name(name: &str) -> bool {
    if name == "reshade.log" {
        return true;
    }
    name.strip_prefix("reshade.log").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// Generic `[ADDON]` config state read from a `ReShade.ini`. Tool-agnostic; a
/// tool's own add-on-state derivation interprets these fields against its add-on
/// file name.
#[derive(Debug, Clone, Default)]
pub(crate) struct AddonConfigState {
    pub(crate) has_addon_section: bool,
    pub(crate) disabled_addons: Vec<String>,
    pub(crate) load_from_dll_main: Option<String>,
}

pub(crate) fn read_addon_config_state(ini: &Ini) -> AddonConfigState {
    AddonConfigState {
        has_addon_section: ini.has_section(ADDON_SECTION),
        disabled_addons: ini
            .get(ADDON_SECTION, DISABLED_ADDONS_KEY)
            .map(split_ini_list)
            .unwrap_or_default(),
        load_from_dll_main: ini
            .get(ADDON_SECTION, LOAD_FROM_DLL_MAIN_KEY)
            .map(str::to_owned),
    }
}

pub(crate) fn split_ini_list(value: &str) -> Vec<String> {
    value
        .split([',', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Reads and parses a `ReShade.ini`, returning `None` when it cannot be read.
pub(crate) fn load_ini(path: &Path) -> Option<Ini> {
    fs::read_to_string(path).ok().map(|text| Ini::parse(&text))
}

pub(super) fn resolve_config_path(base: &Path, raw: &str) -> PathBuf {
    let raw = raw.trim().trim_matches('"');
    let candidate = Path::new(raw);
    let path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    };
    crate::paths::canonicalize_best_effort(&path)
}
#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn resolve_paths_reads_base_and_relative_addon_path() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("base")).expect("base");
        fs::write(
            dir.path().join("ReShade.ini"),
            "[INSTALL]\r\nBasePath=base\r\n[ADDON]\r\nAddonPath=addons\r\n",
        )
        .expect("ini");

        let paths = resolve_paths(dir.path(), Some(&dir.path().join("dxgi.dll")));

        assert!(paths.effective_base_path.ends_with("base"));
        assert!(
            paths
                .effective_addon_path
                .ends_with(Path::new("base").join("addons"))
        );
        assert!(!paths.addon_path_is_absolute);
    }

    #[test]
    fn remove_reshade_logs_removes_rotated_logs_only() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("ReShade.log"), b"x").expect("log");
        fs::write(dir.path().join("reshade.log1"), b"x").expect("log1");
        fs::write(dir.path().join("reshade.log.old"), b"x").expect("old");

        remove_reshade_logs_best_effort(dir.path());

        assert!(!dir.path().join("ReShade.log").exists());
        assert!(!dir.path().join("reshade.log1").exists());
        assert!(dir.path().join("reshade.log.old").exists());
    }
}
