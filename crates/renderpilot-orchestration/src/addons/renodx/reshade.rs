//! ReShade host detection and configuration helpers for RenoDX.
//!
//! RenoDX is a ReShade add-on, so the question is no longer "did RenderPilot
//! install this host?" but "is the host the game will load a ReShade build with
//! full add-on support?". This module keeps that read-only host model and the
//! `ReShade.ini` reads it needs (path resolution, add-on state); the INI write
//! transforms install and DLSS-Fix apply live in [`super::reshade_ini`].

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_detection::{VersionIdentityStrings, inspect_pe};
use renderpilot_domain::Version;
use serde::Serialize;

use crate::addons::ini::Ini;

use super::reshade_ini::{
    ADDON_PATH_KEY, ADDON_SECTION, BASE_PATH_KEY, DISABLED_ADDONS_KEY, INSTALL_SECTION,
    LOAD_FROM_DLL_MAIN_KEY,
};

/// Legacy ownership sentinel from the pre-versioned host model. New installs do
/// not create it; install/uninstall flows remove it best-effort when encountered.
pub const MARKER_FILE_NAME: &str = "renderpilot-renodx.json";

/// Conventional ReShade configuration file name, used when creating one.
pub const RESHADE_INI_FILE_NAME: &str = "ReShade.ini";
const RESHADE_INI: &str = RESHADE_INI_FILE_NAME;

/// Environment override ReShade honours for its base path (config/log/add-on
/// search root) when no `[INSTALL] BasePath` is set.
const RESHADE_BASE_PATH_OVERRIDE_ENV: &str = "RESHADE_BASE_PATH_OVERRIDE";

const RESHADE_VERSION_EXPORT: &str = "ReShadeVersion";
const ADDON_API_EXPORTS: &[&str] = &[
    "ReShadeRegisterAddon",
    "ReShadeUnregisterAddon",
    "ReShadeRegisterEvent",
    "ReShadeUnregisterEvent",
    "ReShadeRegisterOverlay",
    "ReShadeGetImGuiFunctionTable",
];
const REQUIRED_ADDON_API_QUORUM: usize = 3;

const RESHADE_ENGINE_DLLS: &[&str] = &["reshade64.dll", "reshade32.dll"];
const PROXY_DLL_SLOTS: &[&str] = &[
    "dxgi.dll",
    "d3d9.dll",
    "d3d10.dll",
    "d3d10_1.dll",
    "d3d11.dll",
    "d3d12.dll",
    "opengl32.dll",
];

/// Current ReShade host state in a game folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReshadeHost {
    /// No ReShade-looking host was found.
    Absent,
    /// A host or occupied proxy slot is present.
    Present {
        /// Full path to the DLL.
        path: PathBuf,
        /// Proxy slot/file name where it was found.
        slot: String,
        /// Version-resource file version, if readable.
        version: Option<Version>,
        /// Whether the host exports the add-on API RenoDX requires.
        addon_support: ReshadeAddonSupport,
        /// Confidence that the DLL is actually ReShade.
        identity: ReshadeIdentity,
        /// Whether this slot is the one the resolved game executable will load.
        active: ActiveSlotState,
    },
}

impl ReshadeHost {
    /// Returns the present-host details, if any.
    #[must_use]
    pub fn as_present(&self) -> Option<ReshadeHostRef<'_>> {
        match self {
            Self::Absent => None,
            Self::Present {
                path,
                slot,
                version,
                addon_support,
                identity,
                active,
            } => Some(ReshadeHostRef {
                path,
                slot,
                version: version.as_ref(),
                addon_support: *addon_support,
                identity: *identity,
                active: *active,
            }),
        }
    }

    /// Whether the host is present and confidently identified as ReShade.
    #[must_use]
    pub fn is_usable_reshade(&self) -> bool {
        self.as_present()
            .is_some_and(|host| host.identity >= ReshadeIdentity::Probable)
    }

    /// Whether this host sits in the active proxy slot.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.as_present()
            .is_some_and(|host| host.active.state == SlotActivity::Active)
    }
}

