use renderpilot_domain::{AddonKind, InstalledAddon, TrackedSource, TrackedSourceRole};

use renderpilot_application::InstalledAddonRepository;

use crate::Context;
use crate::addons::luma::fetch::types::LumaPayload;
use crate::addons::records::{self, source_with_role};
use crate::addons::tracking;
use crate::game_mutation_lock;

use super::rebuild;

/// Non-HTTP sentinel stored in `last_modified` when a deep advisory check found
/// Available but the ZIP response carried no ETag/Last-Modified. Dual-uses the
/// validator fields as a bind mark so passive probes stop re-downloading.
pub(crate) const ADVISORY_PAYLOAD_CHECKED_MARK: &str = "advisory-deep-checked";

/// True when the source already carries a bind mark (real HTTP validators or the
/// deep-checked sentinel).
#[must_use]
pub(crate) fn source_has_bind_mark(source: &TrackedSource) -> bool {
    source.etag().is_some() || source.last_modified().is_some()
}

/// Replaces an advisory AddonPayload source with real ZIP provenance from a
/// fully validated download whose content digest matched the advisory digest.
/// Used by both update prepare and update-check so a successful advisory match
/// stops re-downloading the full ZIP on every probe.
pub(crate) fn promote_advisory_payload_source(
    sources: &mut Vec<TrackedSource>,
    advisory: &TrackedSource,
    payload: &LumaPayload,
) {
    debug_assert!(advisory.is_advisory());
    debug_assert_eq!(advisory.role(), TrackedSourceRole::AddonPayload);
    replace_addon_payload(
        sources,
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            advisory.url().to_owned(),
            payload.etag.clone(),
            payload.zip_digest.clone(),
        )
        .with_last_modified(payload.last_modified.clone()),
    );
}

/// Keeps the advisory disk digest but attaches ZIP HTTP validators (or the
/// checked sentinel) so [`payload_needs_provenance_bind`] becomes false while
/// the source stays advisory until an update reconverges.
pub(crate) fn mark_advisory_payload_source(
    sources: &mut Vec<TrackedSource>,
    advisory: &TrackedSource,
    payload: &LumaPayload,
) {
    debug_assert!(advisory.is_advisory());
    debug_assert_eq!(advisory.role(), TrackedSourceRole::AddonPayload);
    replace_addon_payload(
        sources,
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            advisory.url().to_owned(),
            payload.etag.clone(),
            advisory.digest().to_owned(),
        )
        .with_last_modified(
            payload
                .last_modified
                .clone()
                .or_else(|| Some(ADVISORY_PAYLOAD_CHECKED_MARK.to_owned())),
        )
        .with_advisory(),
    );
}

fn replace_addon_payload(sources: &mut Vec<TrackedSource>, replacement: TrackedSource) {
    sources.retain(|source| source.role() != TrackedSourceRole::AddonPayload);
    sources.push(replacement);
}

/// Version label to persist after a validated payload is in hand.
///
/// Prefer `Build N` from the ZIP's release tag when parseable; otherwise keep
/// the record's existing label so a missing tag never clears a known version.
/// Adopted (DB-loss) installs often start with `addon_version = None` -- this is
/// how promotion arms the cheap build-number fast path for later passive probes.
#[must_use]
pub(crate) fn resolved_addon_version(
    record: &InstalledAddon,
    payload: &LumaPayload,
) -> Option<String> {
    payload
        .build_number
        .map(crate::addons::luma::source::build_label)
        .or_else(|| record.addon_version().map(str::to_owned))
}

/// True when an advisory payload still needs a one-shot ZIP identity bind.
/// False after promote (Current) or a deep Available mark (bind mark attached).
#[must_use]
pub(crate) fn payload_needs_provenance_bind(record: &InstalledAddon) -> bool {
    source_with_role(record, TrackedSourceRole::AddonPayload)
        .is_some_and(|source| source.is_advisory() && !source_has_bind_mark(source))
}

