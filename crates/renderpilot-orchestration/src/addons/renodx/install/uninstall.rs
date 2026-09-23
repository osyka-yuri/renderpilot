use std::collections::HashSet;
use std::path::{Path, PathBuf};

use renderpilot_domain::{
    InstalledAddon, InstalledAddonHostKind, ManagedFileBaseline, ManagedFileMode,
    NormalizedPathRelation, PathRef, Sha256Hash, normalized_path_relation,
};
use sha2::{Digest, Sha256};

use crate::ServiceError;
use crate::addons::reshade::scan as reshade;
use crate::file_mutation::{V2DiskObservation, observe};

use super::super::reshade_ini::ini_remove_renodx_strategy;

/// A no-follow-validated full RenoDX uninstall plan.
///
/// This is deliberately independent of [`InstalledAddon`] after preparation:
/// both the durable mutation paths and the apply step come from the same
/// operations, so a skipped unsafe path cannot reappear through a later generic
/// record scan.
#[derive(Debug)]
pub(crate) struct PreparedRenoDxUninstall {
    operations: Vec<RenoDxUninstallOperation>,
    log_base_path: Option<PathBuf>,
}

#[derive(Debug)]
enum RenoDxUninstallOperation {
    RemoveCreated {
        path: PathBuf,
    },
    RestoreBackup {
        live: PathBuf,
        backup: PathBuf,
    },
    RemoveManagedOwned {
        path: PathBuf,
        installed_sha256: Sha256Hash,
    },
    RestoreManagedOwned {
        live: PathBuf,
        backup: PathBuf,
        installed_sha256: Sha256Hash,
        baseline_sha256: Sha256Hash,
    },
    RewriteIni {
        path: PathBuf,
        expected_before: Vec<u8>,
        bytes: Vec<u8>,
    },
    RemoveIni {
        path: PathBuf,
    },
}

