//! Turns a fetched RenoDX install into a generic [`InstallPlan`] for the shared
//! [`addons::engine`](crate::addons::engine) and maps the result back to a record.
//!
//! Given a [`PreparedInstall`] (already-fetched, already-verified bytes plus the
//! resolved file names), [`install`] builds the ordered file operations — the
//! add-on, and the ReShade host + `ReShade.ini` + ownership marker when no host is
//! present — hands them to the engine (which backs up, rolls back, and journals),
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
use super::reshade::{self, ReshadeMarker, ReshadeState, detect_reshade, ini_merge_strategy};
use super::types::ReshadeIniTweaks;

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
    /// ReShade host channel label, when a host was installed.
    pub reshade_version: Option<String>,
    /// Upstream URL the ReShade host came from (empty when no host is installed).
    pub reshade_source_url: String,
    /// The ReShade host zip's cache validator (for a cheap host update pre-check).
    pub reshade_source_etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string of the ReShade host zip, when sent.
    pub reshade_last_modified: Option<String>,
    /// SHA-256 of the installed ReShade host DLL (empty when none installed).
    pub reshade_digest: String,
    /// `ReShade.ini` tweaks RenoDX requires.
    pub ini_tweaks: ReshadeIniTweaks,
}

/// Installs RenoDX into `game_dir`, returning the record needed to reverse it.
///
/// Refuses if RenderPilot already manages a RenoDX install here (the caller should
/// uninstall first to reinstall). When no ReShade host is present one is installed
/// (proxy DLL + fresh `ReShade.ini` + marker); a foreign host is reused with only an
/// additive, backed-up `ReShade.ini` merge. The engine rolls back on any failure.
pub fn install(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<InstalledAddon, ServiceError> {
    let reshade = detect_reshade(game_dir);
    if matches!(reshade, ReshadeState::Managed(_)) {
        return Err(errors::invalid(
            "RenoDX is already installed for this game; uninstall before reinstalling".to_owned(),
        ));
    }

    // A RenoDX install we already manage is refused above; only `Absent` (install a
    // host) and `Foreign` (reuse one) remain.
    let manages_host = matches!(reshade, ReshadeState::Absent);
    if manages_host && prepared.reshade_dll_bytes.is_empty() {
        return Err(errors::invalid(
            "no ReShade host present and no ReShade bytes were provided to install one".to_owned(),
        ));
    }

    // Refuse rather than silently displace a file already occupying the proxy-DLL
    // slot. With no ReShade host detected, an existing proxy-named DLL is another
    // graphics overlay (a standalone ReShade, a different proxy) or a game-shipped
    // one, and replacing it would break it; the user resolves the conflict first.
    if manages_host && proxy_slot_occupied(game_dir, &prepared.proxy_dll_name) {
        return Err(errors::invalid(format!(
            "the '{}' slot RenoDX needs is already occupied by another file; remove or \
             relocate it before installing (it may be a different graphics overlay or proxy)",
            prepared.proxy_dll_name
        )));
    }

    let plan = build_plan(prepared, manages_host)?;
    let receipt = engine::install(game_dir, &plan)?;
    build_record(prepared, game_dir, manages_host, &receipt)
}

/// Whether a file already occupies the proxy-DLL slot RenoDX would install into,
/// matched case-insensitively (Windows filesystems are case-insensitive, and a
/// foreign overlay may use any casing). A managed install refuses rather than
/// displace it.
fn proxy_slot_occupied(game_dir: &Path, proxy_dll_name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(game_dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(proxy_dll_name)
    })
}

