//! Turns a fetched RenoDX install into a generic [`InstallPlan`] for the shared
//! [`addons::engine`](crate::addons::engine) and maps the result back to a record.
//!
//! Given a [`PreparedInstall`] (already-fetched, already-verified bytes plus the
//! resolved file names), [`install`] builds the ordered file operations — the
//! add-on and ReShade host DLL via a no-backup [`FileOp::Replace`] (both are
//! rolling snapshots or official redistributables RenoDX already PE-checked, so
//! nothing about a prior version is worth preserving), `ReShade.ini` via a
//! no-backup [`FileOp::UpdateText`] merge (it may carry the user's own hand-tuned
//! ReShade settings) — hands them to the engine (which rolls back and journals
//! each op per its own backup policy) and assembles the reversible
//! [`InstalledAddon`] with the upstream sources to track for updates.
//! [`uninstall`] reverses everything the engine's generic list-based reversal
//! covers, plus bespoke handling for `ReShade.ini` (see its own docs) since a
//! config merge is never something that reversal alone can undo correctly.
//! (In-place file replacement for updates/channel switches lives in
//! [`super::use_cases::reshade_update`], shared by the update and channel-switch
//! commands, which drive the engine directly only for a host moving to a new slot.)
//!
//! All ReShade/HDR specifics live here; the filesystem mechanics are the engine's.

use std::path::{Path, PathBuf};

use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, InstalledAddonHostKind, PathRef,
    TrackedSource, TrackedSourceRole,
};

use crate::ServiceError;
use crate::addons::engine::{self, FileOp, InstallPlan};
use crate::addons::record;

use super::errors;
use super::host_policy::{self, HostAssessment};
use super::policy::HostKind;
use super::reshade;
use super::reshade_ini::{ini_merge_strategy, ini_remove_renodx_strategy};
use super::tracking;
use super::types::{ReshadeChannel, ReshadeIniTweaks};
use super::use_cases::reshade_update::host_binary_source;

/// The DLSS-Fix companion add-on file name prefix (`renodx-dlssfix.`).
pub(super) const DLSS_FIX_FILE_PREFIX: &str = "renodx-dlssfix.";

/// The DLSS-Fix add-on file name for `arch` (e.g. `renodx-dlssfix.addon64`).
#[must_use]
pub(super) fn dlss_fix_file_name(arch: Architecture) -> String {
    format!("{DLSS_FIX_FILE_PREFIX}{}", arch.addon_extension())
}

/// The path of the DLSS-Fix companion add-on within the record's `created_files`,
/// if one is installed (matched by the [`DLSS_FIX_FILE_PREFIX`] file name).
#[must_use]
pub(super) fn dlss_fix_file_path(record: &InstalledAddon) -> Option<PathBuf> {
    record
        .created_files()
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|n| n.starts_with(DLSS_FIX_FILE_PREFIX))
        })
        .map(|f| PathBuf::from(f.as_str()))
}

/// Everything the engine needs to lay down an install, with payloads already
/// downloaded and integrity-verified by the fetch layer.
#[derive(Debug, Clone)]
pub struct PreparedInstall {
    /// Game the install belongs to.
    pub game_id: GameId,
    /// How RenoDX hooks into this game: a per-game proxy DLL or the shared Vulkan
    /// layer. Selects which per-game file layout [`install`] lays down.
    pub host_kind: HostKind,
    /// Proxy DLL file name to install the ReShade host as (for example `dxgi.dll`).
    /// Used only for [`HostKind::Proxy`]; empty for a Vulkan install.
    pub proxy_dll_name: String,
    /// Add-on file name to place (for example `renodx-cp2077.addon64`).
    pub addon_file_name: String,
    /// Upstream URL the add-on was fetched from (empty for a file install).
    pub addon_source_url: String,
    /// SHA-256 of the add-on bytes — the durable change-detection digest.
    pub source_digest: String,
    /// HTTP cache validator for a cheap update pre-check.
    pub source_etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string of the add-on, when the host sent one.
    pub source_last_modified: Option<String>,
    /// Add-on payload bytes.
    pub addon_bytes: Vec<u8>,
    /// ReShade host DLL bytes; used only when no ReShade host is already present.
    pub reshade_dll_bytes: Vec<u8>,
    /// Upstream URL the ReShade host came from (empty when no host is installed).
    pub reshade_source_url: String,
    /// The ReShade host zip's cache validator (for a cheap host update pre-check).
    pub reshade_source_etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string of the ReShade host zip, when sent.
    pub reshade_last_modified: Option<String>,
    /// SHA-256 of the installed ReShade host DLL (empty when none installed).
    pub reshade_digest: String,
    /// Effective channel for a recorded ReShade host artifact.
    pub reshade_channel: Option<ReshadeChannel>,
    /// `ReShade.ini` tweaks RenoDX requires.
    pub ini_tweaks: ReshadeIniTweaks,
}