impl PreparedRenoDxUninstall {
    /// Plans every safe per-game mutation for the current record. Persisted
    /// absent paths remain valid idempotent operations; non-regular, unreadable,
    /// symlink, and reparse paths are logged and omitted instead of blocking
    /// cleanup of other safe files or the metadata row.
    pub(crate) fn prepare(
        record: &InstalledAddon,
        game_dir_hint: Option<&Path>,
    ) -> Result<Self, ServiceError> {
        let mut operations = Vec::new();
        let backed_up: HashSet<String> = record
            .backed_up_files()
            .iter()
            .map(|path| crate::paths::normalized_key(Path::new(path.as_str())))
            .collect();
        let ini_in_created = ini_path_in(record.created_files());
        let ini_in_backed_up = ini_path_in(record.backed_up_files());

        for path in record.created_files() {
            if is_ini_path(path)
                || backed_up.contains(&crate::paths::normalized_key(Path::new(path.as_str())))
            {
                continue;
            }
            let path = PathBuf::from(path.as_str());
            if permits_remove(&path, "created RenoDX file") {
                operations.push(RenoDxUninstallOperation::RemoveCreated { path });
            }
        }

        for path in record.backed_up_files() {
            // ReShade.ini has a dedicated key-removal policy below. Restoring
            // its legacy sidecar would both consume user recovery data and
            // replace current unrelated settings with stale bytes.
            if is_ini_path(path) {
                continue;
            }
            let live = PathBuf::from(path.as_str());
            let backup = match crate::fs::backup_path(&live) {
                Ok(backup) => backup,
                Err(error) => {
                    let diagnostic_path = live.to_string_lossy();
                    tracing::warn!(
                        diagnostic_path = diagnostic_path.as_ref(),
                        "RenoDX uninstall: cannot derive backup path for `{}`: {error}",
                        live.display()
                    );
                    continue;
                }
            };
            if permits_restore(&live, &backup) {
                operations.push(RenoDxUninstallOperation::RestoreBackup { live, backup });
            }
        }

        // An OptiScaler outer can temporarily relocate RenoDX's proxy host
        // through the coordinated `managed_files` projection. Once that outer
        // has been removed, inactive uninstall is again responsible for the
        // host.  Generic `created_files` intentionally cannot stand in for
        // this claim: it has a distinct ownership/baseline contract.
        for managed in record.managed_files() {
            if managed.mode() == ManagedFileMode::Reused {
                continue;
            }
            let live = PathBuf::from(managed.path().as_str());
            if !live
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(reshade::is_proxy_slot)
            {
                return Err(crate::failed(format!(
                    "RenoDX inactive uninstall cannot release a non-proxy managed file: {}",
                    live.display()
                )));
            }
            let Some(_) = read_exact_managed_file(&live, managed.installed_sha256())? else {
                // An already absent Owned/Absent host needs no filesystem
                // action.  It remains safe to retire its metadata record.
                if matches!(managed.baseline(), ManagedFileBaseline::Absent) {
                    continue;
                }
                return Err(crate::failed(format!(
                    "RenoDX managed host disappeared before its baseline could be restored: {}",
                    live.display()
                )));
            };
            match managed.baseline() {
                ManagedFileBaseline::Absent => {
                    operations.push(RenoDxUninstallOperation::RemoveManagedOwned {
                        path: live,
                        installed_sha256: managed.installed_sha256().clone(),
                    });
                }
                ManagedFileBaseline::Present { sha256 } => {
                    let backup = crate::fs::backup_path(&live).map_err(|error| {
                        crate::failed(format!(
                            "RenoDX managed host has an invalid baseline sidecar path {}: {error}",
                            live.display()
                        ))
                    })?;
                    read_exact_managed_file(&backup, sha256)?.ok_or_else(|| {
                        crate::failed(format!(
                            "RenoDX managed host baseline sidecar is missing: {}",
                            backup.display()
                        ))
                    })?;
                    operations.push(RenoDxUninstallOperation::RestoreManagedOwned {
                        live,
                        backup,
                        installed_sha256: managed.installed_sha256().clone(),
                        baseline_sha256: sha256.clone(),
                    });
                }
            }
        }

        let owns_whole_stack = matches!(
            record.host_kind(),
            Some(InstalledAddonHostKind::SharedVulkanLayer)
        ) || host_dll_written_by_this_install(record);
        match ini_in_created.or(ini_in_backed_up) {
            Some(ini_ref) if ini_in_backed_up.is_none() && owns_whole_stack => {
                let path = PathBuf::from(ini_ref.as_str());
                if permits_remove(&path, "owned ReShade.ini") {
                    operations.push(RenoDxUninstallOperation::RemoveIni { path });
                }
            }
            Some(ini_ref) => append_ini_rewrite(
                &mut operations,
                Path::new(ini_ref.as_str()),
                record.renodx_config_receipt(),
            ),
            None => {
                if let Some(path) = locate_untracked_ini(record, game_dir_hint) {
                    append_ini_rewrite(&mut operations, &path, record.renodx_config_receipt());
                }
            }
        }

        let log_base_path =
            crate::addons::tracking::owned_proxy_host_path(record).and_then(|host_path| {
                host_path.parent().map(|game_dir| {
                    reshade::resolve_paths(game_dir, Some(&host_path)).effective_base_path
                })
            });
        Ok(Self {
            operations,
            log_base_path,
        })
    }

    /// Exact paths this plan may mutate. Durable target selection must derive
    /// only from these operations, never from the original install record.
    #[must_use]
    pub(crate) fn affected_paths(&self) -> Vec<PathBuf> {
        self.operations
            .iter()
            .flat_map(RenoDxUninstallOperation::affected_paths)
            .collect()
    }