/// Borrowed view over a [`ReshadeHost::Present`] payload.
#[derive(Debug, Clone, Copy)]
pub struct ReshadeHostRef<'a> {
    /// Full path to the DLL.
    pub path: &'a Path,
    /// Proxy slot/file name where it was found.
    pub slot: &'a str,
    /// Version-resource file version, if readable.
    pub version: Option<&'a Version>,
    /// Whether the host exports the add-on API RenoDX requires.
    pub addon_support: ReshadeAddonSupport,
    /// Confidence that the DLL is actually ReShade.
    pub identity: ReshadeIdentity,
    /// Whether this slot is the one the resolved game executable will load.
    pub active: ActiveSlotState,
}

/// Add-on API capability of the detected host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReshadeAddonSupport {
    /// The host exports the ReShade add-on API.
    Full,
    /// The host is ReShade but does not export the add-on API.
    None,
    /// Capability could not be determined.
    Unknown,
}

/// Confidence that a candidate DLL is ReShade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReshadeIdentity {
    /// A proxy slot is occupied, but ReShade identity is not established.
    Weak,
    /// Version-resource metadata or supporting files strongly point to ReShade.
    Probable,
    /// Export table contains `ReShadeVersion`.
    Confirmed,
}

/// Active-slot classification for a host candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ActiveSlotState {
    /// Whether this host is in the slot the resolved executable should load.
    pub state: SlotActivity,
    /// Why that classification was chosen.
    pub reason: ActiveSlotReason,
}

/// Whether a host slot is expected to load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotActivity {
    /// The slot matches the resolved proxy DLL.
    Active,
    /// Another slot is expected to load instead.
    Inactive,
    /// The active slot is not known.
    Ambiguous,
}

/// Explanation for active-slot classification. Kept minimal to the cases the
/// resolver can actually distinguish today (the active proxy slot is supplied by
/// the matcher, which already folds in import detection and bootstrap-exe
/// resolution); richer provenance can be added if a flow ever needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveSlotReason {
    /// The active slot came from the resolved-executable/matcher result.
    DetectedByMatcher,
    /// Dynamic loading or missing context left the active slot unknown.
    DynamicLoadUnknown,
}

/// Read-only scan of the game folder's ReShade-related DLLs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReshadeScan {
    /// Every ReShade-looking host or occupied active proxy slot found.
    pub hosts: Vec<ReshadeHost>,
}

impl ReshadeScan {
    /// Returns the single active host, if one can be identified.
    #[must_use]
    pub fn active_host(&self) -> Option<&ReshadeHost> {
        self.hosts.iter().find(|host| host.is_active())
    }

    /// Returns the hosts with at least probable ReShade identity.
    #[must_use]
    pub fn reshade_hosts(&self) -> Vec<&ReshadeHost> {
        self.hosts
            .iter()
            .filter(|host| host.is_usable_reshade())
            .collect()
    }

    /// Whether more than one ReShade host was found.
    #[must_use]
    pub fn has_multiple_reshade_hosts(&self) -> bool {
        self.reshade_hosts().len() > 1
    }

    /// Returns a compact host state for UI/DTO use.
    #[must_use]
    pub fn primary_host(&self) -> ReshadeHost {
        self.active_host()
            .cloned()
            .or_else(|| self.reshade_hosts().into_iter().next().cloned())
            .unwrap_or(ReshadeHost::Absent)
    }
}

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

/// State of the RenoDX add-on on disk/config for a detected host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenoDxAddonState {
    /// Whether a matching `renodx-*.addon*` file exists.
    pub present_on_disk: bool,
    /// The path the current install expects to use.
    pub expected_path: PathBuf,
    /// First matching add-on discovered in the effective add-on path.
    pub discovered_path: Option<PathBuf>,
    /// Whether `ReShade.ini` appears to allow the add-on to load. `None` means
    /// the config did not carry enough information to decide.
    pub enabled_by_config: Option<bool>,
    /// How ReShade is expected to load add-ons for this config.
    pub load_mode: RenoDxAddonLoadMode,
}

/// How the add-on will be discovered by ReShade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenoDxAddonLoadMode {
    /// ReShade will search the add-on directory.
    AutoSearch,
    /// `[ADDON] LoadFromDllMain` names an add-on.
    LoadFromDllMain,
    /// The mode cannot be determined.
    Unknown,
}

