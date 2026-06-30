//! Switches the recorded ReShade channel for RenoDX installs.

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, RenoDxInstallState, TrackedSource,
    TrackedSourceRole,
};

use crate::addons::engine::{self, FileOp, InstallPlan, InstallReceipt};
use crate::addons::renodx::channel;
use crate::addons::renodx::errors;
use crate::addons::renodx::facts::{analyze_game, install_target_dir};
use crate::addons::renodx::fetch;
use crate::addons::renodx::game_context::{executable_override, require_game};
use crate::addons::renodx::matcher::{RenoDxResolution, resolve};
use crate::addons::renodx::operation_lock;
use crate::addons::renodx::progress::emit_finalizing;
use crate::addons::renodx::source;
use crate::addons::renodx::tracking;
use crate::addons::renodx::types::{RenoDxManifest, ReshadeChannel};
use crate::addons::renodx::use_cases::reshade_update::{self, recorded_reshade_channel};
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
    let record = context
        .storage()
        .get_installed_addon(game_id)?
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

    let game = require_game(context, game_id)?;
    let analysis = analyze_game(&game, executable_override(context, game_id).as_deref());
    let resolution = resolve(manifest, &analysis.facts);
    let (arch, proxy_dll_name) = match resolution {
        RenoDxResolution::Installable(plan) => (plan.arch, plan.proxy_dll_name.clone()),
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => (plan.arch, plan.proxy_dll_name.clone()),
        _ => {
            return Err(errors::invalid(
                "cannot resolve the ReShade proxy slot for this game".to_owned(),
            ));
        }
    };
    let source = source::require_reshade_source(&manifest.reshade, target_channel, arch)?;
    let download = fetch::fetch_reshade_from_source(&source, arch, progress).await?;

    let host_path = tracking::rollback_host_path(&record).unwrap_or_else(|| {
        install_target_dir(&analysis)
            .unwrap_or_default()
            .join(&proxy_dll_name)
    });
    if !host_path.is_file() {
        return Err(errors::invalid(
            "ReShade host binary is missing; repair it before switching channel".to_owned(),
        ));
    }
    let game_dir = host_path
        .parent()
        .ok_or_else(|| errors::invalid("ReShade host path has no parent directory".to_owned()))?;
    emit_finalizing(progress);
    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::BackupAndReplace {
            name: proxy_dll_name.clone(),
            bytes: download.bytes.clone(),
        }],
    };
    let receipt = engine::install(game_dir, &plan)?;

    let new_source = reshade_update::host_binary_source(
        source.url,
        download.etag,
        download.digest,
        download.last_modified,
        Some(target_channel),
    );
    let updated = rebuild_proxy_switch_record(&record, new_source, Some(&receipt), target_channel)?;
    if let Err(error) = context.storage().upsert_installed_addon(&updated) {
        let dll_restore = engine::uninstall(&receipt.created_files, &receipt.backed_up_files);
        return Err(reshade_update::persistence_failure_error(
            error.into(),
            &[dll_restore],
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