/// Installs RenoDX into `game_dir`, returning the record needed to reverse it.
///
/// Dispatches on the host kind: a [`HostKind::Proxy`] install lays down a per-game
/// ReShade proxy DLL (or reuses a compatible detected host); a [`HostKind::Vulkan`] install lays
/// down only per-game files — the shared Vulkan layer is handled separately by the
/// service. The engine rolls back on any failure.
pub fn install(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<InstalledAddon, ServiceError> {
    match prepared.host_kind {
        HostKind::Proxy => install_proxy(game_dir, prepared),
        HostKind::Vulkan => install_vulkan(game_dir, prepared),
    }
}

/// Installs a Direct3D (proxy-DLL) RenoDX host.
///
/// Refuses if a RenoDX install record already exists here (the caller should
/// uninstall first to reinstall). When no ReShade host is present one is installed
/// (proxy DLL + fresh `ReShade.ini` + marker); a compatible detected host is reused
/// with only an additive, backed-up `ReShade.ini` merge.
fn install_proxy(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<InstalledAddon, ServiceError> {
    let host = host_policy::assess(game_dir, &prepared.proxy_dll_name);
    host.ensure_not_conflicting(&prepared.proxy_dll_name)?;
    if host.writes_host() && prepared.reshade_dll_bytes.is_empty() {
        return Err(errors::invalid(
            "the active ReShade host needs installation or repair, but no ReShade bytes were provided"
                .to_owned(),
        ));
    }

    let paths = reshade::resolve_paths(game_dir, Some(&host.target_path));
    if reshade::addon_path_requires_explicit_elevation(&paths.effective_addon_path) {
        return Err(errors::invalid(format!(
            "ReShade AddonPath `{}` points to a protected system location; move the add-on path \
             or run an explicit elevated install flow",
            paths.effective_addon_path.display()
        )));
    }
    if !paths.effective_addon_path.is_dir() {
        return Err(errors::invalid(format!(
            "ReShade AddonPath `{}` does not exist",
            paths.effective_addon_path.display()
        )));
    }

    let receipt = install_plans(game_dir, &paths.effective_addon_path, prepared, &host)?;
    build_record(
        prepared,
        &paths.effective_addon_path,
        host.writes_host(),
        &receipt,
    )
}

fn install_plans(
    game_dir: &Path,
    addon_dir: &Path,
    prepared: &PreparedInstall,
    host: &HostAssessment,
) -> Result<engine::InstallReceipt, ServiceError> {
    if reshade::same_path(game_dir, addon_dir) {
        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: combined_ops(game_dir, prepared, host.writes_host()),
        };
        return engine::install(game_dir, &plan);
    }

    let addon_plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![addon_op(prepared)],
    };
    let addon_receipt = engine::install(addon_dir, &addon_plan)?;

    let host_ops = host_ops(game_dir, prepared, host.writes_host());
    let host_receipt = if !host_ops.is_empty() {
        let host_plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: host_ops,
        };
        match engine::install(game_dir, &host_plan) {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Err(revert_error) =
                    engine::uninstall(&addon_receipt.created_files, &addon_receipt.backed_up_files)
                {
                    log::warn!(
                        "RenoDX install: host install failed and the add-on rollback also failed: {revert_error}"
                    );
                }
                return Err(error);
            }
        }
    } else {
        engine::InstallReceipt::default()
    };

    Ok(merge_receipts(addon_receipt, host_receipt))
}

fn merge_receipts(
    mut left: engine::InstallReceipt,
    right: engine::InstallReceipt,
) -> engine::InstallReceipt {
    left.created_files.extend(right.created_files);
    left.backed_up_files.extend(right.backed_up_files);
    left
}

fn combined_ops(game_dir: &Path, prepared: &PreparedInstall, writes_host: bool) -> Vec<FileOp> {
    let mut ops = vec![addon_op(prepared)];
    if writes_host {
        ops.push(host_op(prepared));
    }
    if let Some(ini_op) = ini_op_for_game(game_dir, &prepared.ini_tweaks) {
        ops.push(ini_op);
    }
    ops
}

/// The RenoDX add-on file op: a rolling upstream snapshot RenoDx already
/// PE-sanity-checked, so a pre-existing file at that path (a prior install) is
/// simply overwritten — nothing about the old bytes is worth preserving.
fn addon_op(prepared: &PreparedInstall) -> FileOp {
    FileOp::Replace {
        name: prepared.addon_file_name.clone(),
        bytes: prepared.addon_bytes.clone(),
    }
}

/// The ReShade host DLL op: an official redistributable RenoDx fetched itself,
/// so a pre-existing file in that slot is overwritten with no on-disk backup —
/// its identity is confirmed by [`host_policy::assess`] before this ever runs.
fn host_op(prepared: &PreparedInstall) -> FileOp {
    FileOp::Replace {
        name: prepared.proxy_dll_name.clone(),
        bytes: prepared.reshade_dll_bytes.clone(),
    }
}

