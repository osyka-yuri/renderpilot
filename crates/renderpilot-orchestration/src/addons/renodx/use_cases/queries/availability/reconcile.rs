use renderpilot_domain::GameId;

use crate::Context;
use crate::ServiceError;

use crate::addons::availability_pipeline::AvailabilityPreflight;
use crate::addons::renodx::matcher::RenoDxResolution;
use crate::addons::renodx::reconciliation;
use crate::addons::reshade::types::ReshadeSourceCatalog;

use super::super::host_report;
use super::candidate;

/// Adopts a recoverable on-disk install into the preflight record when missing.
/// Callers must hold the per-game `game_mutation_lock`.
///
/// Mutates `preflight.record` in place on success so the caller can build the
/// report from a single preflight without re-scanning.
pub(super) fn maybe_adopt(
    context: &Context,
    preflight: &mut AvailabilityPreflight<RenoDxResolution>,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
) -> Result<(), ServiceError> {
    // Availability is a read query. It may describe an existing row but must
    // never adopt/reconcile disk state while a mutation boundary still has a
    // pending transaction for this game.
    if !context
        .storage()
        .pending_file_mutations_for_game(game_id)?
        .is_empty()
    {
        return Ok(());
    }
    if preflight.blocked.is_some() || preflight.record.is_some() {
        return Ok(());
    }

    let host = host_report::reshade_report(
        &preflight.analysis,
        &preflight.resolution,
        None,
        reshade_sources,
    );
    if let Some(candidate) = candidate::orphaned_install_candidate(
        game_id,
        &preflight.analysis,
        &preflight.resolution,
        &host,
        reshade_sources,
    ) && let Some(adopted) =
        reconciliation::reconcile_orphaned_install_locked(context, &candidate)?
    {
        preflight.record = Some(adopted);
    }

    Ok(())
}
