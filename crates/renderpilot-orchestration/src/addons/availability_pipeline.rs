//! Shared availability preflight: record lookup, game load, exclusivity check,
//! and manifest resolution — the common front half of every addon availability query.

use renderpilot_domain::{AddonKind, GameId, GameInstallation, InstalledAddon};

use crate::addons::exclusivity::{self, ExclusivityBlock, ExclusivityBlockKind};
use crate::addons::game_analysis::{GameAnalysis, install_roots_for_analysis};
use crate::addons::game_context::{executable_override, require_game};
use crate::addons::records;
use crate::addons::reshade::InstallRoots;
use crate::{Context, ServiceError};

/// Why an availability preview is blocked by the other addon tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BlockedOutcome {
    /// The other addon tool occupying this game.
    pub(crate) other_kind: AddonKind,
    /// Whether the block came from an unmanaged on-disk install (`true`)
    /// rather than a tracked database record (`false`).
    pub(crate) unmanaged: bool,
}

/// Maps an exclusivity block to the simple struct both tools' availability
/// outcomes use for their respective blocked states.
#[must_use]
pub(crate) fn blocked_outcome(block: ExclusivityBlock) -> BlockedOutcome {
    BlockedOutcome {
        other_kind: block.other,
        unmanaged: block.kind == ExclusivityBlockKind::UnmanagedFiles,
    }
}

/// Everything the tool-specific availability query needs after the shared
/// preflight steps: record lookup, game load, exclusivity check, and resolution.
pub(crate) struct AvailabilityPreflight<R> {
    /// This tool's install record for the game, when one exists.
    pub(crate) record: Option<InstalledAddon>,
    /// The game's installation row.
    pub(crate) game: GameInstallation,
    /// Set when the other mutually-exclusive addon tool is already present.
    pub(crate) blocked: Option<ExclusivityBlock>,
    /// On-disk facts gathered from the game folder.
    pub(crate) analysis: GameAnalysis,
    /// This tool's manifest resolution for the game.
    pub(crate) resolution: R,
}

/// Runs the shared availability preflight: record lookup, [`require_game`],
/// analysis/resolution, then exclusivity against install scan roots.
///
/// Exclusivity runs **after** analysis so the unmanaged-file backstop scans the
/// same directories as install (`install_target_dir` + `InstallRoots`), not the
/// library install root (which can differ on nested Unreal layouts).
pub(crate) fn preflight<M, R, F>(
    context: &Context,
    game_id: &GameId,
    kind: AddonKind,
    manifest: &M,
    analyze_and_resolve: F,
) -> Result<AvailabilityPreflight<R>, ServiceError>
where
    F: FnOnce(&GameInstallation, &M, Option<&std::path::Path>) -> (GameAnalysis, R),
{
    let record = records::record_of_kind(context, game_id, kind)?;
    let game = require_game(context, game_id)?;
    let override_path = executable_override(context, game_id);
    let (analysis, resolution) = analyze_and_resolve(&game, manifest, override_path.as_deref());
    let roots = install_roots_for_analysis(&analysis);
    let blocked = {
        let scan_dirs = roots.as_ref().map(InstallRoots::scan_dir_paths);
        exclusivity::check_blocked(context, game_id, kind, scan_dirs.as_deref())?
    };
    Ok(AvailabilityPreflight {
        record,
        game,
        blocked,
        analysis,
        resolution,
    })
}