fn host_ops(game_dir: &Path, prepared: &PreparedInstall, writes_host: bool) -> Vec<FileOp> {
    let mut ops = Vec::new();
    if writes_host {
        ops.push(host_op(prepared));
    }
    if let Some(ini_op) = ini_op_for_game(game_dir, &prepared.ini_tweaks) {
        ops.push(ini_op);
    }
    ops
}

/// The `ReShade.ini` merge operation: additively set RenoDX's `[ADDON]` keys,
/// creating the file from empty when none exists. Uses `UpdateText` rather than
/// `MergeText` — RenoDX never keeps a `.bak` of a config file that may carry the
/// user's own hand-tuned ReShade settings. The engine itself tracks a from-empty
/// write as `created_files` (see `engine::InstallChanges::into_receipt`), so
/// `install_plans`/`build_vulkan_plan` need no extra book-keeping for it.
fn ini_op_for_game(game_dir: &Path, tweaks: &ReshadeIniTweaks) -> Option<FileOp> {
    let tweaks = effective_ini_tweaks(game_dir, tweaks);
    ini_tweaks_write_keys(&tweaks).then(|| FileOp::UpdateText {
        name: reshade::RESHADE_INI_FILE_NAME.to_owned(),
        default: String::new(),
        strategy: ini_merge_strategy(&tweaks),
    })
}

fn effective_ini_tweaks(game_dir: &Path, tweaks: &ReshadeIniTweaks) -> ReshadeIniTweaks {
    let mut effective = tweaks.clone();
    if reshade::has_user_effect_assets(game_dir) {
        effective.disabled_addons.clear();
    }
    effective
}

fn ini_tweaks_write_keys(tweaks: &ReshadeIniTweaks) -> bool {
    !tweaks.disabled_addons.is_empty() || tweaks.addon_path.is_some() || tweaks.dlss_fix.is_some()
}

/// Assembles the [`InstalledAddon`] from the engine receipt and the upstream entries
/// to track: the add-on (when fetched upstream) and any replaced/created ReShade host.
fn build_record(
    prepared: &PreparedInstall,
    addon_dir: &Path,
    tracks_host: bool,
    receipt: &engine::InstallReceipt,
) -> Result<InstalledAddon, ServiceError> {
    let addon_path = addon_dir.join(&prepared.addon_file_name);

    let mut sources = Vec::new();
    // A file install has no upstream add-on URL; it may still keep a local-date
    // placeholder so the UI has a DB fallback if file mtime stamping fails.
    if !prepared.addon_source_url.is_empty() || prepared.source_last_modified.is_some() {
        sources.push(
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                prepared.addon_source_url.clone(),
                prepared.source_etag.clone(),
                prepared.source_digest.clone(),
            )
            .with_last_modified(prepared.source_last_modified.clone()),
        );
    }
    // A host source records that this install replaced or created the active host,
    // so update/uninstall can reverse it. A reused suitable host records none.
    if tracks_host {
        sources.push(host_binary_source(
            prepared.reshade_source_url.clone(),
            prepared.reshade_source_etag.clone(),
            prepared.reshade_digest.clone(),
            prepared.reshade_last_modified.clone(),
            prepared.reshade_channel,
        ));
    }

    record::build(
        prepared.game_id.clone(),
        AddonKind::RenoDx,
        &addon_path,
        receipt,
        sources,
    )
}

/// The upstream add-on entry to track for updates, or `None` for a file install
/// (empty URL and no mtime placeholder) which has nothing to track. Shared by the
/// proxy and Vulkan record builders. A file install with a `source_last_modified`
/// placeholder (but no URL) still records it so the UI has a DB fallback if file
/// mtime stamping fails — mirroring the proxy path's `build_record`.
fn addon_tracked_source(prepared: &PreparedInstall) -> Option<TrackedSource> {
    if prepared.addon_source_url.is_empty() && prepared.source_last_modified.is_none() {
        return None;
    }
    Some(
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            prepared.addon_source_url.clone(),
            prepared.source_etag.clone(),
            prepared.source_digest.clone(),
        )
        .with_last_modified(prepared.source_last_modified.clone()),
    )
}

// ---------------------------------------------------------------------------
// Vulkan host: per-game files only (the shared layer is the service's concern)
// ---------------------------------------------------------------------------