    /// Translates the prepared uninstall operations into exact SVAM file
    /// participants without performing any writes.  The operation order is
    /// preserved, including the backup sidecar consumed by a restore.
    pub(crate) fn take_file_intents(
        &mut self,
    ) -> Result<Vec<crate::addons::shared_vulkan_mutation::FileIntent>, ServiceError> {
        let mut intents = Vec::new();
        for operation in std::mem::take(&mut self.operations) {
            match operation {
                RenoDxUninstallOperation::RemoveCreated { path }
                | RenoDxUninstallOperation::RemoveIni { path } => {
                    intents.push(crate::addons::shared_vulkan_mutation::FileIntent {
                        before: read_regular_file(&path)?,
                        live_path: path,
                        after: None,
                    });
                }
                RenoDxUninstallOperation::RestoreBackup { live, backup } => {
                    let before = read_regular_file(&live)?;
                    let restored = read_regular_file(&backup)?.ok_or_else(|| {
                        crate::failed(format!(
                            "RenoDX uninstall backup disappeared before composition: {}",
                            backup.display()
                        ))
                    })?;
                    intents.push(crate::addons::shared_vulkan_mutation::FileIntent {
                        live_path: live,
                        before,
                        after: Some(restored.clone()),
                    });
                    intents.push(crate::addons::shared_vulkan_mutation::FileIntent {
                        live_path: backup,
                        before: Some(restored),
                        after: None,
                    });
                }
                RenoDxUninstallOperation::RemoveManagedOwned {
                    path,
                    installed_sha256,
                } => {
                    let before =
                        read_exact_managed_file(&path, &installed_sha256)?.ok_or_else(|| {
                            crate::failed(format!(
                                "RenoDX managed host disappeared before composition: {}",
                                path.display()
                            ))
                        })?;
                    intents.push(crate::addons::shared_vulkan_mutation::FileIntent {
                        before: Some(before),
                        live_path: path,
                        after: None,
                    });
                }
                RenoDxUninstallOperation::RestoreManagedOwned {
                    live,
                    backup,
                    installed_sha256,
                    baseline_sha256,
                } => {
                    let before =
                        read_exact_managed_file(&live, &installed_sha256)?.ok_or_else(|| {
                            crate::failed(format!(
                                "RenoDX managed host disappeared before composition: {}",
                                live.display()
                            ))
                        })?;
                    let restored = read_exact_managed_file(&backup, &baseline_sha256)?.ok_or_else(|| {
                        crate::failed(format!(
                            "RenoDX managed host baseline sidecar disappeared before composition: {}",
                            backup.display()
                        ))
                    })?;
                    intents.push(crate::addons::shared_vulkan_mutation::FileIntent {
                        live_path: live,
                        before: Some(before),
                        after: Some(restored.clone()),
                    });
                    intents.push(crate::addons::shared_vulkan_mutation::FileIntent {
                        live_path: backup,
                        before: Some(restored),
                        after: None,
                    });
                }
                RenoDxUninstallOperation::RewriteIni {
                    path,
                    expected_before,
                    bytes,
                } => {
                    intents.push(crate::addons::shared_vulkan_mutation::FileIntent {
                        before: Some(expected_before),
                        live_path: path,
                        after: Some(bytes),
                    });
                }
            }
        }
        Ok(intents)
    }

    /// Removes operations whose exact path set is not under a reachable durable
    /// scope. This keeps an offline/deleted install from blocking metadata
    /// cleanup and ensures apply cannot touch a path the transaction did not
    /// snapshot.
    pub(crate) fn retain_reachable(&mut self, scope: Option<&crate::file_mutation::MutationScope>) {
        self.operations.retain(|operation| {
            let reachable = scope.is_some_and(|scope| {
                operation
                    .affected_paths()
                    .iter()
                    .all(|path| scope.contains_reachable(path))
            });
            if !reachable {
                tracing::warn!(
                    "RenoDX uninstall: skipping unreachable planned {} operation",
                    operation.kind_name()
                );
            }
            reachable
        });
    }

