//! Update detection for installed Luma add-ons.

use renderpilot_domain::{AddonKind, GameId, InstalledAddon, TrackedSourceRole};

use crate::addons::luma::dto::update::LumaUpdateReport;
use crate::addons::luma::tracking;
use crate::addons::luma::types::LumaManifest;
use crate::addons::luma::use_cases::update_target;
use crate::addons::records::{self, source_with_role};
use crate::addons::reshade::types::ReshadeSourceCatalog;
use crate::addons::update::UpdateStatus;
use crate::{Context, ServiceError};

mod probe;
#[cfg(test)]
mod tests;

use probe::{check_addon, check_dgvoodoo, check_host, elevate_addon_if_torn};

/// Checks whether the installed add-on for `game_id` has an upstream update. A
/// record belonging to a different addon kind (e.g. RenoDX) reads as "nothing
/// installed" — never checked as if it were a Luma install.
///
/// `deep: true` runs full ZIP / host-archive identity. `deep: false` stays on
/// HEAD/build-number + disk intactness, except a one-shot release-ZIP bind for
/// unbound advisory payloads after DB-loss adoption (host nightlies never
/// auto-download on passive).
pub async fn check_update(
    context: &Context,
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: &GameId,
    deep: bool,
) -> Result<LumaUpdateReport, ServiceError> {
    match records::record_of_kind(context, game_id, AddonKind::Luma)? {
        Some(record) => Ok(check_record(context, manifest, reshade_sources, &record, deep).await),
        None => Ok(LumaUpdateReport::new(None, None, None)),
    }
}

/// Bulk update check over every active Luma record. The shared records layer
/// applies kind and tool-presence policy before anything is update-checked.
///
/// Passive path: hosts stay cheap; unbound advisory payloads still one-shot
/// bind ZIP provenance per game.
pub async fn check_updates(
    context: &Context,
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
) -> Result<Vec<(GameId, UpdateStatus)>, ServiceError> {
    let records = installed_luma_records(context)?;
    let mut out = Vec::new();
    for record in records {
        let report = check_record(context, manifest, reshade_sources, &record, false).await;
        out.push((record.game_id().clone(), report.overall));
    }
    Ok(out)
}

/// Produces the honest bulk result when the live manifest is unavailable.
/// Presentation layers use this instead of reaching through [`Context`] to
/// storage or returning an empty map that would imply no installed Luma add-ons.
pub fn unknown_updates_for_installed(
    context: &Context,
) -> Result<Vec<(GameId, UpdateStatus)>, ServiceError> {
    Ok(installed_luma_records(context)?
        .map(|record| (record.game_id().clone(), UpdateStatus::Unknown))
        .collect())
}

fn installed_luma_records(
    context: &Context,
) -> Result<impl Iterator<Item = InstalledAddon>, ServiceError> {
    records::active_records_of_kind(context, AddonKind::Luma)
}

async fn check_record(
    context: &Context,
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
    record: &InstalledAddon,
    deep: bool,
) -> LumaUpdateReport {
    if !record.has_addon_source() {
        log::error!(
            "invalid Luma install record for `{}`: missing add-on payload provenance",
            record.game_id()
        );
        return LumaUpdateReport::new(Some(UpdateStatus::Unknown), None, None);
    }

    // Resolved once and shared: both the addon's tier-3 fallback and the host
    // check need the freshly re-resolved asset/arch/proxy-slot, none of which is
    // persisted on the record itself.
    let Some(target) = update_target::resolve_update_target(context, manifest, record.game_id())
        .ok()
        .flatten()
    else {
        return LumaUpdateReport::new(
            Some(UpdateStatus::Unknown),
            source_with_role(record, TrackedSourceRole::HostBinary)
                .is_some()
                .then_some(UpdateStatus::Unknown),
            source_with_role(record, TrackedSourceRole::DgVoodooWrapper)
                .is_some()
                .then_some(UpdateStatus::Unknown),
        );
    };
    // Recovery bind is payload-only — never force a multi-hundred MB nightly
    // host download on a passive game-details open.
    let addon_deep = deep || tracking::payload_needs_provenance_bind(record);
    let addon = check_addon(context, record, Some(&target), addon_deep).await;
    let host = check_host(record, manifest, reshade_sources, Some(&target), deep).await;
    let dgvoodoo = check_dgvoodoo(record, Some(&target));
    // A torn install sentinel means a prior op did not finish cleanly. Payload
    // files may still match ETag/digest, so cheap check would report `current`
    // while prepare already full-reconverges on `had_torn_marker`. Elevate the
    // addon verdict so Update becomes eligible. Only when we know the sentinel
    // root (resolved target.game_dir == UpdateLayout::sentinel_dir).
    let addon = elevate_addon_if_torn(addon, Some(&target));
    LumaUpdateReport::new(addon, host, dgvoodoo)
}
