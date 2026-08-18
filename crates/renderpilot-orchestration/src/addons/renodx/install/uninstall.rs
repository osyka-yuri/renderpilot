use std::collections::HashSet;
use std::path::{Path, PathBuf};

use renderpilot_domain::{InstalledAddon, InstalledAddonHostKind, PathRef};

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
    RemoveCreated { path: PathBuf },
    RestoreBackup { live: PathBuf, backup: PathBuf },
    RewriteIni { path: PathBuf, bytes: Vec<u8> },
    RemoveIni { path: PathBuf },
}

impl PreparedRenoDxUninstall {
    /// Plans every safe per-game mutation for the current record. Persisted
    /// absent paths remain valid idempotent operations; non-regular, unreadable,
    /// symlink, and reparse paths are logged and omitted instead of blocking
    /// cleanup of other safe files or the metadata row.
    #[must_use]
    pub(crate) fn prepare(record: &InstalledAddon, game_dir_hint: Option<&Path>) -> Self {
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
                    log::warn!(
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
            Some(ini_ref) => append_ini_rewrite(&mut operations, Path::new(ini_ref.as_str())),
            None => {
                if let Some(path) = locate_untracked_ini(record, game_dir_hint) {
                    append_ini_rewrite(&mut operations, &path);
                }
            }
        }

        let log_base_path =
            crate::addons::tracking::owned_proxy_host_path(record).and_then(|host_path| {
                host_path.parent().map(|game_dir| {
                    reshade::resolve_paths(game_dir, Some(&host_path)).effective_base_path
                })
            });
        Self {
            operations,
            log_base_path,
        }
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
                log::warn!(
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
                RenoDxUninstallOperation::RewriteIni { path, bytes } => {
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

/// Compatibility harness for the existing focused install-engine tests. The
/// production path uses the prepared plan directly with a durable transaction.
#[cfg(test)]
pub(crate) fn uninstall(
    record: &InstalledAddon,
    game_dir_hint: Option<&Path>,
) -> Result<(), ServiceError> {
    let plan = PreparedRenoDxUninstall::prepare(record, game_dir_hint);
    plan.apply()?;
    plan.remove_logs_best_effort();
    Ok(())
}

impl RenoDxUninstallOperation {
    const fn kind_name(&self) -> &'static str {
        match self {
            Self::RemoveCreated { .. } => "remove-created",
            Self::RestoreBackup { .. } => "restore-backup",
            Self::RewriteIni { .. } => "rewrite-ini",
            Self::RemoveIni { .. } => "remove-ini",
        }
    }

    fn affected_paths(&self) -> Vec<PathBuf> {
        match self {
            Self::RemoveCreated { path }
            | Self::RemoveIni { path }
            | Self::RewriteIni { path, .. } => vec![path.clone()],
            Self::RestoreBackup { live, backup } => vec![live.clone(), backup.clone()],
        }
    }
}

fn host_dll_written_by_this_install(record: &InstalledAddon) -> bool {
    crate::addons::tracking::owned_proxy_host_path(record).is_some()
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
            log::warn!(
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
    log::warn!(
        "RenoDX uninstall: skipping unsafe backup restore `{}` <- `{}` ({live_observation:?}, {backup_observation:?})",
        live.display(),
        backup.display()
    );
    false
}

fn append_ini_rewrite(operations: &mut Vec<RenoDxUninstallOperation>, path: &Path) {
    match observe(path) {
        V2DiskObservation::Absent => {}
        V2DiskObservation::Regular { .. } => match std::fs::read_to_string(path) {
            Ok(existing) => {
                let stripped = ini_remove_renodx_strategy().apply(&existing);
                if stripped != existing {
                    operations.push(RenoDxUninstallOperation::RewriteIni {
                        path: path.to_path_buf(),
                        bytes: stripped.into_bytes(),
                    });
                }
            }
            Err(error) => log::warn!(
                "RenoDX uninstall: skipping unreadable ReShade.ini `{}`: {error}",
                path.display()
            ),
        },
        observation => log::warn!(
            "RenoDX uninstall: skipping unsafe ReShade.ini `{}` ({observation:?})",
            path.display()
        ),
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
    use renderpilot_domain::{AddonKind, GameId};
    use tempfile::tempdir;

    fn record(addon: &Path) -> InstalledAddon {
        InstalledAddon::new(
            GameId::new("manual:renodx-uninstall-plan").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new(addon.to_string_lossy()).expect("addon"),
        )
    }

    #[test]
    fn plan_and_apply_skip_nonregular_companion_but_remove_safe_owned_paths() {
        let root = tempdir().expect("root");
        let addon = root.path().join("renodx-game.addon64");
        let companion = root.path().join("renodx-dlssfix.addon64");
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::create_dir(&companion).expect("nonregular companion");
        let record = record(&addon)
            .with_created_file(PathRef::new(companion.to_string_lossy()).expect("companion"));

        let plan = PreparedRenoDxUninstall::prepare(&record, None);
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

        let plan = PreparedRenoDxUninstall::prepare(&record, None);
        let affected = plan.affected_paths();
        assert!(affected.contains(&addon));
        assert!(affected.contains(&host));
        assert!(affected.contains(&backup));

        plan.apply().expect("apply");
        assert_eq!(std::fs::read(&host).expect("restored"), b"original");
        assert!(!backup.exists());
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

        let plan = PreparedRenoDxUninstall::prepare(&record, None);
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

        let plan = PreparedRenoDxUninstall::prepare(&record, None);
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