    /// Applies the prepared operations without accepting an install record or
    /// performing any new discovery.
    pub(crate) fn apply(&self) -> Result<(), ServiceError> {
        let mut touched_dirs = HashSet::new();
        for operation in &self.operations {
            match operation {
                RenoDxUninstallOperation::RemoveCreated { path }
                | RenoDxUninstallOperation::RemoveIni { path } => {
                    crate::fs::remove_file_if_exists(path)?;
                    insert_parent(&mut touched_dirs, path);
                }
                RenoDxUninstallOperation::RestoreBackup { live, backup } => {
                    crate::fs::remove_file_if_exists(live)?;
                    std::fs::rename(backup, live).map_err(|error| {
                        crate::failed(format!(
                            "failed to restore RenoDX backup `{}` to `{}`: {error}",
                            backup.display(),
                            live.display()
                        ))
                    })?;
                    insert_parent(&mut touched_dirs, live);
                }
                RenoDxUninstallOperation::RemoveManagedOwned {
                    path,
                    installed_sha256,
                } => {
                    read_exact_managed_file(path, installed_sha256)?.ok_or_else(|| {
                        crate::failed(format!(
                            "RenoDX managed host disappeared before removal: {}",
                            path.display()
                        ))
                    })?;
                    crate::fs::remove_file_if_exists(path)?;
                    insert_parent(&mut touched_dirs, path);
                }
                RenoDxUninstallOperation::RestoreManagedOwned {
                    live,
                    backup,
                    installed_sha256,
                    baseline_sha256,
                } => {
                    read_exact_managed_file(live, installed_sha256)?.ok_or_else(|| {
                        crate::failed(format!(
                            "RenoDX managed host disappeared before baseline restoration: {}",
                            live.display()
                        ))
                    })?;
                    read_exact_managed_file(backup, baseline_sha256)?.ok_or_else(|| {
                        crate::failed(format!(
                            "RenoDX managed host baseline sidecar disappeared before restoration: {}",
                            backup.display()
                        ))
                    })?;
                    crate::fs::remove_file_if_exists(live)?;
                    std::fs::rename(backup, live).map_err(|error| {
                        crate::failed(format!(
                            "failed to restore RenoDX managed host baseline `{}` to `{}`: {error}",
                            backup.display(),
                            live.display()
                        ))
                    })?;
                    insert_parent(&mut touched_dirs, live);
                }
                RenoDxUninstallOperation::RewriteIni {
                    path,
                    expected_before,
                    bytes,
                } => {
                    let current = read_regular_file(path)?.ok_or_else(|| {
                        crate::failed(format!(
                            "RenoDX ReShade.ini disappeared before rewrite: {}",
                            path.display()
                        ))
                    })?;
                    if current != *expected_before {
                        return Err(crate::failed(format!(
                            "RenoDX ReShade.ini changed after uninstall preparation: {}",
                            path.display()
                        )));
                    }
                    crate::fs::write_file_atomically(path, bytes)?;
                    insert_parent(&mut touched_dirs, path);
                }
            }
        }
        for directory in touched_dirs {
            crate::fs::sync_directory_best_effort(&directory);
        }
        Ok(())
    }

    /// ReShade logs are advisory diagnostics, so they are cleaned only after
    /// the durable game-file mutation and metadata delete have committed.
    pub(crate) fn remove_logs_best_effort(&self) {
        if let Some(base_path) = &self.log_base_path {
            reshade::remove_reshade_logs_best_effort(base_path);
        }
    }
}

fn read_regular_file(path: &Path) -> Result<Option<Vec<u8>>, ServiceError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(crate::failed(error.to_string())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(crate::failed(format!(
            "RenoDX uninstall participant is not a regular file: {}",
            path.display()
        )));
    }
    std::fs::read(path)
        .map(Some)
        .map_err(|error| crate::failed(error.to_string()))
}

/// Reads one coordinated host only when its no-follow regular-file observation
/// still matches the persisted SHA-256 claim.  `managed_files` lack an identity
/// token, so digest equality is the full ownership proof available to the
/// inactive legacy route; a mismatch must never be downgraded to generic
/// deletion authority.
fn read_exact_managed_file(
    path: &Path,
    expected_sha256: &Sha256Hash,
) -> Result<Option<Vec<u8>>, ServiceError> {
    let Some(bytes) = read_regular_file(path)? else {
        return Ok(None);
    };
    let actual = Sha256Hash::new(hex::encode(Sha256::digest(&bytes))).map_err(|error| {
        crate::failed(format!(
            "RenoDX managed host produced an invalid SHA-256 at {}: {error}",
            path.display()
        ))
    })?;
    if &actual != expected_sha256 {
        return Err(crate::failed(format!(
            "RenoDX managed host changed from its persisted receipt: {}",
            path.display()
        )));
    }
    Ok(Some(bytes))
}