/// Policy action for a detected ReShade host relative to the desired version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReshadeHostAction {
    /// No safe automatic host action is available.
    Conflict,
    /// Replace with the full add-on-support build.
    ReinstallWithAddonSupport,
    /// Repair an unidentifiable or partially readable host.
    RepairHost,
    /// Update the active host to the desired version.
    UpdateHost,
    /// Host is suitable as-is.
    UpToDate,
}

impl ReshadeHostAction {
    /// Whether applying this policy action writes or replaces the ReShade host DLL.
    #[must_use]
    pub const fn writes_host(self) -> bool {
        matches!(
            self,
            Self::ReinstallWithAddonSupport | Self::RepairHost | Self::UpdateHost
        )
    }
}

/// Returns the path of the legacy marker within `game_dir`.
#[must_use]
pub fn marker_path(game_dir: &Path) -> PathBuf {
    game_dir.join(MARKER_FILE_NAME)
}

/// Removes the legacy ownership marker best-effort.
pub fn remove_legacy_marker(game_dir: &Path) {
    let path = marker_path(game_dir);
    if let Err(error) = fs::remove_file(&path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            log::warn!(
                "failed to remove legacy RenoDX marker `{}`: {error}",
                path.display()
            );
        }
    }
}

/// Detects ReShade hosts in `game_dir`, marking `active_proxy_slot` as the slot
/// the resolved executable should load when known.
#[must_use]
pub fn scan_reshade_hosts(game_dir: &Path, active_proxy_slot: Option<&str>) -> ReshadeScan {
    let mut hosts = Vec::new();
    let Ok(entries) = fs::read_dir(game_dir) else {
        return ReshadeScan { hosts };
    };

    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let lower = file_name.to_ascii_lowercase();
        // Only DLLs that could actually be the ReShade host are worth the PE reads:
        // a known proxy slot, the ReShade engine DLL, or a `reshade*` name. This
        // skips the dozens of unrelated game DLLs in a typical install folder.
        if !lower.ends_with(".dll") || !is_host_candidate(&lower) {
            continue;
        }
        let Some(host) =
            inspect_host_candidate(game_dir, entry.path(), &file_name, active_proxy_slot)
        else {
            continue;
        };
        hosts.push(host);
    }

    ReshadeScan { hosts }
}

fn inspect_host_candidate(
    game_dir: &Path,
    path: PathBuf,
    file_name: &str,
    active_proxy_slot: Option<&str>,
) -> Option<ReshadeHost> {
    let lower = file_name.to_ascii_lowercase();
    let is_known_slot = is_proxy_slot(&lower) || is_reshade_engine_dll(&lower);
    let is_active_slot = active_proxy_slot.is_some_and(|slot| lower.eq_ignore_ascii_case(slot));

    // Read the candidate DLL once; derive every PE fact from the single buffer
    // (a host folder scan otherwise re-read each DLL once per field).
    let inspection = inspect_pe(&path);
    let export_list = inspection
        .as_ref()
        .and_then(|pe| pe.export_names.as_deref());
    let has_reshade_export = export_list
        .unwrap_or(&[])
        .iter()
        .any(|name| name.eq_ignore_ascii_case(RESHADE_VERSION_EXPORT));
    let metadata_points_to_reshade = inspection
        .as_ref()
        .is_some_and(|pe| version_strings_point_to_reshade(&pe.identity));

    let identity = if has_reshade_export {
        Some(ReshadeIdentity::Confirmed)
    } else if metadata_points_to_reshade
        || is_reshade_engine_dll(&lower)
        || (is_proxy_slot(&lower) && has_neighboring_reshade_files(game_dir))
    {
        Some(ReshadeIdentity::Probable)
    } else if is_active_slot || (is_known_slot && lower.starts_with("reshade")) {
        Some(ReshadeIdentity::Weak)
    } else {
        None
    }?;

    let addon_support = addon_support_from_exports(export_list, has_reshade_export);
    let active = active_slot_state(&lower, active_proxy_slot);
    let version = inspection.as_ref().and_then(|pe| pe.version.clone());

    Some(ReshadeHost::Present {
        path,
        slot: file_name.to_owned(),
        version,
        addon_support,
        identity,
        active,
    })
}