/// Installs a Vulkan RenoDX add-on into `game_dir`.
///
/// The host is the shared Vulkan layer (installed separately by the service),
/// so this lays down only per-game files: the add-on and a `ReShade.ini` with
/// the required add-on configuration. Refuses if the add-on is already present.
fn install_vulkan(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<InstalledAddon, ServiceError> {
    let addon_path = game_dir.join(&prepared.addon_file_name);
    if addon_path.is_file() {
        return Err(errors::invalid(
            "RenoDX is already installed for this game; uninstall before reinstalling".to_owned(),
        ));
    }

    let plan = build_vulkan_plan(prepared, game_dir)?;
    let receipt = engine::install(game_dir, &plan)?;

    let sources: Vec<TrackedSource> = addon_tracked_source(prepared).into_iter().collect();
    record::build(
        prepared.game_id.clone(),
        AddonKind::RenoDx,
        &addon_path,
        &receipt,
        sources,
    )
}

/// Builds the per-game file operations for a Vulkan install: the add-on and the
/// `ReShade.ini` merge. Install state is tracked in the app database.
fn build_vulkan_plan(
    prepared: &PreparedInstall,
    game_dir: &Path,
) -> Result<InstallPlan, ServiceError> {
    let mut ops = vec![addon_op(prepared)];
    if let Some(ini_op) = ini_op_for_game(game_dir, &prepared.ini_tweaks) {
        ops.push(ini_op);
    }
    Ok(InstallPlan {
        kind: AddonKind::RenoDx,
        ops,
    })
}

/// Reverses an install, returning the game folder to its prior state.
///
/// `ReShade.ini` is never part of the engine's generic list-based reversal below —
/// it is filtered out of `created_files`/`backed_up_files` first and handled on
/// its own, so it is **never restored from a `.bak` snapshot** even for a legacy
/// record whose ini predates RenoDX's no-backup policy (an old `MergeText`
/// install that backed one up). Its fate: deleted outright only when this record
/// created it from nothing, no legacy backup exists for it either, *and* this
/// install owns the whole ReShade stack it sits beside — a Vulkan install (the
/// per-game ini is exclusively RenoDX's; the shared layer is a separate concern),
/// or a proxy install that also wrote/replaced the host DLL itself. A reused,
/// merely-compatible host (nothing about it is "ours") never gets its freshly
/// created ini deleted either, even though RenoDX is the one that wrote it —
/// only stripped, same as a pre-existing one. Stripping (via
/// [`ini_remove_renodx_strategy`]) removes exactly RenoDX's own keys, leaving
/// everything else — the user's own settings, and any orphaned legacy `.bak` —
/// exactly as it was. `game_dir_hint` (from the caller's own game-folder
/// resolution) is a best-effort assist for locating an ini this record's
/// book-keeping doesn't reference at all (an `UpdateText` merge into a
/// pre-existing ini is deliberately untracked; see [`ini_op_for_game`]) if the
/// host's own directory can't be resolved either.
pub fn uninstall(
    record: &InstalledAddon,
    game_dir_hint: Option<&Path>,
) -> Result<(), ServiceError> {
    let log_base_path = if record.has_host_binary_provenance() {
        tracking::rollback_host_path(record).and_then(|host_path| {
            host_path.parent().map(|game_dir| {
                reshade::resolve_paths(game_dir, Some(&host_path)).effective_base_path
            })
        })
    } else {
        None
    };

    let ini_in_created = ini_path_in(record.created_files());
    let ini_in_backed_up = ini_path_in(record.backed_up_files());
    let owns_whole_stack = matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    ) || host_dll_written_by_this_install(record);

    engine::uninstall(
        &non_ini_path_bufs(record.created_files()),
        &non_ini_path_bufs(record.backed_up_files()),
    )?;

    match ini_in_created.or(ini_in_backed_up) {
        Some(ini_ref) if ini_in_backed_up.is_none() && owns_whole_stack => {
            // Created by this record from nothing, no legacy `.bak` snapshot
            // exists for it, and this install owns the whole ReShade stack it
            // sits beside — it's ours outright.
            crate::fs::remove_file_if_exists(Path::new(ini_ref.as_str()))?;
        }
        Some(ini_ref) => strip_renodx_ini_keys_best_effort(Path::new(ini_ref.as_str())),
        None => {
            if let Some(ini_path) = locate_untracked_ini(record, game_dir_hint) {
                strip_renodx_ini_keys_best_effort(&ini_path);
            }
        }
    }

    if let Some(base_path) = log_base_path {
        reshade::remove_reshade_logs_best_effort(&base_path);
    }
    Ok(())
}

/// Whether this record's `created_files` includes the proxy host DLL — i.e. this
/// install wrote or replaced the active ReShade host itself (whether via a
/// no-backup `Replace` or an older `BackupAndReplace`, both of which land in
/// `created_files`), rather than reusing one that was already there and never
/// touching it.
fn host_dll_written_by_this_install(record: &InstalledAddon) -> bool {
    record
        .created_files()
        .iter()
        .any(|path| path.file_name().is_some_and(reshade::is_proxy_slot))
}

/// The `ReShade.ini` entry in `paths`, if any (case-insensitive by file name).
fn ini_path_in(paths: &[PathRef]) -> Option<&PathRef> {
    paths.iter().find(|path| is_ini_path(path))
}

fn is_ini_path(path: &PathRef) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(reshade::RESHADE_INI_FILE_NAME))
}

/// `paths` as owned `PathBuf`s with the `ReShade.ini` entry, if any, excluded —
/// so the engine's generic reversal never touches it (see [`uninstall`]).
fn non_ini_path_bufs(paths: &[PathRef]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|path| !is_ini_path(path))
        .map(|path| PathBuf::from(path.as_str()))
        .collect()
}