/// Best-effort: after a deep advisory check (or update prepare) matched a
/// validated ZIP, promote the stored AddonPayload source to real ZIP provenance
/// so later passive probes use HEAD/ETag without re-downloading.
///
/// Passive probes reach here only when elevated for DB-loss recovery. Failures
/// are logged and ignored — callers already returned `Current`. Holds the
/// per-game `game_mutation_lock` while reloading and upserting. Skips
/// `recover_pending` (DB-only, not a file mutation).
///
/// `still_current` re-validates the reloaded advisory source against `payload`.
pub(crate) async fn try_promote_advisory_payload(
    context: &Context,
    game_id: &renderpilot_domain::GameId,
    advisory_digest: &str,
    payload: &LumaPayload,
    still_current: impl FnOnce(&TrackedSource, &LumaPayload) -> bool,
) {
    let _guard = game_mutation_lock::lock(game_id).await;
    let Some(current) = reload_advisory_payload(context, game_id, advisory_digest) else {
        return;
    };
    let Some(current_source) = source_with_role(&current, TrackedSourceRole::AddonPayload) else {
        return;
    };
    if !still_current(current_source, payload) {
        return;
    }

    let mut sources = current.tracked_sources().to_vec();
    promote_advisory_payload_source(&mut sources, current_source, payload);
    persist_rebuilt(
        context,
        game_id,
        &current,
        sources,
        resolved_addon_version(&current, payload),
        "promoted advisory payload provenance after update check",
    );
}

/// After a deep advisory check found Available, attach ZIP validators onto the
/// still-advisory source and bind `addon_version` when parseable.
pub(crate) async fn try_mark_advisory_payload_checked(
    context: &Context,
    game_id: &renderpilot_domain::GameId,
    advisory_digest: &str,
    payload: &LumaPayload,
) {
    let _guard = game_mutation_lock::lock(game_id).await;
    let Some(current) = reload_advisory_payload(context, game_id, advisory_digest) else {
        return;
    };
    let Some(current_source) = source_with_role(&current, TrackedSourceRole::AddonPayload) else {
        return;
    };
    if source_has_bind_mark(current_source) {
        return;
    }

    let mut sources = current.tracked_sources().to_vec();
    mark_advisory_payload_source(&mut sources, current_source, payload);
    persist_rebuilt(
        context,
        game_id,
        &current,
        sources,
        resolved_addon_version(&current, payload),
        "marked advisory payload deep-checked after available update probe",
    );
}

fn reload_advisory_payload(
    context: &Context,
    game_id: &renderpilot_domain::GameId,
    advisory_digest: &str,
) -> Option<InstalledAddon> {
    let current = match records::record_of_kind(context, game_id, AddonKind::Luma) {
        Ok(record) => record?,
        Err(error) => {
            log::warn!("Luma advisory bind: failed to re-load install for `{game_id}`: {error}");
            return None;
        }
    };
    let current_source = source_with_role(&current, TrackedSourceRole::AddonPayload)?;
    if !current_source.is_advisory() || current_source.digest() != advisory_digest {
        return None;
    }
    Some(current)
}

fn persist_rebuilt(
    context: &Context,
    game_id: &renderpilot_domain::GameId,
    current: &InstalledAddon,
    sources: Vec<TrackedSource>,
    addon_version: Option<String>,
    label: &str,
) {
    let refreshed = match rebuild(
        current,
        tracking::RebuildParts {
            addon_file: current.addon_file().clone(),
            addon_version: tracking::AddonVersionUpdate::Set(addon_version),
            managed_files: tracking::ManagedFilesUpdate::Keep,
            created_files: current.created_files().to_vec(),
            backed_up_files: current.backed_up_files().to_vec(),
            tracked_sources: sources,
            label: label.to_owned(),
        },
    ) {
        Ok(record) => record,
        Err(error) => {
            log::warn!("Luma advisory bind: failed to rebuild record for `{game_id}`: {error}");
            return;
        }
    };
    if let Err(error) = context.storage().upsert_installed_addon(&refreshed) {
        log::warn!("Luma advisory bind: failed to persist for `{game_id}`: {error}");
    }
}
