//! Switches the recorded ReShade channel for RenoDX installs.

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, RenoDxInstallState, TrackedSource,
    TrackedSourceRole,
};

use crate::addons::engine::InstallReceipt;
use crate::addons::file_update::{
    Replacement, apply_replacements, persistence_failure_error, restore_originals,
    restore_originals_best_effort,
};
use crate::addons::operation_lock;
use crate::addons::progress::emit_tool_finalizing;
use crate::addons::records;
use crate::addons::renodx::errors;
use crate::addons::renodx::tracking;
use crate::addons::renodx::types::RenoDxManifest;
use crate::addons::renodx::use_cases::reshade_update::{
    recorded_reshade_channel, resolve_host_update_target,
};
use crate::addons::reshade::channel;
use crate::addons::reshade::fetch::fetch_reshade_from_source;
use crate::addons::reshade::types::ReshadeChannel;
use crate::addons::reshade::update::host_binary_source;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Switches the recorded ReShade host binary artifact between stable and nightly.
pub async fn switch_reshade_channel(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    target_channel: ReshadeChannel,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    let _guard = operation_lock::lock(game_id).await;
    let record = records::record_of_kind(context, game_id, AddonKind::RenoDx)?
        .ok_or_else(errors::not_installed)?;
    if matches!(
        record.host_kind(),
        Some(InstalledAddonHostKind::SharedVulkanLayer)
    ) {
        return switch_vulkan_reshade_channel(context, manifest, record, target_channel, progress)
            .await;
    }
    let host_source =
        channel::single_host_source(&record).map_err(|_| errors::duplicate_host_sources())?;

    if !manifest.reshade.supports_channel(target_channel) {
        return Err(errors::channel_unavailable(target_channel));
    }
    let current = recorded_reshade_channel(&record);

    if current == Some(target_channel) {
        let healed = if let Some(host_source) = host_source {
            let healed_source = channel::with_host_channel(host_source, target_channel);
            tracking::replace_host_source(&record, &healed_source)?
        } else {
            record.with_reshade_channel(target_channel.as_str())
        }
        .with_reshade_channel(target_channel.as_str());
        context.storage().upsert_installed_addon(&healed)?;
        return Ok(tracking::install_state_from_record(&healed));
    }

    // `resolve_host_update_target` also returns `None` for a recognized custom
    // build (e.g. GShade) — RenoDX doesn't manage its channel either, and the
    // action isn't offered in the UI for one in the first place.
    let target = resolve_host_update_target(context, manifest, game_id, target_channel)?
        .ok_or_else(|| {
            errors::invalid("cannot resolve the ReShade proxy slot for this game".to_owned())
        })?;
    if target.conflict {
        return Err(errors::invalid(
            "ReShade host conflict must be resolved before switching channel".to_owned(),
        ));
    }
    if !target.target_path.is_file() {
        return Err(errors::invalid(
            "ReShade host binary is missing; repair it before switching channel".to_owned(),
        ));
    }

    let download = fetch_reshade_from_source(&target.source, target.arch, progress).await?;
    emit_tool_finalizing(progress, AddonKind::RenoDx);
    let originals = apply_replacements(vec![Replacement {
        path: target.target_path.clone(),
        bytes: download.bytes,
        mtime: None,
    }])?;

    let new_source = host_binary_source(
        target.source.url.clone(),
        download.etag,
        download.digest,
        download.last_modified,
        Some(target_channel),
    );
    // The record may not have tracked this exact path before (a legacy record
    // adopted without host provenance, or the active slot changed) — carry it
    // through as a receipt so the rebuild below adds it to `created_files`.
    let receipt = InstallReceipt {
        created_files: vec![target.target_path.clone()],
        backed_up_files: Vec::new(),
    };
    let updated =
        match rebuild_proxy_switch_record(&record, new_source, Some(&receipt), target_channel) {
            Ok(updated) => updated,
            Err(error) => {
                restore_originals_best_effort(&originals);
                return Err(error);
            }
        };
    if let Err(error) = context.storage().upsert_installed_addon(&updated) {
        let restore_result = restore_originals(&originals);
        return Err(persistence_failure_error(
            error.into(),
            std::slice::from_ref(&restore_result),
        ));
    }
    Ok(tracking::install_state_from_record(&updated))
}

