//! Shared pure policy for Luma update queries and commands.
//!
//! Resolves the current install target (asset/arch/proxy slot) fresh from the
//! manifest — none of it is persisted on the install record — and owns host
//! rewrite / check-status policy shared by prepare and `check_update` so those
//! paths cannot drift.
//!
//! Disk/network host apply lives in [`commands::update::host`]; this module is
//! policy only and never imports from [`crate::addons::renodx`].

use std::path::PathBuf;

use renderpilot_application::GameRepository;
use renderpilot_domain::{Architecture, GameId, Version};

use crate::addons::game_analysis::install_target_dir;
use crate::addons::luma::game_context::analyze_and_resolve;
use crate::addons::luma::matcher::LumaResolution;
use crate::addons::luma::types::{LumaExternalRequirement, LumaManifest};
use crate::addons::reshade::host_policy::{self, HostAssessment, HostLifecycle};
use crate::addons::update::UpdateStatus;
use crate::{Context, ServiceError};

/// The currently-resolved Luma install target for a game: what asset/arch/proxy
/// slot the manifest resolves to *right now*, re-derived fresh since none of it
/// is persisted on the install record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedUpdateTarget {
    pub(crate) game_dir: PathBuf,
    pub(crate) asset: String,
    pub(crate) addon_file: String,
    pub(crate) arch: Architecture,
    pub(crate) proxy_dll_name: String,
    pub(crate) external_requirement: Option<LumaExternalRequirement>,
}

/// Re-resolves `game_id` against `manifest`, returning `None` when the game can
/// no longer be resolved (removed from the library) or no longer matches an
/// installable Luma title (e.g. de-listed since install).
pub(crate) fn resolve_update_target(
    context: &Context,
    manifest: &LumaManifest,
    game_id: &GameId,
) -> Result<Option<ResolvedUpdateTarget>, ServiceError> {
    let Some(game) = context.storage().find_game(game_id)? else {
        return Ok(None);
    };
    let override_path = crate::addons::game_context::executable_override(context, game_id);
    let (analysis, resolution) = analyze_and_resolve(&game, manifest, override_path.as_deref());
    let LumaResolution::Installable(plan) = resolution else {
        return Ok(None);
    };
    let game_dir = install_target_dir(&analysis)?;
    Ok(Some(ResolvedUpdateTarget {
        game_dir,
        asset: plan.asset.clone(),
        addon_file: plan.addon_file.clone(),
        arch: plan.arch,
        proxy_dll_name: plan.proxy_dll_name.clone(),
        external_requirement: plan.external_requirement.clone(),
    }))
}

/// Resolves the complete live target required by Update and Repair. Recorded
/// paths and provenance cannot safely reconstruct matcher-owned layout,
/// dependency, architecture, and host-slot decisions.
pub(crate) fn require_update_target(
    context: &Context,
    manifest: &LumaManifest,
    game_id: &GameId,
) -> Result<ResolvedUpdateTarget, ServiceError> {
    resolve_update_target(context, manifest, game_id)?.ok_or_else(|| {
        crate::addons::luma::errors::invalid(
            "Luma no longer has a live installable profile for this game; update and repair are unavailable until the catalogue match is restored"
                .to_owned(),
        )
    })
}

/// Assesses the ReShade host for an update, gated on Luma's minimum host
/// version. `None` when the active slot is a recognized custom build (e.g.
/// GShade) — never checked or replaced automatically, so every caller gets
/// that guarantee for free.
#[must_use]
pub(crate) fn assess_host_for_update(
    target: &ResolvedUpdateTarget,
    min_version: &Version,
) -> Option<HostAssessment> {
    let assessment = host_policy::assess_for_tool_with_allowed_addons(
        &target.game_dir,
        &target.proxy_dll_name,
        "Luma",
        Some(min_version),
        &[target.addon_file.as_str()],
    );
    if assessment.is_known_custom_build() {
        return None;
    }
    Some(assessment)
}

/// Whether update/prepare may rewrite the proxy slot.
///
/// - **Owned** hosts: InstallNew / AdoptEmpty / RepairEmpty.
/// - **Untracked** hosts on a managed Luma record: only empty-slot installs
///   (`InstallNew` / `RepairEmpty`) so we never clobber a foreign runtime.
#[must_use]
pub(crate) fn host_rewrite_allowed(
    owns_host: bool,
    conflict: bool,
    lifecycle: HostLifecycle,
) -> bool {
    if conflict {
        return false;
    }
    if owns_host {
        return matches!(
            lifecycle,
            HostLifecycle::InstallNew | HostLifecycle::AdoptEmpty | HostLifecycle::RepairEmpty
        );
    }
    // Self-heal: empty proxy slot with no ownership yet.
    matches!(
        lifecycle,
        HostLifecycle::InstallNew | HostLifecycle::RepairEmpty
    )
}

/// Whether a rewriteable host actually needs new bytes on disk.
///
/// `action_writes_host` mirrors [`HostAssessment::writes_host`] (action-based);
/// a missing file yields `current_digest = None` and always needs a write.
#[must_use]
pub(crate) fn host_needs_write(
    action_writes_host: bool,
    current_digest: Option<&str>,
    nightly_digest: &str,
) -> bool {
    action_writes_host || current_digest != Some(nightly_digest)
}