/// Compatibility harness for the existing focused install-engine tests. The
/// production path uses the prepared plan directly with a durable transaction.
#[cfg(test)]
pub(crate) fn uninstall(
    record: &InstalledAddon,
    game_dir_hint: Option<&Path>,
) -> Result<(), ServiceError> {
    let plan = PreparedRenoDxUninstall::prepare(record, game_dir_hint)?;
    plan.apply()?;
    plan.remove_logs_best_effort();
    Ok(())
}

impl RenoDxUninstallOperation {
    const fn kind_name(&self) -> &'static str {
        match self {
            Self::RemoveCreated { .. } => "remove-created",
            Self::RestoreBackup { .. } => "restore-backup",
            Self::RemoveManagedOwned { .. } => "remove-managed-owned",
            Self::RestoreManagedOwned { .. } => "restore-managed-owned",
            Self::RewriteIni { .. } => "rewrite-ini",
            Self::RemoveIni { .. } => "remove-ini",
        }
    }

    fn affected_paths(&self) -> Vec<PathBuf> {
        match self {
            Self::RemoveCreated { path }
            | Self::RemoveIni { path }
            | Self::RewriteIni { path, .. }
            | Self::RemoveManagedOwned { path, .. } => vec![path.clone()],
            Self::RestoreBackup { live, backup }
            | Self::RestoreManagedOwned { live, backup, .. } => vec![live.clone(), backup.clone()],
        }
    }
}

fn host_dll_written_by_this_install(record: &InstalledAddon) -> bool {
    crate::addons::tracking::owned_proxy_host_path(record).is_some()
        || record.managed_files().iter().any(|file| {
            file.mode() == ManagedFileMode::Owned
                && matches!(file.baseline(), ManagedFileBaseline::Absent)
                && Path::new(file.path().as_str())
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(reshade::is_proxy_slot)
        })
}

fn ini_path_in(paths: &[PathRef]) -> Option<&PathRef> {
    paths.iter().find(|path| is_ini_path(path))
}

fn is_ini_path(path: &PathRef) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(reshade::RESHADE_INI_FILE_NAME))
}

fn permits_remove(path: &Path, label: &str) -> bool {
    match observe(path) {
        V2DiskObservation::Absent | V2DiskObservation::Regular { .. } => true,
        observation => {
            let diagnostic_path = path.to_string_lossy();
            tracing::warn!(
                diagnostic_path = diagnostic_path.as_ref(),
                "RenoDX uninstall: skipping unsafe {label} `{}` ({observation:?})",
                path.display()
            );
            false
        }
    }
}

fn permits_restore(live: &Path, backup: &Path) -> bool {
    let live_observation = observe(live);
    let backup_observation = observe(backup);
    let live_safe = matches!(
        live_observation,
        V2DiskObservation::Absent | V2DiskObservation::Regular { .. }
    );
    let backup_safe = matches!(backup_observation, V2DiskObservation::Regular { .. });
    if live_safe && backup_safe {
        return true;
    }
    let diagnostic_path = live.to_string_lossy();
    tracing::warn!(
        diagnostic_path = diagnostic_path.as_ref(),
        "RenoDX uninstall: skipping unsafe backup restore `{}` <- `{}` ({live_observation:?}, {backup_observation:?})",
        live.display(),
        backup.display()
    );
    false
}