fn replace_or_append_host_source(
    record: &InstalledAddon,
    new_source: TrackedSource,
) -> Vec<TrackedSource> {
    let mut sources = record.tracked_sources().to_vec();
    let mut replaced = false;
    for entry in &mut sources {
        if entry.role() == TrackedSourceRole::HostBinary {
            *entry = new_source.clone();
            replaced = true;
        }
    }
    if !replaced {
        sources.push(new_source);
    }
    sources
}

fn rebuild_proxy_switch_record(
    record: &InstalledAddon,
    new_source: TrackedSource,
    receipt: Option<&InstallReceipt>,
    target_channel: ReshadeChannel,
) -> Result<InstalledAddon, ServiceError> {
    tracking::rebuild_with_sources_and_receipt(
        record,
        replace_or_append_host_source(record, new_source),
        receipt,
        "RenoDX channel switch",
    )
    .map(|updated| updated.with_reshade_channel(target_channel.as_str()))
}

async fn switch_vulkan_reshade_channel(
    context: &Context,
    manifest: &RenoDxManifest,
    record: InstalledAddon,
    target_channel: ReshadeChannel,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    if !manifest.reshade.supports_channel(target_channel) {
        return Err(errors::channel_unavailable(target_channel));
    }
    let target_channel = manifest.reshade.effective_install_channel(target_channel);
    crate::addons::renodx::use_cases::commands::update_reshade::UpdateReShadeCommand {
        context,
        manifest,
        channel: target_channel,
        progress,
    }
    .execute()
    .await?;

    let updated = record.with_reshade_channel(target_channel.as_str());
    context.storage().upsert_installed_addon(&updated)?;
    Ok(tracking::install_state_from_record(&updated))
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{AddonKind, GameId, PathRef};

    use super::*;

    fn record_with_sources(sources: Vec<TrackedSource>) -> InstalledAddon {
        let addon = PathRef::new(r"C:\Games\Test\renodx-test.addon64").expect("path");
        InstalledAddon::from_parts(
            GameId::new("steam:42").expect("id"),
            AddonKind::RenoDx,
            addon.clone(),
            None,
            vec![addon],
            Vec::new(),
            sources,
        )
        .expect("record")
    }

    fn source(role: TrackedSourceRole, url: &str, digest: &str) -> TrackedSource {
        TrackedSource::new(role, url, None, digest)
    }

    #[test]
    fn host_source_replacement_appends_for_legacy_records_without_host_source() {
        let record = record_with_sources(vec![source(
            TrackedSourceRole::AddonPayload,
            "https://example/renodx.addon64",
            "addon-digest",
        )]);
        let host = source(
            TrackedSourceRole::HostBinary,
            "https://reshade.me/downloads/ReShade_Setup.exe",
            "host-digest",
        );

        let sources = replace_or_append_host_source(&record, host);

        assert_eq!(sources.len(), 2);
        assert_eq!(
            sources
                .iter()
                .filter(|source| source.role() == TrackedSourceRole::HostBinary)
                .count(),
            1
        );
    }

    #[test]
    fn host_source_replacement_replaces_existing_host_source() {
        let record = record_with_sources(vec![
            source(
                TrackedSourceRole::AddonPayload,
                "https://example/renodx.addon64",
                "addon-digest",
            ),
            source(
                TrackedSourceRole::HostBinary,
                "https://old.example/ReShade.exe",
                "old-host-digest",
            ),
        ]);
        let host = source(
            TrackedSourceRole::HostBinary,
            "https://reshade.me/downloads/ReShade_Setup.exe",
            "new-host-digest",
        );

        let sources = replace_or_append_host_source(&record, host);

        assert_eq!(sources.len(), 2);
        let host = sources
            .iter()
            .find(|source| source.role() == TrackedSourceRole::HostBinary)
            .expect("host source");
        assert_eq!(host.digest(), "new-host-digest");
    }

    #[test]
    fn proxy_switch_record_updates_top_level_channel() {
        let record = record_with_sources(vec![source(
            TrackedSourceRole::HostBinary,
            "https://reshade.me/downloads/ReShade_Setup.exe",
            "old-host-digest",
        )])
        .with_reshade_channel("stable");
        let host = source(
            TrackedSourceRole::HostBinary,
            "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip",
            "new-host-digest",
        )
        .with_channel("nightly");

        let updated = rebuild_proxy_switch_record(&record, host, None, ReshadeChannel::Nightly)
            .expect("switch record");

        assert_eq!(updated.reshade_channel(), Some("nightly"));
        assert_eq!(
            recorded_reshade_channel(&updated),
            Some(ReshadeChannel::Nightly)
        );
    }
}
