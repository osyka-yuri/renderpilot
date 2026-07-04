/// Shared helpers for ReShade host update queries and commands.
use std::path::PathBuf;

use renderpilot_application::GameRepository;
use renderpilot_domain::{Architecture, GameId, InstalledAddon};

use crate::{Context, ServiceError};

use crate::addons::game_analysis::{analyze_game, install_target_dir};
use crate::addons::renodx::matcher::{RenoDxResolution, resolve};
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::reshade::channel;
use crate::addons::reshade::host_policy;
use crate::addons::reshade::scan::ReshadeHostAction;
use crate::addons::reshade::source::{ReshadeSource, require_reshade_source};
use crate::addons::reshade::types::ReshadeChannel;

/// Recorded ReShade channel, including legacy URL-derived records.
pub(crate) fn recorded_reshade_channel(record: &InstalledAddon) -> Option<ReshadeChannel> {
    record
        .reshade_channel()
        .and_then(|c| ReshadeChannel::parse_recorded(Some(c)).into_parsed())
        .or_else(|| {
            channel::installed_channel(record)
                .ok()
                .flatten()
                .and_then(|c| c.into_parsed())
        })
}

/// Resolved ReShade host update target for proxy installs.
pub(crate) struct HostUpdateTarget {
    /// Game directory holding the proxy host.
    pub(crate) game_dir: PathBuf,
    /// Proxy slot file name.
    pub(crate) slot: String,
    /// ReShade host architecture.
    pub(crate) arch: Architecture,
    /// Host policy action required by current disk state.
    pub(crate) action: ReshadeHostAction,
    /// Whether the host policy found a conflict.
    pub(crate) conflict: bool,
    /// ReShade source for the requested channel.
    pub(crate) source: ReshadeSource,
    /// Requested/effective channel.
    pub(crate) channel: ReshadeChannel,
    /// Existing target path for the host.
    pub(crate) target_path: PathBuf,
}

/// Resolves the target ReShade host and source for a proxy-host update. `Ok(None)`
/// both when there's nothing to resolve (game/title/host unresolvable) *and* when
/// the active slot is a recognized custom build (e.g. GShade, see
/// [`host_policy::HostAssessment::is_known_custom_build`]) — RenoDX never checks
/// it for updates or replaces it, so every caller gets that guarantee for free
/// without checking for it itself.
pub(crate) fn resolve_host_update_target(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    channel: ReshadeChannel,
) -> Result<Option<HostUpdateTarget>, ServiceError> {
    let Some(game) = context.storage().find_game(game_id)? else {
        return Ok(None);
    };
    let override_path = crate::nvapi::resolve::stored_override_path(context, game_id.as_str())
        .ok()
        .flatten();
    let analysis = analyze_game(&game, override_path.as_deref());
    let resolution = resolve(manifest, &analysis.facts);
    let (arch, proxy_dll_name) = match resolution {
        RenoDxResolution::Installable(plan) => (plan.arch, plan.proxy_dll_name.clone()),
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => (plan.arch, plan.proxy_dll_name.clone()),
        _ => return Ok(None),
    };
    let game_dir = install_target_dir(&analysis)?;
    let assessment = host_policy::assess(&game_dir, &proxy_dll_name);
    if assessment.is_known_custom_build() {
        return Ok(None);
    }
    let source = require_reshade_source(&manifest.reshade, channel, arch)?;
    Ok(Some(HostUpdateTarget {
        game_dir,
        slot: assessment.slot,
        arch,
        action: assessment.action,
        conflict: assessment.conflict,
        source,
        channel,
        target_path: assessment.target_path,
    }))
}
