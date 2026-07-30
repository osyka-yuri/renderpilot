//! Active component freshness checks and post-cascade record rewrites.

use std::path::Path;

use renderpilot_application::AppError;
use renderpilot_domain::{ComponentFile, LibraryComponent, ManagedFileMode};

use super::baseline::{BaselineConflict, verified_hash};

/// Freshly hashed active component state accepted at the mutation boundary.
#[derive(Debug, Clone)]
pub(crate) struct CurrentComponentSnapshot {
    component: LibraryComponent,
}

impl CurrentComponentSnapshot {
    pub(crate) fn into_component(self) -> LibraryComponent {
        self.component
    }
}

/// Re-reads every active component member and proves that the bytes are still
/// the catalog snapshot or the active bytes of an owned coordinated binding.
/// The returned component never carries a stale hash or version.
pub(crate) fn current_component_snapshot(
    component: &LibraryComponent,
    managed_files: &[renderpilot_domain::ManagedAddonFile],
) -> Result<CurrentComponentSnapshot, BaselineConflict> {
    let mut files = Vec::new();

    for persisted in component.files() {
        let path = Path::new(persisted.path().as_str());
        if !path.is_file() {
            return Err(BaselineConflict::MissingActiveFile(path.to_path_buf()));
        }
        let catalog = persisted
            .sha256()
            .cloned()
            .ok_or_else(|| BaselineConflict::MissingActiveHash(path.to_path_buf()))?;
        let actual = verified_hash(path)?;
        let managed = managed_files
            .iter()
            .find(|binding| {
                binding.mode() == ManagedFileMode::Owned
                    && crate::paths::same_path(Path::new(binding.path().as_str()), path)
            })
            .map(|binding| binding.installed_sha256().clone());
        if actual != catalog && managed.as_ref() != Some(&actual) {
            return Err(BaselineConflict::ActiveHashMismatch {
                path: path.to_path_buf(),
                catalog,
                managed,
                actual,
            });
        }

        let mut current = ComponentFile::new(persisted.path().clone()).with_sha256(actual);
        if let Some(install_as) = persisted.install_as() {
            current = current.with_install_as(install_as);
        }
        current = super::with_observed_metadata(current, component.technology(), path);
        files.push(current);
    }

    Ok(CurrentComponentSnapshot {
        component: component.rebuild_with_files(files),
    })
}

/// Removes owned bindings consumed by a catalog bundle rollback. Reused
/// bindings and unrelated add-on kinds are intentionally left untouched.
pub(crate) fn record_after_component_rollback(
    record: &renderpilot_domain::InstalledAddon,
    component: &LibraryComponent,
    baseline: &[ComponentFile],
) -> Result<Option<renderpilot_domain::InstalledAddon>, AppError> {
    let rolled_back_paths = component
        .files()
        .iter()
        .chain(baseline)
        .map(|file| file.path().clone())
        .collect::<Vec<_>>();
    record_after_paths_rollback(record, &rolled_back_paths)
}

/// Removes owned bindings consumed by an orphaned catalog rollback, where the
/// component row no longer exists but its immutable path provenance remains.
pub(crate) fn record_after_paths_rollback(
    record: &renderpilot_domain::InstalledAddon,
    rolled_back_paths: &[renderpilot_domain::PathRef],
) -> Result<Option<renderpilot_domain::InstalledAddon>, AppError> {
    let rolled_back_paths = rolled_back_paths
        .iter()
        .map(|path| crate::paths::normalized_key(Path::new(path.as_str())))
        .collect::<std::collections::HashSet<_>>();
    let remaining: Vec<_> = record
        .managed_files()
        .iter()
        .filter(|managed| {
            managed.mode() != ManagedFileMode::Owned
                || !rolled_back_paths.contains(&crate::paths::normalized_key(Path::new(
                    managed.path().as_str(),
                )))
        })
        .cloned()
        .collect();
    if remaining.len() == record.managed_files().len() {
        Ok(None)
    } else {
        record
            .clone()
            .try_with_managed_files(remaining)
            .map(Some)
            .map_err(AppError::from)
    }
}