fn addon_support_from_exports(
    exports: Option<&[String]>,
    has_reshade_export: bool,
) -> ReshadeAddonSupport {
    let Some(exports) = exports else {
        return ReshadeAddonSupport::Unknown;
    };
    let addon_api_count = ADDON_API_EXPORTS
        .iter()
        .filter(|expected| {
            exports
                .iter()
                .any(|name| name.eq_ignore_ascii_case(expected))
        })
        .count();

    if addon_api_count >= REQUIRED_ADDON_API_QUORUM {
        ReshadeAddonSupport::Full
    } else if has_reshade_export {
        ReshadeAddonSupport::None
    } else {
        ReshadeAddonSupport::Unknown
    }
}

fn active_slot_state(slot: &str, active_proxy_slot: Option<&str>) -> ActiveSlotState {
    match active_proxy_slot {
        Some(active) if slot.eq_ignore_ascii_case(active) => ActiveSlotState {
            state: SlotActivity::Active,
            reason: ActiveSlotReason::DetectedByMatcher,
        },
        Some(_) => ActiveSlotState {
            state: SlotActivity::Inactive,
            reason: ActiveSlotReason::DetectedByMatcher,
        },
        None => ActiveSlotState {
            state: SlotActivity::Ambiguous,
            reason: ActiveSlotReason::DynamicLoadUnknown,
        },
    }
}

fn version_strings_point_to_reshade(strings: &VersionIdentityStrings) -> bool {
    let values = [
        strings.product_name.as_deref(),
        strings.file_description.as_deref(),
        strings.original_filename.as_deref(),
        strings.company_name.as_deref(),
    ];
    values.into_iter().flatten().any(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("reshade") || value.contains("crosire")
    })
}

fn has_neighboring_reshade_files(game_dir: &Path) -> bool {
    reshade_ini_path(game_dir).is_some() || reshade_log_paths(game_dir).next().is_some()
}

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
    let default_base = host_path
        .and_then(Path::parent)
        .unwrap_or(game_dir)
        .to_path_buf();
    let ini_path = reshade_ini_path(&default_base).or_else(|| reshade_ini_path(game_dir));
    let ini = ini_path.as_deref().and_then(load_ini);

    let base_raw = ini
        .as_ref()
        .and_then(|ini| ini.get(INSTALL_SECTION, BASE_PATH_KEY))
        .map(str::to_owned)
        .or_else(|| env::var(RESHADE_BASE_PATH_OVERRIDE_ENV).ok());
    let effective_base_path = base_raw
        .as_deref()
        .map(|raw| resolve_config_path(&default_base, raw))
        .unwrap_or(default_base);

    // `[ADDON] AddonPath` is config-only — ReShade has no environment override for it.
    let addon_raw = ini
        .as_ref()
        .and_then(|ini| ini.get(ADDON_SECTION, ADDON_PATH_KEY))
        .map(str::to_owned);
    let addon_path_is_absolute = addon_raw
        .as_deref()
        .is_some_and(|raw| Path::new(raw.trim().trim_matches('"')).is_absolute());
    let effective_addon_path = addon_raw
        .as_deref()
        .map(|raw| resolve_config_path(&effective_base_path, raw))
        .unwrap_or_else(|| effective_base_path.clone());

    ReshadePaths {
        ini_path,
        effective_base_path,
        effective_addon_path,
        addon_path_is_absolute,
    }
}