fn append_ini_rewrite(
    operations: &mut Vec<RenoDxUninstallOperation>,
    path: &Path,
    receipt: Option<&renderpilot_domain::RenoDxConfigReceipt>,
) {
    match observe(path) {
        V2DiskObservation::Absent => {}
        V2DiskObservation::Regular { .. } => match std::fs::read(path) {
            Ok(existing_bytes) => {
                let owned_config = receipt.filter(|receipt| {
                    receipt.is_supported()
                        && path.to_str().is_some_and(|path| {
                            matches!(
                                normalized_path_relation(receipt.ini_path.as_str(), path),
                                NormalizedPathRelation::Equal
                            )
                        })
                });
                let planned_after = owned_config.and_then(|receipt| {
                    match super::super::reshade_ini::plan_config_removal(
                        &receipt.ini_path,
                        Some(&existing_bytes),
                        receipt,
                    ) {
                        Ok(plan) => plan.after,
                        Err(error) => {
                            let diagnostic_path = path.to_string_lossy();
                            tracing::warn!(
                                diagnostic_path = diagnostic_path.as_ref(),
                                "RenoDX uninstall: cannot plan RenoDX config removal for `{}`: {error}",
                                path.display()
                            );
                            None
                        }
                    }
                });
                let set_path_bytes: &[u8] = planned_after.as_deref().unwrap_or(&existing_bytes);
                // A malformed/non-UTF8 file is never rewritten through the
                // text cleanup path. In particular, a receipt CAS failure
                // must preserve the user's current RenoDX configuration bytes exactly.
                let Ok(existing) = std::str::from_utf8(set_path_bytes) else {
                    let diagnostic_path = path.to_string_lossy();
                    tracing::warn!(
                        diagnostic_path = diagnostic_path.as_ref(),
                        "RenoDX uninstall: skipping non-UTF8 ReShade.ini `{}`",
                        path.display()
                    );
                    return;
                };
                let stripped = ini_remove_renodx_strategy().apply(existing);
                if stripped.as_bytes() != existing_bytes.as_slice() {
                    operations.push(RenoDxUninstallOperation::RewriteIni {
                        path: path.to_path_buf(),
                        expected_before: existing_bytes,
                        bytes: stripped.into_bytes(),
                    });
                }
            }
            Err(error) => {
                let diagnostic_path = path.to_string_lossy();
                tracing::warn!(
                    diagnostic_path = diagnostic_path.as_ref(),
                    "RenoDX uninstall: skipping unreadable ReShade.ini `{}`: {error}",
                    path.display()
                )
            }
        },
        observation => {
            let diagnostic_path = path.to_string_lossy();
            tracing::warn!(
                diagnostic_path = diagnostic_path.as_ref(),
                "RenoDX uninstall: skipping unsafe ReShade.ini `{}` ({observation:?})",
                path.display()
            )
        }
    }
}

fn locate_untracked_ini(record: &InstalledAddon, game_dir_hint: Option<&Path>) -> Option<PathBuf> {
    let host_dir = crate::addons::tracking::host_proxy_path(record)
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let addon_dir = Path::new(record.addon_file().as_str())
        .parent()
        .map(Path::to_path_buf);

    host_dir
        .into_iter()
        .chain(game_dir_hint.map(Path::to_path_buf))
        .chain(addon_dir)
        .find_map(|dir| reshade::reshade_ini_path(&dir))
}

