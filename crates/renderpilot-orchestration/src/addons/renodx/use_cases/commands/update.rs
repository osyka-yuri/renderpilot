//! Applies updates to installed RenoDX add-ons and host artifacts.

use renderpilot_domain::{AddonKind, GameId};

use crate::addons::progress::emit_tool_finalizing;
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::reshade::types::ReshadeSourceCatalog;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

mod commit;
mod prepare;
mod snapshot;

#[cfg(test)]
mod tests;

use commit::authorize_update_commit;
use prepare::prepare_update_artifacts;
use snapshot::{ensure_update_snapshot_matches, resolve_update_snapshot};

/// Complete request for a generic RenoDX update.
pub struct UpdateRequest<'a> {
    /// Application services and storage.
    pub context: &'a Context,
    /// RenoDX manifest used to resolve the update.
    pub manifest: &'a RenoDxManifest,
    /// ReShade sources used when the host must be updated.
    pub reshade_sources: &'a ReshadeSourceCatalog,
    /// Game whose RenoDX installation is being updated.
    pub game_id: &'a GameId,
    /// Fresh permits for every mutation scope the resolved update may require.
    pub safety: crate::GameMutationSafetyPermits,
    /// Optional download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

/// Applies an update to the main RenoDX add-on and host artifacts only.
/// DLSS-Fix has an independent update/repair command and its source/path
/// projection is copied byte-for-byte through this generic transaction.
///
/// Network prepare for per-game artifacts runs **outside** the per-game lock
/// (same 3-phase contract as Luma update). Shared Vulkan layer updates still
/// apply under the lock in phase 3 (system-wide mutation).
pub async fn update(request: UpdateRequest<'_>) -> Result<(), ServiceError> {
    let UpdateRequest {
        context,
        manifest,
        reshade_sources,
        game_id,
        safety,
        progress,
    } = request;
    // Phase 1: snapshot under the per-game lock.
    let snapshot = {
        let _guard =
            crate::mutation_boundary::enter_game_mutation_boundary_async(context, game_id).await?;
        resolve_update_snapshot(context, manifest, reshade_sources, game_id)?
    };

    // Phase 2: downloads only for per-game sources (no disk apply).
    let prepared = prepare_update_artifacts(&snapshot, progress).await?;
    let shared_update = match snapshot.shared_vulkan_channel {
        Some(channel) => Some(
            crate::addons::renodx::use_cases::commands::update_reshade::PreparedReShadeUpdate::prepare(
                reshade_sources,
                channel,
                progress,
            )
            .await?,
        ),
        None => None,
    };

    // Phase 3: re-lock, revalidate, shared Vulkan (if any), apply.
    // Peer exclusivity is not re-checked here: one installed-addon row per game
    // plus our own record already blocks foreign tools for the duration of prepare.
    let guards = crate::mutation_boundary::enter_mutation_boundary_async(
        context,
        game_id,
        shared_update.is_some(),
    )
    .await?;
    let revalidated = resolve_update_snapshot(context, manifest, reshade_sources, game_id)?;
    ensure_update_snapshot_matches(&snapshot, &revalidated)?;
    let current = &revalidated.record;

    emit_tool_finalizing(progress, AddonKind::RenoDx);
    let replacement_paths = prepared.replacement_paths();
    let host_install_path = prepared.host_install_path();
    let targets = crate::addons::renodx::mutation_targets::update_targets(
        &revalidated.record,
        &replacement_paths,
        host_install_path.as_deref(),
    )?;
    match shared_update {
        Some(shared_update) => commit::authorize_combined_update(commit::CombinedUpdateRequest {
            context,
            guards,
            safety: &safety,
            shared_update,
            artifacts: prepared,
            current,
            targets,
            game_id,
        }),
        None => authorize_update_commit(context, guards, &safety, |guard| {
            commit::apply_update(commit::UpdateCommit {
                context,
                guard,
                artifacts: prepared,
                current,
                targets,
                game_id,
            })
        }),
    }
}