/// Check status when HEAD/ETag already reports the recorded host as current.
/// Empty / under-min hosts still need a rewrite even when provenance matches.
#[must_use]
pub(crate) fn host_status_when_validators_match(lifecycle: HostLifecycle) -> UpdateStatus {
    match lifecycle {
        HostLifecycle::InstallNew | HostLifecycle::RepairEmpty => UpdateStatus::Available,
        HostLifecycle::AdoptEmpty | HostLifecycle::ReuseUser => UpdateStatus::Current,
        HostLifecycle::Conflict => UpdateStatus::Unknown,
    }
}

/// Check status after comparing on-disk host digest to the nightly download.
#[must_use]
pub(crate) fn host_status_from_digests(
    lifecycle: HostLifecycle,
    current_digest: &str,
    nightly_digest: &str,
) -> UpdateStatus {
    match lifecycle {
        // Missing host is handled by the caller; treat InstallNew defensively
        // as available (empty slot that still needs a write).
        HostLifecycle::InstallNew | HostLifecycle::RepairEmpty => {
            if current_digest != nightly_digest {
                UpdateStatus::Available
            } else {
                // Empty/under-min lifecycle still wants a rewrite even if digests
                // already match (rare: file present but lifecycle says repair).
                UpdateStatus::Available
            }
        }
        HostLifecycle::AdoptEmpty => {
            if current_digest != nightly_digest {
                UpdateStatus::Available
            } else {
                UpdateStatus::Current
            }
        }
        // User content is never rewritten. Digest match means the host is fine;
        // mismatch stays Unknown (cannot auto-update without stomping presets).
        HostLifecycle::ReuseUser => {
            if current_digest == nightly_digest {
                UpdateStatus::Current
            } else {
                UpdateStatus::Unknown
            }
        }
        HostLifecycle::Conflict => UpdateStatus::Unknown,
    }
}

#[cfg(test)]
mod host_policy_tests {
    use super::*;
    use crate::addons::reshade::host_policy::HostLifecycle;

    #[test]
    fn rewrite_allowed_matrix() {
        assert!(host_rewrite_allowed(true, false, HostLifecycle::InstallNew));
        assert!(host_rewrite_allowed(true, false, HostLifecycle::AdoptEmpty));
        assert!(host_rewrite_allowed(
            true,
            false,
            HostLifecycle::RepairEmpty
        ));
        assert!(!host_rewrite_allowed(true, false, HostLifecycle::ReuseUser));
        assert!(!host_rewrite_allowed(true, true, HostLifecycle::InstallNew));
        assert!(host_rewrite_allowed(
            false,
            false,
            HostLifecycle::InstallNew
        ));
        assert!(host_rewrite_allowed(
            false,
            false,
            HostLifecycle::RepairEmpty
        ));
        assert!(!host_rewrite_allowed(
            false,
            false,
            HostLifecycle::AdoptEmpty
        ));
        assert!(!host_rewrite_allowed(
            false,
            false,
            HostLifecycle::ReuseUser
        ));
    }

    #[test]
    fn needs_write_when_missing_or_digest_differs() {
        assert!(host_needs_write(true, None, "nightly"));
        assert!(host_needs_write(true, Some("nightly"), "nightly"));
        assert!(!host_needs_write(false, Some("nightly"), "nightly"));
        assert!(host_needs_write(false, Some("old"), "nightly"));
        assert!(host_needs_write(false, None, "nightly"));
    }

    #[test]
    fn validators_match_status_matrix() {
        assert_eq!(
            host_status_when_validators_match(HostLifecycle::InstallNew),
            UpdateStatus::Available
        );
        assert_eq!(
            host_status_when_validators_match(HostLifecycle::RepairEmpty),
            UpdateStatus::Available
        );
        assert_eq!(
            host_status_when_validators_match(HostLifecycle::AdoptEmpty),
            UpdateStatus::Current
        );
        assert_eq!(
            host_status_when_validators_match(HostLifecycle::ReuseUser),
            UpdateStatus::Current
        );
        assert_eq!(
            host_status_when_validators_match(HostLifecycle::Conflict),
            UpdateStatus::Unknown
        );
    }

    #[test]
    fn digest_status_matrix() {
        assert_eq!(
            host_status_from_digests(HostLifecycle::InstallNew, "a", "b"),
            UpdateStatus::Available
        );
        assert_eq!(
            host_status_from_digests(HostLifecycle::RepairEmpty, "same", "same"),
            UpdateStatus::Available
        );
        assert_eq!(
            host_status_from_digests(HostLifecycle::AdoptEmpty, "same", "same"),
            UpdateStatus::Current
        );
        assert_eq!(
            host_status_from_digests(HostLifecycle::AdoptEmpty, "old", "new"),
            UpdateStatus::Available
        );
        assert_eq!(
            host_status_from_digests(HostLifecycle::ReuseUser, "same", "same"),
            UpdateStatus::Current
        );
        assert_eq!(
            host_status_from_digests(HostLifecycle::ReuseUser, "old", "new"),
            UpdateStatus::Unknown
        );
    }
}