/// Builds the ordered file operations for the install.
///
/// Always lays the add-on down first; when installing a host, follows with the proxy
/// DLL, the `ReShade.ini` merge, and the ownership marker (so a later op never edits
/// a file an earlier op has not yet created).
fn build_plan(prepared: &PreparedInstall, manages_host: bool) -> Result<InstallPlan, ServiceError> {
    let mut ops = vec![FileOp::Create {
        name: prepared.addon_file_name.clone(),
        bytes: prepared.addon_bytes.clone(),
    }];

    if manages_host {
        ops.push(FileOp::BackupAndReplace {
            name: prepared.proxy_dll_name.clone(),
            bytes: prepared.reshade_dll_bytes.clone(),
        });
        ops.push(ini_op(&prepared.ini_tweaks));
        ops.push(FileOp::Create {
            name: reshade::MARKER_FILE_NAME.to_owned(),
            bytes: marker_bytes(prepared)?,
        });
    }
    // A foreign host is reused as-is: we place only the add-on file and leave the
    // user's `ReShade.ini` untouched (ReShade loads root add-ons by default), so a
    // hand-tuned foreign config is never backed up, rewritten, or clobbered.

    Ok(InstallPlan {
        kind: AddonKind::RenoDx,
        ops,
    })
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

/// Serializes the ownership marker RenderPilot drops when it installs the host.
fn marker_bytes(prepared: &PreparedInstall) -> Result<Vec<u8>, ServiceError> {
    let marker = ReshadeMarker::new(
        prepared.proxy_dll_name.clone(),
        prepared.reshade_version.clone(),
        prepared.addon_file_name.clone(),
    );
    serde_json::to_vec_pretty(&marker)
        .map_err(|error| errors::failed(format!("failed to serialize ReShade marker: {error}")))
}

/// Assembles the [`InstalledAddon`] from the engine receipt and the upstream sources
/// to track: the add-on (when fetched from upstream) and the managed ReShade host.
fn build_record(
    prepared: &PreparedInstall,
    game_dir: &Path,
    manages_host: bool,
    receipt: &engine::InstallReceipt,
) -> Result<InstalledAddon, ServiceError> {
    let addon_path = game_dir.join(&prepared.addon_file_name);

    let mut sources = Vec::new();
    // A file install has no upstream add-on URL, so it tracks no add-on source
    // (updates honestly report `Unknown`).
    if !prepared.addon_source_url.is_empty() {
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
    // A managed host records its source; a reused foreign host records none (and so
    // reads as unmanaged and is never modified on update/uninstall).
    if manages_host {
        sources.push(
            TrackedSource::new(
                TrackedSourceRole::Host,
                prepared.reshade_source_url.clone(),
                prepared.reshade_source_etag.clone(),
                prepared.reshade_digest.clone(),
            )
            .with_last_modified(prepared.reshade_last_modified.clone()),
        );
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
    engine::uninstall(
        &to_path_bufs(record.created_files()),
        &to_path_bufs(record.backed_up_files()),
    )
}

fn to_path_bufs(paths: &[PathRef]) -> Vec<PathBuf> {
    paths.iter().map(|p| PathBuf::from(p.as_str())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::reshade::marker_path;
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
            reshade_dll_bytes: b"reshade-dll-bytes".to_vec(),
            reshade_version: Some("nightly".to_owned()),
            reshade_source_url: "https://nightly.link/crosire/reshade/x64.zip".to_owned(),
            reshade_source_etag: Some("\"rs-etag-1\"".to_owned()),
            reshade_last_modified: Some("Tue, 17 Jun 2026 09:00:00 GMT".to_owned()),
            reshade_digest: "reshade-digest".to_owned(),
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

    #[test]
    fn fresh_install_lays_down_host_addon_ini_and_marker() {
        let dir = tempdir().expect("tempdir");
        let record = install(dir.path(), &prepared()).expect("install");

        assert_eq!(
            read(&dir.path().join("renodx-cp2077.addon64")),
            b"addon-bytes"
        );
        assert_eq!(read(&dir.path().join("dxgi.dll")), b"reshade-dll-bytes");
        assert!(dir.path().join("ReShade.ini").is_file());
        assert!(marker_path(dir.path()).is_file());

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

        // addon + proxy + ini + marker.
        assert_eq!(record.created_files().len(), 4);
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
        fs::write(dir.path().join("dxgi.dll"), b"foreign-reshade").expect("write");
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
        assert_eq!(read(&dir.path().join("dxgi.dll")), b"foreign-reshade");
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
    fn foreign_reshade_engine_dll_leaves_ini_absent() {
        let dir = tempdir().expect("tempdir");
        // A foreign ReShade detected only by its engine DLL, with no ReShade.ini yet.
        fs::write(dir.path().join("ReShade64.dll"), b"foreign").expect("write");

        let record = install(dir.path(), &prepared()).expect("install");

        assert!(!record.reshade_managed_by_us());
        // We do not create or back up a ReShade.ini for a foreign host; only the
        // add-on is placed, and the foreign DLL is left alone.
        assert!(!dir.path().join("ReShade.ini").exists());
        assert!(!marker_path(dir.path()).exists());
        assert!(
            record
                .created_files()
                .iter()
                .all(|p| !p.as_str().ends_with("ReShade.ini"))
        );
        assert!(record.backed_up_files().is_empty());

        uninstall(&record).expect("uninstall");
        // No ini appears; the foreign DLL stays; the add-on is gone.
        assert!(!dir.path().join("ReShade.ini").exists());
        assert_eq!(read(&dir.path().join("ReShade64.dll")), b"foreign");
        assert!(!dir.path().join("renodx-cp2077.addon64").exists());
    }

    #[test]
    fn foreign_install_leaves_original_ini_intact() {
        let dir = tempdir().expect("tempdir");
        let original_ini = "[GENERAL]\r\nPreset=mine.ini\r\n";
        fs::write(dir.path().join("dxgi.dll"), b"foreign-reshade").expect("write");
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
        assert_eq!(read(&dir.path().join("dxgi.dll")), b"foreign-reshade");
        assert_eq!(
            String::from_utf8(read(&dir.path().join("ReShade.ini"))).unwrap(),
            original_ini
        );
    }

    #[test]
    fn refuses_install_when_already_managed() {
        let dir = tempdir().expect("tempdir");
        install(dir.path(), &prepared()).expect("first install");
        // Second install must refuse because our marker is present.
        let error = install(dir.path(), &prepared()).expect_err("should refuse");
        assert!(matches!(error, ServiceError::InvalidInput(_)));
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
