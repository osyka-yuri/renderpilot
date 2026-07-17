//! ReShade proxy-slot scanning: what's occupying each candidate slot in a game
//! folder, and whether it looks like ReShade. The host data model lives in
//! [`super::host_model`].

use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_detection::inspect_pe;

use super::host_model::{
    ActiveSlotReason, ActiveSlotState, ReshadeAddonSupport, ReshadeHost, ReshadeHostAction,
    ReshadeIdentity, ReshadeScan, SlotActivity,
};
use super::identity::{is_proxy_slot, is_reshade_engine_dll, version_strings_point_to_reshade};
use super::paths::{reshade_ini_path, reshade_log_paths};

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

/// Whether `path` is a ReShade host by PE evidence alone (export name or version
/// resource strings). Used by torn-install recovery to remove half-written
/// RenderPilot hosts without deleting unrecognized game DX wrappers.
///
/// Unreadable / non-PE files return `false` (leave them alone).
#[must_use]
pub(crate) fn is_reshade_proxy_file(path: &Path) -> bool {
    let Some(inspection) = inspect_pe(path) else {
        return false;
    };
    let has_reshade_export = inspection
        .export_names
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .any(|name| name.eq_ignore_ascii_case(RESHADE_VERSION_EXPORT));
    has_reshade_export || version_strings_point_to_reshade(&inspection.identity)
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

fn has_neighboring_reshade_files(game_dir: &Path) -> bool {
    reshade_ini_path(game_dir).is_some() || reshade_log_paths(game_dir).next().is_some()
}

/// Applies the strict structural host policy.
///
/// An absent host yields [`ReshadeHostAction::UpdateHost`] — "a host must be
/// written" — which the install flow treats as "install a fresh host" and the
/// update flow as "place a recorded host binary". Version resources are display-only:
/// freshness is decided by channel artifact validation.
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

/// Whether a DLL name is plausibly the ReShade host (worth a PE inspection): a
/// known proxy slot, the ReShade engine DLL, or a `reshade*`-named file.
fn is_host_candidate(lower_name: &str) -> bool {
    is_proxy_slot(lower_name)
        || is_reshade_engine_dll(lower_name)
        || lower_name.starts_with("reshade")
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::Version;
    use tempfile::tempdir;

    use super::*;

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

        assert_eq!(host_action(&ReshadeHost::Absent), A::UpdateHost);
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Full,
                ReshadeIdentity::Confirmed,
                SlotActivity::Inactive,
                Some("6.6.0"),
            )),
            A::Conflict
        );
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Full,
                ReshadeIdentity::Weak,
                SlotActivity::Active,
                Some("6.6.0"),
            )),
            A::Conflict
        );
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::None,
                ReshadeIdentity::Confirmed,
                SlotActivity::Active,
                Some("1.0.0"),
            )),
            A::ReinstallWithAddonSupport
        );
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Unknown,
                ReshadeIdentity::Probable,
                SlotActivity::Active,
                Some("6.6.0"),
            )),
            A::RepairHost
        );
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Full,
                ReshadeIdentity::Confirmed,
                SlotActivity::Active,
                None,
            )),
            A::UpToDate
        );
        assert_eq!(
            host_action(&present_host(
                ReshadeAddonSupport::Full,
                ReshadeIdentity::Confirmed,
                SlotActivity::Active,
                Some("1.0.0"),
            )),
            A::UpToDate
        );
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
