//! Shared install preflight: resolve scan roots and enforce exclusivity + torn recovery.
//!
//! Callers hold the per-game [`crate::game_mutation_lock`] before calling these helpers.

use renderpilot_domain::{AddonKind, GameId};

use crate::addons::engine;
use crate::addons::exclusivity;
use crate::addons::game_analysis::{GameAnalysis, install_target_dir};
use crate::addons::reshade::InstallRoots;
use crate::addons::tool::require_tool;
use crate::{Context, ServiceError};

/// Resolves install scan roots from analysis (exe parent + optional split AddonPath).
pub(crate) fn resolve_install_scan_roots(
    analysis: &GameAnalysis,
) -> Result<InstallRoots, ServiceError> {
    let dir = install_target_dir(analysis)?;
    Ok(InstallRoots::resolve_from_ini(&dir))
}

/// Ensures the requesting tool is not blocked by a peer, then recovers a torn
/// install when a sentinel is present via [`crate::addons::tool::AddonTool::recover_torn`].
///
/// The user-facing block message comes from
/// [`crate::addons::tool::AddonTool::exclusive_block_message`].
pub(crate) fn guard_exclusivity_and_torn(
    context: &Context,
    game_id: &GameId,
    kind: AddonKind,
    roots: &InstallRoots,
) -> Result<(), ServiceError> {
    let tool = require_tool(kind);
    let scan_dirs = roots.scan_dir_paths();
    exclusivity::ensure_not_blocked(context, game_id, kind, Some(scan_dirs.as_slice()))?;
    if engine::is_install_torn(roots.sentinel_dir(), kind) {
        tool.recover_torn(scan_dirs.as_slice());
    }
    Ok(())
}
