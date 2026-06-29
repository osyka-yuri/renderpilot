//! Turns a fetched RenoDX install into a generic [`InstallPlan`] for the shared
//! [`addons::engine`](crate::addons::engine) and maps the result back to a record.
//!
//! Given a [`PreparedInstall`] (already-fetched, already-verified bytes plus the
//! resolved file names), [`install`] builds the ordered file operations — the
//! add-on, and the ReShade host + `ReShade.ini` when no suitable host is present
//! or the active host needs repair — hands them to the engine (which backs up,
//! rolls back, and journals),
//! and assembles the reversible [`InstalledAddon`] with the upstream sources to
//! track for updates. [`uninstall`] is a thin tool-specific wrapper over the
//! engine's generic reversal. (In-place file replacement for updates lives in the
//! update flow, which drives the engine directly.)
//!
//! All ReShade/HDR specifics live here; the filesystem mechanics are the engine's.

use std::path::{Path, PathBuf};

use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, PathRef, TrackedSource, TrackedSourceRole,
};

use crate::ServiceError;
use crate::addons::engine::{self, FileOp, InstallPlan};
use crate::addons::record;

use super::errors;
use super::host_policy::{self, HostAssessment};
use super::reshade;
use super::reshade_ini::ini_merge_strategy;
use super::tracking;
use super::types::{ReshadeChannel, ReshadeIniTweaks};

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
    /// Proxy DLL file name to install the ReShade host as (for example `dxgi.dll`).
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
    /// Effective channel for the managed ReShade host source.
    pub reshade_channel: Option<ReshadeChannel>,
    /// `ReShade.ini` tweaks RenoDX requires.
    pub ini_tweaks: ReshadeIniTweaks,
}

/// Installs RenoDX into `game_dir`, returning the record needed to reverse it.
///
/// Idempotently installs RenoDX. A suitable active ReShade host is reused
/// regardless of who placed it; an active host without add-on support is backed up
/// and replaced with the bundled full add-on-support build. The engine rolls back
/// on any failure and the DB is still updated by the caller only after files land.
pub fn install(
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

    reshade::remove_legacy_marker(game_dir);

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

    let host_receipt = if host.writes_host() {
        let host_plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: host_ops(game_dir, prepared),
        };
        match engine::install(game_dir, &host_plan) {
            Ok(receipt) => receipt,
            Err(error) => {
                let _ =
                    engine::uninstall(&addon_receipt.created_files, &addon_receipt.backed_up_files);
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
        ops.extend(host_ops(game_dir, prepared));
    }
    ops
}

fn addon_op(prepared: &PreparedInstall) -> FileOp {
    FileOp::Create {
        name: prepared.addon_file_name.clone(),
        bytes: prepared.addon_bytes.clone(),
    }
}

fn host_ops(game_dir: &Path, prepared: &PreparedInstall) -> Vec<FileOp> {
    let mut ops = vec![FileOp::BackupAndReplace {
        name: prepared.proxy_dll_name.clone(),
        bytes: prepared.reshade_dll_bytes.clone(),
    }];
    if reshade::reshade_ini_path(game_dir).is_none() {
        ops.push(ini_op(&prepared.ini_tweaks));
    }
    ops
}

/// The `ReShade.ini` merge operation: additively set RenoDX's `[ADDON]` keys,
/// creating the file from empty when none exists.
fn ini_op(tweaks: &ReshadeIniTweaks) -> FileOp {
    FileOp::MergeText {
        name: reshade::RESHADE_INI_FILE_NAME.to_owned(),
        default: String::new(),
        strategy: ini_merge_strategy(tweaks),
    }
}

/// Assembles the [`InstalledAddon`] from the engine receipt and the upstream sources
/// to track: the add-on (when fetched from upstream) and the managed ReShade host.
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
        let mut host = TrackedSource::new(
            TrackedSourceRole::Host,
            prepared.reshade_source_url.clone(),
            prepared.reshade_source_etag.clone(),
            prepared.reshade_digest.clone(),
        )
        .with_last_modified(prepared.reshade_last_modified.clone());
        if let Some(channel) = prepared.reshade_channel {
            host = host.with_channel(channel.as_str());
        }
        sources.push(host);
    }

    record::build(
        prepared.game_id.clone(),
        AddonKind::RenoDx,
        &addon_path,
        receipt,
        sources,
    )
}

/// Reverses an install, returning the game folder to its prior state.
pub fn uninstall(record: &InstalledAddon) -> Result<(), ServiceError> {
    let log_base_path = if record.reshade_managed_by_us() {
        tracking::managed_host_path(record).and_then(|host_path| {
            host_path.parent().map(|game_dir| {
                reshade::resolve_paths(game_dir, Some(&host_path)).effective_base_path
            })
        })
    } else {
        None
    };

    engine::uninstall(
        &to_path_bufs(record.created_files()),
        &to_path_bufs(record.backed_up_files()),
    )?;

    if let Some(base_path) = log_base_path {
        reshade::remove_reshade_logs_best_effort(&base_path);
    }
    if let Some(game_dir) = Path::new(record.addon_file().as_str()).parent() {
        reshade::remove_legacy_marker(game_dir);
    }
    Ok(())
}

