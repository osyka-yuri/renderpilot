use std::path::{Path, PathBuf};

use renderpilot_domain::RenoDxConfigReceipt;
use renderpilot_domain::{AddonKind, InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::ServiceError;
use crate::addons::engine::{self, InstallPlan};
use crate::addons::record;
use crate::addons::reshade::split_install::{InstallRoots, PayloadRollback, run_split_install};

use super::super::errors;
use super::PreparedInstall;
use super::ops::{addon_op, combined_ops, host_ops, ini_op_for_game};
use crate::addons::renodx::game_participants::{self, GameParticipantPlan};
use crate::addons::reshade::host_policy;
use crate::addons::reshade::scan as reshade;
use crate::addons::reshade::update::host_binary_source;

/// Read-only proxy preparation shared by the ordinary split executor and the
/// combined Shared Vulkan reinstall transaction.
pub(crate) struct PreparedProxyInstall {
    roots: crate::addons::reshade::split_install::InstallRoots,
    unified_ops: Vec<engine::FileOp>,
    payload_ops: Vec<engine::FileOp>,
    game_ops: Vec<engine::FileOp>,
    addon_dir: PathBuf,
    writes_host: bool,
    adopted_existing: Vec<PathBuf>,
    config_receipt: Option<RenoDxConfigReceipt>,
}

impl PreparedProxyInstall {
    /// Returns the exact participant plan and record projection, together with
    /// all roots touched by the existing proxy layout policy.
    pub(crate) fn into_combined_parts(
        self,
        prepared: &PreparedInstall,
    ) -> Result<(GameParticipantPlan, InstalledAddon, Vec<PathBuf>), ServiceError> {
        let Self {
            roots,
            unified_ops,
            payload_ops,
            game_ops,
            writes_host,
            adopted_existing,
            ..
        } = self;
        let participants = if roots.is_unified {
            game_participants::build(
                &roots.game_dir,
                InstallPlan {
                    kind: AddonKind::RenoDx,
                    ops: unified_ops,
                },
            )?
        } else {
            let payload = game_participants::build(
                &roots.addon_dir,
                InstallPlan {
                    kind: AddonKind::RenoDx,
                    ops: payload_ops,
                },
            )?;
            let host = game_participants::build(
                &roots.game_dir,
                InstallPlan {
                    kind: AddonKind::RenoDx,
                    ops: game_ops,
                },
            )?;
            payload.merge(host)?
        };
        let record = build_record(
            prepared,
            &roots.addon_dir,
            writes_host,
            &adopted_existing,
            participants.receipt(),
            participants.config_receipt(),
        )?;
        let touched_roots = if roots.is_unified {
            vec![roots.game_dir]
        } else {
            vec![roots.game_dir, roots.addon_dir]
        };
        Ok((participants, record, touched_roots))
    }
}

/// Installs a Direct3D (proxy-DLL) RenoDX host.
///
/// Refuses if a RenoDX install record already exists here (the caller should
/// uninstall first to reinstall). When no ReShade host is present one is installed
/// (proxy DLL + fresh `ReShade.ini` + marker); a compatible empty host is
/// adopted, while a compatible user setup remains reused with only RenoDX's
/// additive, no-backup INI merge.
pub(super) fn install_proxy(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<(InstalledAddon, engine::PendingInstallCommit), ServiceError> {
    let prepared_proxy = prepare_proxy_install(game_dir, prepared)?;
    let PreparedProxyInstall {
        roots,
        unified_ops,
        payload_ops,
        game_ops,
        addon_dir,
        writes_host,
        adopted_existing,
        config_receipt,
        ..
    } = prepared_proxy;
    let success = run_split_install(
        &roots,
        AddonKind::RenoDx,
        unified_ops,
        payload_ops,
        game_ops,
        PayloadRollback::Flat,
    )?;
    let record = build_record(
        prepared,
        &addon_dir,
        writes_host,
        &adopted_existing,
        &success.receipt,
        config_receipt.as_ref(),
    )?;
    Ok((record, success.commit))
}

/// Resolves the ordinary proxy policy and all touched bytes without writing.
/// The generic split executor still applies ordinary installs; inactive
/// Shared-Vulkan transitions consume the same plan through their one SVAM
/// reservation instead.
pub(crate) fn prepare_proxy_install(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<PreparedProxyInstall, ServiceError> {
    let host = host_policy::assess(game_dir, &prepared.proxy_dll_name);
    host.ensure_initial_installable(&prepared.proxy_dll_name)?;
    if host.initial_writes_host() && prepared.reshade_dll_bytes.is_empty() {
        return Err(errors::invalid(
            "the active ReShade host needs installation or repair, but no ReShade bytes were provided"
                .to_owned(),
        ));
    }

    let paths = reshade::resolve_paths(game_dir, Some(&host.target_path));
    if !paths.effective_addon_path.is_dir() {
        return Err(errors::invalid(format!(
            "ReShade AddonPath `{}` does not exist",
            paths.effective_addon_path.display()
        )));
    }
    let adopted_existing = host.initial_owned_existing_paths(paths.ini_path.as_deref());
    let prepared_ini = ini_op_for_game(game_dir, prepared)?;
    let config_receipt = prepared_ini.as_ref().and_then(|operation| match operation {
        engine::FileOp::RenoDxConfig { receipt, .. } => Some(receipt.clone()),
        _ => None,
    });
    let roots = InstallRoots::resolve(game_dir, &host.target_path);
    let writes_host = host.initial_writes_host();
    let (unified_ops, payload_ops, game_ops) = if roots.is_unified {
        (
            combined_ops(prepared, writes_host, prepared_ini.as_ref()),
            Vec::new(),
            Vec::new(),
        )
    } else {
        (
            Vec::new(),
            vec![addon_op(prepared)],
            host_ops(prepared, writes_host, prepared_ini.as_ref()),
        )
    };
    Ok(PreparedProxyInstall {
        roots,
        unified_ops,
        payload_ops,
        game_ops,
        addon_dir: paths.effective_addon_path,
        writes_host,
        adopted_existing,
        config_receipt,
    })
}

/// Assembles the [`InstalledAddon`] from the engine receipt and the upstream entries
/// to track: the add-on (when fetched upstream) and any replaced/created ReShade host.
pub(super) fn build_record(
    prepared: &PreparedInstall,
    addon_dir: &Path,
    tracks_host: bool,
    adopted_existing: &[PathBuf],
    receipt: &engine::InstallReceipt,
    config_receipt: Option<&RenoDxConfigReceipt>,
) -> Result<InstalledAddon, ServiceError> {
    let addon_path = addon_dir.join(&prepared.addon_file_name);

    let mut sources = Vec::new();
    if !prepared.addon_source_url.is_empty() || prepared.source_last_modified.is_some() {
        sources.push(
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                prepared.addon_source_url.clone(),
                prepared.source_etag.clone(),
                prepared.source_digest.clone(),
            )
            .with_last_modified(prepared.source_last_modified.clone()),
        );
    }
    if tracks_host {
        sources.push(host_binary_source(
            prepared.reshade_source_url.clone(),
            prepared.reshade_source_etag.clone(),
            prepared.reshade_digest.clone(),
            prepared.reshade_last_modified.clone(),
            prepared.reshade_channel,
        ));
    }

    let record = record::build(
        prepared.game_id.clone(),
        AddonKind::RenoDx,
        &addon_path,
        receipt,
        sources,
    )?;
    let record = record
        .with_renodx_config_receipt(config_receipt.cloned())
        .map_err(|error| errors::invalid(error.to_string()))?;
    record::adopt_existing_paths(record, adopted_existing)
}

/// The upstream add-on entry to track for updates, or `None` for a file install
/// (empty URL and no mtime placeholder) which has nothing to track.
pub(super) fn addon_tracked_source(prepared: &PreparedInstall) -> Option<TrackedSource> {
    if prepared.addon_source_url.is_empty() && prepared.source_last_modified.is_none() {
        return None;
    }
    Some(
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            prepared.addon_source_url.clone(),
            prepared.source_etag.clone(),
            prepared.source_digest.clone(),
        )
        .with_last_modified(prepared.source_last_modified.clone()),
    )
}

/// Installs a Vulkan RenoDX add-on into `game_dir`.
///
/// The host is the shared Vulkan layer (installed separately by the service),
/// so this lays down only per-game files: the add-on and a `ReShade.ini` with
/// the required add-on configuration. Refuses if the add-on is already present.
pub(super) fn install_vulkan(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<(InstalledAddon, engine::PendingInstallCommit), ServiceError> {
    let addon_path = game_dir.join(&prepared.addon_file_name);
    if addon_path.is_file() {
        return Err(errors::invalid(
            "RenoDX is already installed for this game; uninstall before reinstalling".to_owned(),
        ));
    }

    let sources: Vec<TrackedSource> = addon_tracked_source(prepared).into_iter().collect();
    let plan = build_vulkan_plan(prepared, game_dir)?;
    let config_receipt = plan.ops.iter().find_map(|operation| match operation {
        engine::FileOp::RenoDxConfig { receipt, .. } => Some(receipt.clone()),
        _ => None,
    });
    let pending = engine::install_pending(game_dir, &plan)?;

    let record = record::build(
        prepared.game_id.clone(),
        AddonKind::RenoDx,
        &addon_path,
        &pending.receipt,
        sources,
    )?
    .with_renodx_config_receipt(config_receipt)
    .map_err(|error| errors::invalid(error.to_string()))?;
    Ok((record, pending.commit))
}

/// Builds the per-game file operations for a Vulkan install: the add-on and the
/// `ReShade.ini` merge. Install state is tracked in the app database.
pub(super) fn build_vulkan_plan(
    prepared: &PreparedInstall,
    game_dir: &Path,
) -> Result<InstallPlan, ServiceError> {
    let mut ops = vec![addon_op(prepared)];
    if let Some(ini_op) = ini_op_for_game(game_dir, prepared)? {
        ops.push(ini_op);
    }
    Ok(InstallPlan {
        kind: AddonKind::RenoDx,
        ops,
    })
}

/// Plans the exact touched game files for a combined Vulkan mutation.
///
/// This is deliberately separate from the generic engine executor: the shared
/// transaction must capture exact before/after bytes before it writes either
/// participant, while ordinary game-only installs retain the engine's baseline
/// sequential behavior.
pub(crate) fn build_vulkan_game_participants(
    prepared: &PreparedInstall,
    game_dir: &Path,
) -> Result<GameParticipantPlan, ServiceError> {
    let plan = build_vulkan_plan(prepared, game_dir)?;
    game_participants::build(game_dir, plan)
}

/// Maps the exact Vulkan game participant receipt to the normal RenoDX record.
pub(crate) fn build_vulkan_record(
    prepared: &PreparedInstall,
    game_dir: &Path,
    participants: &GameParticipantPlan,
) -> Result<InstalledAddon, ServiceError> {
    build_record(
        prepared,
        game_dir,
        false,
        &[],
        participants.receipt(),
        participants.config_receipt(),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use renderpilot_domain::GameId;
    use tempfile::{TempDir, tempdir};

    use super::*;

    fn prepared(game_id: &GameId) -> PreparedInstall {
        PreparedInstall {
            game_id: game_id.clone(),
            host_kind: crate::addons::reshade::proxy::HostKind::Proxy,
            proxy_dll_name: "dxgi.dll".to_owned(),
            addon_file_name: "renodx-test.addon64".to_owned(),
            addon_source_url: "https://example.test/renodx-test.addon64".to_owned(),
            source_digest: "source-digest".to_owned(),
            source_etag: None,
            source_last_modified: None,
            addon_bytes: b"addon".to_vec(),
            reshade_dll_bytes: b"reshade-host".to_vec(),
            reshade_source_url: "https://example.test/reshade.zip".to_owned(),
            reshade_source_etag: None,
            reshade_last_modified: None,
            reshade_digest: "reshade-digest".to_owned(),
            reshade_channel: Some(crate::addons::reshade::types::ReshadeChannel::Stable),
            processing_path: crate::addons::renodx::types::RenoDxProcessingPath::Upgrade,
            renodx_config: None,
            ini_tweaks: crate::addons::renodx::types::renodx_ini_defaults(),
        }
    }

    fn split_proxy_fixture() -> (TempDir, TempDir, PreparedInstall) {
        let game_dir = tempdir().expect("game root");
        let addon_dir = tempdir().expect("external AddonPath");
        let game_id = GameId::new("manual:renodx-proxy-plan").expect("game id");
        let prepared = prepared(&game_id);
        fs::write(
            game_dir.path().join("ReShade.ini"),
            format!(
                "[ADDON]\r\nAddonPath={}\r\n[renodx]\r\nSet_Path = custom\r\n",
                addon_dir.path().display()
            ),
        )
        .expect("seed split AddonPath and config baseline");
        (game_dir, addon_dir, prepared)
    }

    #[test]
    fn split_proxy_read_only_plan_matches_executor_receipts_and_config_projection() {
        let (game_dir, addon_dir, prepared) = split_proxy_fixture();
        let proxy_plan =
            prepare_proxy_install(game_dir.path(), &prepared).expect("read-only proxy plan");
        let (participants, projected_record, roots) = proxy_plan
            .into_combined_parts(&prepared)
            .expect("combined proxy projection");
        assert!(
            roots
                .iter()
                .any(|root| crate::paths::same_path(root, game_dir.path()))
        );
        assert!(
            roots
                .iter()
                .any(|root| crate::paths::same_path(root, addon_dir.path())),
            "the split AddonPath root must be included in the composed scope: {roots:?}"
        );
        let (files, _) = participants.into_parts();
        assert!(files.iter().any(|file| {
            crate::paths::same_path(
                &file.live_path,
                &addon_dir.path().join("renodx-test.addon64"),
            ) && file.before.is_none()
                && file.after.as_deref() == Some(b"addon".as_slice())
        }));
        assert!(files.iter().any(|file| {
            crate::paths::same_path(&file.live_path, &game_dir.path().join("dxgi.dll"))
                && file.before.is_none()
                && file.after.as_deref() == Some(b"reshade-host".as_slice())
        }));
        assert!(files.iter().any(|file| {
            crate::paths::same_path(&file.live_path, &game_dir.path().join("ReShade.ini"))
                && file.before.is_some()
                && file.after.is_some()
        }));
        assert!(projected_record.renodx_config_receipt().is_some());
        assert!(projected_record.created_files().iter().any(|path| {
            crate::paths::same_path(
                Path::new(path.as_str()),
                &addon_dir.path().join("renodx-test.addon64"),
            )
        }));

        let (executed_record, commit) =
            install_proxy(game_dir.path(), &prepared).expect("ordinary split proxy executor");
        commit.finish_committed();
        assert!(executed_record.eq_ignoring_persistence_timestamps(&projected_record));
        assert!(executed_record.renodx_config_receipt().is_some());
        assert_eq!(
            fs::read(addon_dir.path().join("renodx-test.addon64")).expect("external payload"),
            b"addon"
        );
        assert_eq!(
            fs::read(game_dir.path().join("dxgi.dll")).expect("game host"),
            b"reshade-host"
        );
    }
}