fn insert_parent(target: &mut HashSet<PathBuf>, path: &Path) {
    if let Some(parent) = path.parent() {
        target.insert(parent.to_path_buf());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::{
        AddonKind, GameId, ManagedAddonFile, ManagedFileBaseline, RenoDxConfigReceipt,
        RenoDxSetPathBaseline, RenoDxSetPathValue, Sha256Hash,
    };
    use tempfile::tempdir;

    fn record(addon: &Path) -> InstalledAddon {
        InstalledAddon::new(
            GameId::new("manual:renodx-uninstall-plan").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new(addon.to_string_lossy()).expect("addon"),
        )
    }

    fn hash(bytes: &[u8]) -> Sha256Hash {
        Sha256Hash::new(hex::encode(Sha256::digest(bytes))).expect("digest")
    }

    #[test]
    fn plan_removes_owned_managed_proxy_host_after_outer_release() {
        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let host = root.path().join("dxgi.dll");
        let ini = root.path().join(reshade::RESHADE_INI_FILE_NAME);
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::write(&host, b"managed ReShade host").expect("host");
        std::fs::write(&ini, b"[ADDON]\r\nDisabledAddons=Generic Depth\r\n").expect("ini");
        let record = record(&addon)
            .with_created_file(PathRef::new(ini.to_string_lossy()).expect("ini path"))
            .try_with_managed_files(vec![ManagedAddonFile::owned(
                PathRef::new(host.to_string_lossy()).expect("host path"),
                ManagedFileBaseline::Absent,
                hash(b"managed ReShade host"),
            )])
            .expect("record");

        let plan = PreparedRenoDxUninstall::prepare(&record, None).expect("plan");
        assert!(plan.affected_paths().contains(&host));

        plan.apply().expect("apply");
        assert!(!addon.exists());
        assert!(!host.exists());
        assert!(
            !ini.exists(),
            "the ReShade.ini created with an owned managed host must be removed"
        );
    }

    #[test]
    fn plan_restores_owned_managed_proxy_host_baseline() {
        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let host = root.path().join("dxgi.dll");
        let backup = crate::fs::backup_path(&host).expect("backup path");
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::write(&host, b"managed ReShade host").expect("host");
        std::fs::write(&backup, b"original ReShade host").expect("backup");
        let record = record(&addon)
            .try_with_managed_files(vec![ManagedAddonFile::owned(
                PathRef::new(host.to_string_lossy()).expect("host path"),
                ManagedFileBaseline::Present {
                    sha256: hash(b"original ReShade host"),
                },
                hash(b"managed ReShade host"),
            )])
            .expect("record");

        let plan = PreparedRenoDxUninstall::prepare(&record, None).expect("plan");
        assert!(plan.affected_paths().contains(&host));
        assert!(plan.affected_paths().contains(&backup));

        plan.apply().expect("apply");
        assert_eq!(
            std::fs::read(&host).expect("restored host"),
            b"original ReShade host"
        );
        assert!(!backup.exists());
    }

    #[test]
    fn managed_host_drift_cannot_be_deleted_after_planning() {
        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let host = root.path().join("dxgi.dll");
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::write(&host, b"managed ReShade host").expect("host");
        let record = record(&addon)
            .try_with_managed_files(vec![ManagedAddonFile::owned(
                PathRef::new(host.to_string_lossy()).expect("host path"),
                ManagedFileBaseline::Absent,
                hash(b"managed ReShade host"),
            )])
            .expect("record");
        let plan = PreparedRenoDxUninstall::prepare(&record, None).expect("plan");
        std::fs::write(&host, b"foreign replacement").expect("drift host");

        assert!(plan.apply().is_err());
        assert_eq!(
            std::fs::read(&host).expect("host remains"),
            b"foreign replacement"
        );
    }

    #[test]
    fn plan_and_apply_skip_nonregular_companion_but_remove_safe_managed_paths() {
        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let companion = root.path().join("renodx-dlssfix.addon64");
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::create_dir(&companion).expect("nonregular companion");
        let record = record(&addon)
            .with_created_file(PathRef::new(companion.to_string_lossy()).expect("companion"));

        let plan = PreparedRenoDxUninstall::prepare(&record, None).expect("plan");
        assert_eq!(plan.affected_paths(), vec![addon.clone()]);

        plan.apply().expect("apply safe operations");
        assert!(!addon.exists());
        assert!(companion.is_dir());
    }

    #[test]
    fn plan_keeps_backup_restore_and_its_exact_durable_paths_together() {
        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let host = root.path().join("dxgi.dll");
        let backup = crate::fs::backup_path(&host).expect("backup path");
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::write(&host, b"new").expect("host");
        std::fs::write(&backup, b"original").expect("backup");
        let record = record(&addon)
            .with_created_file(PathRef::new(host.to_string_lossy()).expect("host"))
            .with_backed_up_file(PathRef::new(host.to_string_lossy()).expect("backup host"));

        let plan = PreparedRenoDxUninstall::prepare(&record, None).expect("plan");
        let affected = plan.affected_paths();
        assert!(affected.contains(&addon));
        assert!(affected.contains(&host));
        assert!(affected.contains(&backup));

        plan.apply().expect("apply");
        assert_eq!(std::fs::read(&host).expect("restored"), b"original");
        assert!(!backup.exists());
    }

    #[test]
    fn prepared_ini_rewrite_rejects_drift_without_overwriting_the_user() {
        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let ini = root.path().join(reshade::RESHADE_INI_FILE_NAME);
        let original = b"[renodx]\nSet_Path=1\n";
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::write(&ini, original).expect("ini");
        let receipt = RenoDxConfigReceipt::new(
            PathRef::new(ini.to_string_lossy()).expect("ini path"),
            RenoDxSetPathBaseline::Absent,
            true,
            RenoDxSetPathValue::One,
        );
        let record = record(&addon)
            .with_created_file(PathRef::new(ini.to_string_lossy()).expect("ini"))
            .with_renodx_config_receipt(Some(receipt))
            .expect("receipt");
        let plan = PreparedRenoDxUninstall::prepare(&record, None).expect("plan");
        std::fs::write(&ini, b"[renodx]\nSet_Path=user-value\n").expect("user edit");

        assert!(plan.apply().is_err());
        assert_eq!(
            std::fs::read(&ini).expect("ini remains"),
            b"[renodx]\nSet_Path=user-value\n"
        );
    }

    #[test]
    fn shared_ini_intent_keeps_prepared_before_image_after_drift() {
        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let ini = root.path().join(reshade::RESHADE_INI_FILE_NAME);
        let original = b"[renodx]\nSet_Path=1\n";
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::write(&ini, original).expect("ini");
        let receipt = RenoDxConfigReceipt::new(
            PathRef::new(ini.to_string_lossy()).expect("ini path"),
            RenoDxSetPathBaseline::Absent,
            true,
            RenoDxSetPathValue::One,
        );
        let record = record(&addon)
            .with_created_file(PathRef::new(ini.to_string_lossy()).expect("ini"))
            .with_renodx_config_receipt(Some(receipt))
            .expect("receipt");
        let mut plan = PreparedRenoDxUninstall::prepare(&record, None).expect("plan");
        std::fs::write(&ini, b"[renodx]\nSet_Path=user-value\n").expect("user edit");
        let intents = plan.take_file_intents().expect("intents");
        let ini_intent = intents
            .into_iter()
            .find(|intent| intent.live_path == ini)
            .expect("ini intent");
        assert_eq!(ini_intent.before, Some(original.to_vec()));
        assert_ne!(
            ini_intent.after,
            Some(b"[renodx]\nSet_Path=user-value\n".to_vec())
        );
    }

    #[test]
    fn legacy_reshade_ini_backup_is_never_restored_or_consumed() {
        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let ini = root.path().join(reshade::RESHADE_INI_FILE_NAME);
        let backup = crate::fs::backup_path(&ini).expect("backup path");
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::write(&ini, b"[GENERAL]\r\nUserSetting=keep\r\n").expect("current ini");
        std::fs::write(&backup, b"[GENERAL]\r\nUserSetting=stale\r\n").expect("legacy backup");
        let record =
            record(&addon).with_backed_up_file(PathRef::new(ini.to_string_lossy()).expect("ini"));

        let plan = PreparedRenoDxUninstall::prepare(&record, None).expect("plan");
        let affected = plan.affected_paths();
        assert!(!affected.contains(&ini));
        assert!(!affected.contains(&backup));

        plan.apply().expect("apply");
        assert_eq!(
            std::fs::read(&ini).expect("current ini"),
            b"[GENERAL]\r\nUserSetting=keep\r\n"
        );
        assert_eq!(
            std::fs::read(&backup).expect("legacy backup"),
            b"[GENERAL]\r\nUserSetting=stale\r\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn plan_skips_an_unreadable_companion_without_blocking_safe_cleanup() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let companion = root.path().join("renodx-dlssfix.addon64");
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::write(&companion, b"companion").expect("companion");
        let mut permissions = std::fs::metadata(&companion)
            .expect("metadata")
            .permissions();
        permissions.set_mode(0o000);
        std::fs::set_permissions(&companion, permissions).expect("make unreadable");
        let record = record(&addon)
            .with_created_file(PathRef::new(companion.to_string_lossy()).expect("companion"));

        let plan = PreparedRenoDxUninstall::prepare(&record, None).expect("plan");
        assert_eq!(plan.affected_paths(), vec![addon.clone()]);
        plan.apply().expect("apply safe operations");
        assert!(!addon.exists());

        let mut permissions = std::fs::metadata(&companion)
            .expect("metadata")
            .permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(&companion, permissions).expect("restore permissions");
        assert!(companion.exists());
    }
}