fn to_path_bufs(paths: &[PathRef]) -> Vec<PathBuf> {
    paths.iter().map(|p| PathBuf::from(p.as_str())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::reshade::marker_path;
    use crate::addons::renodx::test_support::{
        MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports,
    };
    use std::fs;
    use tempfile::tempdir;

    fn prepared() -> PreparedInstall {
        PreparedInstall {
            game_id: GameId::new("steam:1091500").expect("id"),
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
        assert!(!marker_path(dir.path()).exists());

        assert!(record.reshade_managed_by_us());

        // The add-on source is tracked for updates.
        let addon = source(&record, TrackedSourceRole::AddonPayload);
        assert_eq!(
            addon.url(),
            "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64"
        );
        assert_eq!(addon.digest(), "abc123");
        assert_eq!(addon.etag(), Some("\"etag-1\""));

        // A managed host records its upstream source for host update tracking.
        let host = source(&record, TrackedSourceRole::Host);
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
        uninstall(&record).expect("uninstall");

        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
        assert!(!dir.path().join("dxgi.dll").exists());
        assert!(!dir.path().join("ReShade.ini").exists());
        assert!(!marker_path(dir.path()).exists());
        assert_eq!(read(&dir.path().join("game.exe")), b"game");
    }

    #[test]
    fn foreign_reshade_is_reused_untouched() {
        let dir = tempdir().expect("tempdir");
        // Simulate a foreign ReShade install with a hand-tuned config.
        let original_ini = "[GENERAL]\r\nPreset=mine.ini\r\n";
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
        fs::write(dir.path().join("ReShade.ini"), original_ini).expect("write");

        let record = install(dir.path(), &prepared()).expect("install");

        assert!(!record.reshade_managed_by_us());
        // No Host source is tracked for a reused foreign host.
        assert!(
            record
                .tracked_sources()
                .iter()
                .all(|s| s.role() != TrackedSourceRole::Host)
        );
        // Foreign DLL untouched (we did not rewrite it or back it up).
        assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
        assert!(!marker_path(dir.path()).exists());
        // Our addon is present; the foreign ini is left byte-for-byte untouched.
        assert!(dir.path().join("renodx-cp2077.addon64").is_file());
        assert_eq!(
            String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
            original_ini
        );
        // Nothing was backed up: we touched only the add-on file.
        assert!(record.backed_up_files().is_empty());
    }

    #[test]
    fn inactive_reshade_engine_dll_refuses_second_host() {
        let dir = tempdir().expect("tempdir");
        // ReShade exists, but not in the slot this game will load.
        fs::write(dir.path().join("ReShade64.dll"), reshade_host_bytes(true)).expect("write");

        let error = install(dir.path(), &prepared()).expect_err("should refuse inactive host");
        assert!(matches!(error, ServiceError::InvalidInput(_)));
        assert!(!dir.path().join("ReShade.ini").exists());
        assert!(!marker_path(dir.path()).exists());
        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    }

    #[test]
    fn foreign_install_leaves_original_ini_intact() {
        let dir = tempdir().expect("tempdir");
        let original_ini = "[GENERAL]\r\nPreset=mine.ini\r\n";
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(true)).expect("write");
        fs::write(dir.path().join("ReShade.ini"), original_ini).expect("write");

        let record = install(dir.path(), &prepared()).expect("install");
        // The foreign ini is never backed up or rewritten.
        assert!(record.backed_up_files().is_empty());
        assert_eq!(
            String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
            original_ini
        );

        uninstall(&record).expect("uninstall");

        // Addon removed, foreign DLL and original ini intact.
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
    fn active_host_without_addon_support_is_replaced_and_restored() {
        let dir = tempdir().expect("tempdir");
        // A ReShade host occupies the active slot, but it is the build WITHOUT
        // add-on support — RenoDX's add-on cannot load there, so install must
        // replace it with the bundled add-on-capable build, reversibly.
        fs::write(dir.path().join("dxgi.dll"), reshade_host_bytes(false)).expect("write");

        let record = install(dir.path(), &prepared()).expect("install");

        // Our add-on-capable host now occupies the slot; the original is backed up.
        assert_eq!(read(&dir.path().join("dxgi.dll")), reshade_host_bytes(true));
        assert!(record.reshade_managed_by_us());
        assert!(!record.backed_up_files().is_empty());

        uninstall(&record).expect("uninstall");

        // The original add-on-less host is restored and our add-on is gone.
        assert_eq!(
            read(&dir.path().join("dxgi.dll")),
            reshade_host_bytes(false)
        );
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
    fn refuses_install_when_proxy_slot_is_occupied_by_a_foreign_file() {
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
}