/// Best-effort location of a `ReShade.ini` this record's book-keeping doesn't
/// reference at all — the common case: a compatible detected host merged into a
/// pre-existing ini (never tracked; see [`ini_op_for_game`]). Tries the proxy
/// host's own directory (most reliable, when this record replaced or reused
/// one), then the caller-supplied game-directory hint, then the add-on's own
/// parent directory (RenoDX never sets an explicit `AddonPath`, so this is
/// usually the same folder anyway).
fn locate_untracked_ini(record: &InstalledAddon, game_dir_hint: Option<&Path>) -> Option<PathBuf> {
    let host_dir =
        tracking::rollback_host_path(record).and_then(|path| path.parent().map(Path::to_path_buf));
    let addon_dir = Path::new(record.addon_file().as_str())
        .parent()
        .map(Path::to_path_buf);

    host_dir
        .into_iter()
        .chain(game_dir_hint.map(Path::to_path_buf))
        .chain(addon_dir)
        .find_map(|dir| reshade::reshade_ini_path(&dir))
}

/// Removes exactly the keys/sections RenoDX itself added to a `ReShade.ini` it
/// does not exclusively own, leaving the rest of the file (including the user's
/// own settings, and any legacy `.bak` sitting next to it) untouched.
/// Best-effort: an unreadable or unwritable ini is logged and left as-is rather
/// than failing the whole uninstall.
fn strip_renodx_ini_keys_best_effort(ini_path: &Path) {
    let existing = match std::fs::read_to_string(ini_path) {
        Ok(contents) => contents,
        Err(error) => {
            log::warn!(
                "RenoDX uninstall: failed to read `{}` to strip its keys: {error}",
                ini_path.display()
            );
            return;
        }
    };
    let stripped = ini_remove_renodx_strategy().apply(&existing);
    if let Err(error) = crate::fs::write_file_atomically(ini_path, stripped.as_bytes()) {
        log::warn!(
            "RenoDX uninstall: failed to strip its keys from `{}`: {error}",
            ini_path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::test_support::{
        MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports,
    };
    use std::fs;
    use tempfile::tempdir;

    fn prepared() -> PreparedInstall {
        PreparedInstall {
            game_id: GameId::new("steam:1091500").expect("id"),
            host_kind: HostKind::Proxy,
            proxy_dll_name: "dxgi.dll".to_owned(),
            addon_file_name: "renodx-cp2077.addon64".to_owned(),
            addon_source_url: "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64"
                .to_owned(),
            source_digest: "abc123".to_owned(),
            source_etag: Some("\"etag-1\"".to_owned()),
            source_last_modified: Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned()),
            addon_bytes: b"addon-bytes".to_vec(),
            reshade_dll_bytes: reshade_host_bytes(true),
            reshade_source_url: "https://nightly.link/crosire/reshade/x64.zip".to_owned(),
            reshade_source_etag: Some("\"rs-etag-1\"".to_owned()),
            reshade_last_modified: Some("Tue, 17 Jun 2026 09:00:00 GMT".to_owned()),
            reshade_digest: "reshade-digest".to_owned(),
            reshade_channel: Some(ReshadeChannel::Nightly),
            ini_tweaks: ReshadeIniTweaks::renodx_defaults(),
        }
    }

    fn read(path: &Path) -> Vec<u8> {
        fs::read(path).expect("file should exist")
    }

    fn path_ref(path: &Path) -> PathRef {
        PathRef::new(path.to_string_lossy().into_owned()).expect("valid path")
    }

    fn write_effect_asset(game_dir: &Path) {
        let shaders = game_dir.join("reshade-shaders").join("Shaders");
        fs::create_dir_all(&shaders).expect("create shaders dir");
        fs::write(shaders.join("UserEffect.fx"), b"technique User {}").expect("write effect");
    }

    fn source(record: &InstalledAddon, role: TrackedSourceRole) -> TrackedSource {
        record
            .tracked_sources()
            .iter()
            .find(|s| s.role() == role)
            .cloned()
            .unwrap_or_else(|| panic!("expected a tracked source for {role:?}"))
    }

    fn reshade_host_bytes(addon_support: bool) -> Vec<u8> {
        let mut exports = vec!["ReShadeVersion"];
        if addon_support {
            exports.extend([
                "ReShadeRegisterAddon",
                "ReShadeUnregisterAddon",
                "ReShadeRegisterEvent",
                "ReShadeUnregisterEvent",
            ]);
        }
        build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &exports)
    }

    #[test]
    fn fresh_install_lays_down_host_addon_and_ini_without_marker() {
        let dir = tempdir().expect("tempdir");
        let record = install(dir.path(), &prepared()).expect("install");

        assert_eq!(
            read(&dir.path().join("renodx-cp2077.addon64")),
            b"addon-bytes"
        );
        assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
        assert!(dir.path().join("ReShade.ini").is_file());
        let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
        assert!(ini.contains("DisabledAddons=Generic Depth,Effect Runtime Sync"));

        assert!(record.has_host_binary_provenance());

        // The add-on source is tracked for updates.
        let addon = source(&record, TrackedSourceRole::AddonPayload);
        assert_eq!(
            addon.url(),
            "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64"
        );
        assert_eq!(addon.digest(), "abc123");
        assert_eq!(addon.etag(), Some("\"etag-1\""));

        // A replaced or created host records its upstream entry for host update tracking.
        let host = source(&record, TrackedSourceRole::HostBinary);
        assert_eq!(host.url(), "https://nightly.link/crosire/reshade/x64.zip");
        assert_eq!(host.digest(), "reshade-digest");
        assert_eq!(host.etag(), Some("\"rs-etag-1\""));

        // addon + proxy + ini.
        assert_eq!(record.created_files().len(), 3);
        assert!(record.backed_up_files().is_empty());
    }

    #[test]
    fn fresh_install_round_trips_to_clean_folder() {
        let dir = tempdir().expect("tempdir");
        // A pre-existing unrelated file must survive the round trip.
        fs::write(dir.path().join("game.exe"), b"game").expect("write");

        let record = install(dir.path(), &prepared()).expect("install");
        uninstall(&record, None).expect("uninstall");

        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
        assert!(!dir.path().join("dxgi.dll").exists());
        assert!(!dir.path().join("ReShade.ini").exists());
        assert_eq!(read(&dir.path().join("game.exe")), b"game");
    }

    #[test]
    fn compatible_detected_reshade_is_reused_untouched() {
        let dir = tempdir().expect("tempdir");
        // Simulate an existing compatible ReShade install with a hand-tuned config.
        let original_ini = "[GENERAL]\r\nPreset=mine.ini\r\n";
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
        fs::write(dir.path().join("ReShade.ini"), original_ini).expect("write");
        write_effect_asset(dir.path());

        let record = install(dir.path(), &prepared()).expect("install");

        assert!(!record.has_host_binary_provenance());
        // No Host source is tracked for a reused detected host.
        assert!(
            record
                .tracked_sources()
                .iter()
                .all(|s| s.role() != TrackedSourceRole::HostBinary)
        );
        // Existing DLL untouched (we did not rewrite it or back it up).
        assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
        // The add-on is present; the existing ini is left byte-for-byte untouched.
        assert!(dir.path().join("renodx-cp2077.addon64").is_file());
        assert_eq!(
            String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
            original_ini
        );
        // Nothing was backed up: we touched only the add-on file.
        assert!(record.backed_up_files().is_empty());
    }

    #[test]
    fn detected_host_without_effects_gets_default_disabled_addons() {
        let dir = tempdir().expect("tempdir");
        let original_ini = "[GENERAL]\r\nNoPreset=1\r\n";
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
        fs::write(dir.path().join("ReShade.ini"), original_ini).expect("write");

        let record = install(dir.path(), &prepared()).expect("install");

        assert!(!record.has_host_binary_provenance());
        assert_eq!(
            String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
            "[GENERAL]\r\nNoPreset=1\r\n\r\n[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n"
        );
        // No `.bak` — the merge into a pre-existing ini uses `UpdateText`, not
        // `MergeText`, so it is never backed up.
        assert!(record.backed_up_files().is_empty());
        assert!(!dir.path().join("ReShade.ini.bak").exists());

        uninstall(&record, None).expect("uninstall");

        // The ini pre-dates this install (never in `created_files`), so uninstall
        // never deletes or snapshot-restores it — only RenoDX's own key is
        // stripped, leaving the user's `[GENERAL]` content exactly as it was.
        let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
        assert!(ini.contains("NoPreset=1"));
        assert!(!ini.contains("DisabledAddons"));
        assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    }

    #[test]
    fn reused_host_with_no_pre_existing_ini_gets_a_fresh_one_stripped_not_deleted() {
        let dir = tempdir().expect("tempdir");
        // A compatible, already-active ReShade host — reused, never rewritten —
        // but no `ReShade.ini` exists yet (e.g. installed but never launched).
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");

        let record = install(dir.path(), &prepared()).expect("install");
        assert!(!record.has_host_binary_provenance());
        let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
        assert!(ini.contains("DisabledAddons=Generic Depth,Effect Runtime Sync"));

        uninstall(&record, None).expect("uninstall");

        // The ini RenoDX created from nothing survives uninstall — stripped, not
        // deleted — because the host beside it was merely reused, never written
        // by this install; RenoDX doesn't own the whole stack here, only the
        // add-on and the keys it added to the config.
        let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
        assert!(!ini.contains("DisabledAddons"));
        assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
    }

    #[test]
    fn legacy_backed_up_ini_is_stripped_not_restored_from_its_stale_snapshot() {
        let dir = tempdir().expect("tempdir");
        let addon_path = dir.path().join("renodx-cp2077.addon64");
        fs::write(&addon_path, b"addon").expect("write addon");
        // The current ini: what an old `MergeText`-based install produced, plus a
        // setting the user added by hand afterward.
        fs::write(
            dir.path().join("ReShade.ini"),
            "[GENERAL]\r\nPreset=mine.ini\r\n\r\n\
             [ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
        )
        .expect("write ini");
        // The stale pre-install snapshot that install's `MergeText` backed up —
        // this must never come back, even though it's still sitting right there.
        fs::write(
            dir.path().join("ReShade.ini.bak"),
            "[GENERAL]\r\nPreset=old.ini\r\n",
        )
        .expect("write bak");

        let addon_ref = path_ref(&addon_path);
        let ini_ref = path_ref(&dir.path().join("ReShade.ini"));
        let record = InstalledAddon::from_parts(
            GameId::new("steam:1091500").expect("id"),
            AddonKind::RenoDx,
            addon_ref.clone(),
            None,
            vec![addon_ref, ini_ref.clone()],
            vec![ini_ref],
            Vec::new(),
        )
        .expect("record");

        uninstall(&record, None).expect("uninstall");

        let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
        assert!(ini.contains("Preset=mine.ini"));
        assert!(!ini.contains("DisabledAddons"));
        assert!(!ini.contains("Preset=old.ini"));
        // The legacy `.bak` is left exactly as it was — orphaned, never restored
        // from and never deleted either.
        assert_eq!(
            String::from_utf8(read(&dir.path().join("ReShade.ini.bak"))).unwrap(),
            "[GENERAL]\r\nPreset=old.ini\r\n"
        );
    }

    #[test]
    fn strip_locates_an_untracked_ini_via_the_host_directory_not_the_addon_directory() {
        let dir = tempdir().expect("tempdir");
        // The add-on lives in a subfolder (a custom `[ADDON] AddonPath`); the host
        // and the pre-existing `ReShade.ini` live in the game directory itself.
        let addons_subdir = dir.path().join("addons");
        fs::create_dir_all(&addons_subdir).expect("mkdir");
        let addon_path = addons_subdir.join("renodx-cp2077.addon64");
        fs::write(&addon_path, b"addon").expect("write addon");
        let host_path = dir.path().join("dxgi.dll");
        fs::write(&host_path, reshade_host_bytes(true)).expect("write host");
        fs::write(
            dir.path().join("ReShade.ini"),
            "[GENERAL]\r\nPreset=mine.ini\r\n\r\n\
             [ADDON]\r\nAddonPath=addons\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
        )
        .expect("write ini");

        let record = InstalledAddon::from_parts(
            GameId::new("steam:1091500").expect("id"),
            AddonKind::RenoDx,
            path_ref(&addon_path),
            None,
            vec![path_ref(&addon_path), path_ref(&host_path)],
            Vec::new(),
            Vec::new(),
        )
        .expect("record");

        uninstall(&record, None).expect("uninstall");

        // The ini is found via the host's own directory (not the add-on's
        // subfolder) and stripped, not left with RenoDX's key still in it.
        let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
        assert!(ini.contains("Preset=mine.ini"));
        assert!(!ini.contains("DisabledAddons"));
    }

    #[test]
    fn inactive_reshade_engine_dll_refuses_second_host() {
        let dir = tempdir().expect("tempdir");
        // ReShade exists, but not in the slot this game will load.
        fs::write(dir.path().join("ReShade64.dll"), reshade_host_bytes(true)).expect("write");

        let error = install(dir.path(), &prepared()).expect_err("should refuse inactive host");
        assert!(matches!(error, ServiceError::InvalidInput(_)));
        assert!(!dir.path().join("ReShade.ini").exists());
        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    }

    #[test]
    fn detected_host_install_leaves_original_ini_intact() {
        let dir = tempdir().expect("tempdir");
        let original_ini = "[GENERAL]\r\nPreset=mine.ini\r\n";
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
        fs::write(dir.path().join("ReShade.ini"), original_ini).expect("write");
        write_effect_asset(dir.path());

        let record = install(dir.path(), &prepared()).expect("install");
        // The existing ini is never backed up or rewritten.
        assert!(record.backed_up_files().is_empty());
        assert_eq!(
            String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
            original_ini
        );

        uninstall(&record, None).expect("uninstall");

        // Add-on removed, existing DLL and original ini intact.
        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
        assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
        assert_eq!(
            String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
            original_ini
        );
    }

    #[test]
    fn repeated_install_is_idempotent() {
        let dir = tempdir().expect("tempdir");
        install(dir.path(), &prepared()).expect("first install");
        let record = install(dir.path(), &prepared()).expect("second install");
        assert!(
            record
                .created_files()
                .iter()
                .any(|path| { path.as_str().ends_with("renodx-cp2077.addon64") })
        );
    }

    #[test]
    fn active_host_without_addon_support_is_replaced_with_no_backup() {
        let dir = tempdir().expect("tempdir");
        // A ReShade host occupies the active slot, but it is the build WITHOUT
        // add-on support — RenoDX's add-on cannot load there, so install must
        // replace it with the bundled add-on-capable build. Its identity was
        // already confirmed by `host_policy::assess` before this runs, so RenoDX
        // treats it as an unambiguous official ReShade build: no backup is kept.
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(false)).expect("write");

        let record = install(dir.path(), &prepared()).expect("install");

        // Our add-on-capable host now occupies the slot; the original is gone,
        // not backed up.
        assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
        assert!(record.has_host_binary_provenance());
        assert!(record.backed_up_files().is_empty());
        assert!(!dir.path().join("dxgi.dll.bak").exists());

        uninstall(&record, None).expect("uninstall");

        // Uninstall deletes the host we installed outright — there is no `.bak`
        // to restore the original add-on-less build from.
        assert!(!dir.path().join("dxgi.dll").exists());
        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    }

    #[test]
    fn host_repair_requires_reshade_bytes() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(false)).expect("write");
        let mut prepared = prepared();
        prepared.reshade_dll_bytes.clear();

        let error = install(dir.path(), &prepared).expect_err("repair needs bytes");

        assert!(matches!(error, ServiceError::InvalidInput(_)));
        assert_eq!(
            read(&dir.path().join("dxgi.dll")),
            reshade_host_bytes(false)
        );
        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    }

    #[test]
    fn multiple_reshade_hosts_refuse_install_before_writes() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
        fs::write(dir.path().join("ReShade64.dll"), reshade_host_bytes(true)).expect("write");

        let error = install(dir.path(), &prepared()).expect_err("multiple hosts conflict");

        assert!(matches!(error, ServiceError::InvalidInput(_)));
        assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
        assert_eq!(
            read(&dir.path().join("ReShade64.dll")),
            reshade_host_bytes(true)
        );
        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    }

    #[test]
    fn refuses_install_when_proxy_slot_is_occupied_by_an_unknown_file() {
        let dir = tempdir().expect("tempdir");
        // A file already occupies the proxy-DLL slot — another graphics overlay or a
        // game-shipped dxgi.dll. With no ReShade host detected, the install must
        // refuse rather than silently displace it.
        fs::write(dir.path().join("dxgi.dll"), b"another-overlay").expect("write");

        let error = install(dir.path(), &prepared()).expect_err("should refuse");
        assert!(matches!(error, ServiceError::InvalidInput(_)));
        // The occupying file is left untouched, and nothing else was laid down.
        assert_eq!(read(&dir.path().join("dxgi.dll")), b"another-overlay");
        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    }

    /// A Vulkan-host prepared install: no proxy DLL, no ReShade bytes (the host is
    /// the shared layer handled separately by the service).
    fn vulkan_prepared() -> PreparedInstall {
        PreparedInstall {
            host_kind: HostKind::Vulkan,
            proxy_dll_name: String::new(),
            reshade_dll_bytes: Vec::new(),
            ..prepared()
        }
    }

    #[test]
    fn vulkan_install_lays_down_addon_and_ini_without_a_proxy() {
        let dir = tempdir().expect("tempdir");
        let record = install(dir.path(), &vulkan_prepared()).expect("vulkan install");

        assert_eq!(
            read(&dir.path().join("renodx-cp2077.addon64")),
            b"addon-bytes"
        );
        assert!(dir.path().join("ReShade.ini").is_file());
        // No proxy DLL is written for a Vulkan install (the host is the shared layer).
        assert!(!dir.path().join("dxgi.dll").exists());
        let ini = String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap();
        assert!(ini.contains("[ADDON]"));
        // A file install (no upstream URL is set on the addon source here) still
        // tracks the add-on when one is recorded; the host is never tracked.
        assert!(
            record
                .tracked_sources()
                .iter()
                .all(|s| s.role() != TrackedSourceRole::HostBinary)
        );
        // addon + ini.
        assert_eq!(record.created_files().len(), 2);
    }

    #[test]
    fn vulkan_install_round_trips_to_clean_folder() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("game.exe"), b"game").expect("write");

        // In production, `use_cases::commands::install::annotate_install_record`
        // stamps `host_kind` onto the record straight after this call — `uninstall`
        // relies on it to know a Vulkan install owns its per-game ini outright.
        let record = install(dir.path(), &vulkan_prepared())
            .expect("install")
            .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
        uninstall(&record, None).expect("uninstall");

        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
        assert!(!dir.path().join("ReShade.ini").exists());
        assert_eq!(read(&dir.path().join("game.exe")), b"game");
    }

    #[test]
    fn vulkan_install_refuses_when_already_installed() {
        let dir = tempdir().expect("tempdir");
        install(dir.path(), &vulkan_prepared()).expect("first install");
        let error = install(dir.path(), &vulkan_prepared()).expect_err("should refuse");
        assert!(matches!(error, ServiceError::InvalidInput(_)));
    }
}