/// Returns whether a path is too risky for an implicit add-on write.
#[must_use]
pub fn addon_path_requires_explicit_elevation(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    normalized.starts_with(r"c:\windows")
        || normalized.contains(r"\windows\")
        || normalized.starts_with(r"c:\program files")
        || normalized.starts_with(r"c:\program files (x86)")
}

/// Computes the RenoDX add-on state from disk and `ReShade.ini`.
#[must_use]
pub fn renodx_addon_state(paths: &ReshadePaths, addon_file_name: &str) -> RenoDxAddonState {
    let expected_path = paths.effective_addon_path.join(addon_file_name);
    let discovered_path = discover_renodx_addon(&paths.effective_addon_path);
    let ini_state = paths
        .ini_path
        .as_deref()
        .and_then(load_ini)
        .map(|ini| read_addon_config_state(&ini));
    // Soft heuristic: ReShade's `DisabledAddons` lists add-on *titles*, which for
    // RenoDX usually match the add-on file stem; we compare against the file name
    // (expected and discovered) as the best signal available without loading the
    // add-on. Used only for an informational "disabled" hint, never to gate logic.
    let enabled_by_config = ini_state.as_ref().and_then(|state| {
        if state.disabled_addons.iter().any(|value| {
            value.eq_ignore_ascii_case(addon_file_name)
                || discovered_path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .is_some_and(|name| value.eq_ignore_ascii_case(&name.to_string_lossy()))
        }) {
            Some(false)
        } else if state.has_addon_section {
            Some(true)
        } else {
            None
        }
    });
    let load_mode = ini_state
        .as_ref()
        .map(|state| {
            if state.load_from_dll_main.is_some() {
                RenoDxAddonLoadMode::LoadFromDllMain
            } else if state.has_addon_section || paths.ini_path.is_none() {
                RenoDxAddonLoadMode::AutoSearch
            } else {
                RenoDxAddonLoadMode::Unknown
            }
        })
        .unwrap_or(RenoDxAddonLoadMode::AutoSearch);

    RenoDxAddonState {
        present_on_disk: expected_path.is_file() || discovered_path.is_some(),
        expected_path,
        discovered_path,
        enabled_by_config,
        load_mode,
    }
}

/// Applies the strict structural host policy.
///
/// An absent host yields [`ReshadeHostAction::UpdateHost`] — "a host must be
/// written" — which the install flow treats as "install a fresh host" and the
/// update flow as "place the managed host". Version resources are display-only:
/// freshness is decided by channel-source digest checks.
#[must_use]
pub fn host_action(host: &ReshadeHost) -> ReshadeHostAction {
    let Some(host) = host.as_present() else {
        return ReshadeHostAction::UpdateHost;
    };
    if host.active.state != SlotActivity::Active || host.identity < ReshadeIdentity::Probable {
        return ReshadeHostAction::Conflict;
    }
    match host.addon_support {
        ReshadeAddonSupport::None => return ReshadeHostAction::ReinstallWithAddonSupport,
        ReshadeAddonSupport::Unknown => return ReshadeHostAction::RepairHost,
        ReshadeAddonSupport::Full => {}
    }
    ReshadeHostAction::UpToDate
}

/// Deletes `ReShade.log` and rotated `ReShade.log1..N` files near `base_path`.
pub fn remove_reshade_logs_best_effort(base_path: &Path) {
    for path in reshade_log_paths(base_path) {
        if let Err(error) = fs::remove_file(&path) {
            log::warn!(
                "failed to remove ReShade log `{}` during RenoDX cleanup: {error}",
                path.display()
            );
        }
    }
}

fn reshade_log_paths(base_path: &Path) -> impl Iterator<Item = PathBuf> {
    let entries = fs::read_dir(base_path).ok();
    entries
        .into_iter()
        .flat_map(|entries| entries.flatten())
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

fn discover_renodx_addon(addon_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(addon_dir).ok()?;
    entries.flatten().find_map(|entry| {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            return None;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        (name.starts_with("renodx-") && (name.ends_with(".addon64") || name.ends_with(".addon32")))
            .then(|| entry.path())
    })
}

#[derive(Debug, Clone, Default)]
struct AddonConfigState {
    has_addon_section: bool,
    disabled_addons: Vec<String>,
    load_from_dll_main: Option<String>,
}

fn read_addon_config_state(ini: &Ini) -> AddonConfigState {
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

fn split_ini_list(value: &str) -> Vec<String> {
    value
        .split([',', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Reads and parses a `ReShade.ini`, returning `None` when it cannot be read.
fn load_ini(path: &Path) -> Option<Ini> {
    fs::read_to_string(path).ok().map(|text| Ini::parse(&text))
}

fn resolve_config_path(base: &Path, raw: &str) -> PathBuf {
    let raw = raw.trim().trim_matches('"');
    let candidate = Path::new(raw);
    let path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    };
    canonicalize_best_effort(&path)
}

fn canonicalize_best_effort(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Path equality after best-effort canonicalization, so `.`/relative forms and
/// symlinks compare equal when the targets exist on disk.
#[must_use]
pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    canonicalize_best_effort(left) == canonicalize_best_effort(right)
}

/// Whether a DLL name is plausibly the ReShade host (worth a PE inspection): a
/// known proxy slot, the ReShade engine DLL, or a `reshade*`-named file.
fn is_host_candidate(lower_name: &str) -> bool {
    is_proxy_slot(lower_name)
        || is_reshade_engine_dll(lower_name)
        || lower_name.starts_with("reshade")
}

/// Whether `name` is one of the proxy-DLL slots a ReShade host can occupy. The
/// single source of truth reused by the install/update record helpers.
#[must_use]
pub(super) fn is_proxy_slot(name: &str) -> bool {
    PROXY_DLL_SLOTS
        .iter()
        .any(|slot| name.eq_ignore_ascii_case(slot))
}

fn is_reshade_engine_dll(name: &str) -> bool {
    RESHADE_ENGINE_DLLS
        .iter()
        .any(|slot| name.eq_ignore_ascii_case(slot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn detect_returns_absent_for_clean_folder() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("game.exe"), b"x").expect("write");
        assert_eq!(
            scan_reshade_hosts(dir.path(), None).primary_host(),
            ReshadeHost::Absent
        );
    }

    #[test]
    fn active_proxy_slot_without_reshade_identity_is_weak_conflict_signal() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("dxgi.dll"), b"not-a-pe").expect("write");
        let scan = scan_reshade_hosts(dir.path(), Some("dxgi.dll"));

        let host = scan.active_host().expect("active slot");
        let details = host.as_present().expect("present");
        assert_eq!(details.identity, ReshadeIdentity::Weak);
        assert_eq!(details.active.state, SlotActivity::Active);
    }

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
    fn renodx_addon_state_reports_disabled_by_config() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("ReShade.ini"),
            "[ADDON]\r\nDisabledAddons=renodx-cp2077.addon64\r\n",
        )
        .expect("ini");
        fs::write(dir.path().join("renodx-cp2077.addon64"), b"x").expect("addon");
        let paths = resolve_paths(dir.path(), Some(&dir.path().join("dxgi.dll")));

        let state = renodx_addon_state(&paths, "renodx-cp2077.addon64");

        assert!(state.present_on_disk);
        assert_eq!(state.enabled_by_config, Some(false));
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

    fn present_host(
        addon_support: ReshadeAddonSupport,
        identity: ReshadeIdentity,
        state: SlotActivity,
        version: Option<&str>,
    ) -> ReshadeHost {
        ReshadeHost::Present {
            path: PathBuf::from("dxgi.dll"),
            slot: "dxgi.dll".to_owned(),
            version: version.map(|v| Version::parse(v).expect("version")),
            addon_support,
            identity,
            active: ActiveSlotState {
                state,
                reason: ActiveSlotReason::DetectedByMatcher,
            },
        }
    }

    #[test]
    fn host_action_follows_strict_precedence() {
        use ReshadeHostAction as A;

        // Absent → a host must be written.
        assert_eq!(host_action(&ReshadeHost::Absent), A::UpdateHost);
        // Inactive slot → conflict, regardless of how good the host otherwise is.
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Full,
                ReshadeIdentity::Confirmed,
                SlotActivity::Inactive,
                Some("6.6.0"),
            )),
            A::Conflict
        );
        // Active slot but not confidently ReShade → conflict.
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Full,
                ReshadeIdentity::Weak,
                SlotActivity::Active,
                Some("6.6.0"),
            )),
            A::Conflict
        );
        // No add-on support → reinstall.
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::None,
                ReshadeIdentity::Confirmed,
                SlotActivity::Active,
                Some("1.0.0"),
            )),
            A::ReinstallWithAddonSupport
        );
        // Unknown add-on support → repair.
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Unknown,
                ReshadeIdentity::Probable,
                SlotActivity::Active,
                Some("6.6.0"),
            )),
            A::RepairHost
        );
        // Full add-on support is usable even when the display version is unreadable.
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Full,
                ReshadeIdentity::Confirmed,
                SlotActivity::Active,
                None,
            )),
            A::UpToDate
        );
        // Version is display-only; freshness is digest/validator-driven elsewhere.
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Full,
                ReshadeIdentity::Confirmed,
                SlotActivity::Active,
                Some("1.0.0"),
            )),
            A::UpToDate
        );
        // Full + modern version is also usable.
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Full,
                ReshadeIdentity::Confirmed,
                SlotActivity::Active,
                Some("6.6.0"),
            )),
            A::UpToDate
        );
    }
}
