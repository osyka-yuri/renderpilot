//! Catalog path ownership claims for coordinated files.

use std::path::Path;

use renderpilot_application::{AppError, AppResult, ComponentRepository};
use renderpilot_domain::{
    GameId, InstalledAddon, ManagedAddonFile, ManagedFileBaseline, Sha256Hash,
};
use renderpilot_storage_sqlite::SqliteStorage;

use super::load_component_backup_availability;

/// Catalog facts for one coordinated path, normalized independently of any
/// concrete add-on implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogPathClaim {
    active_hashes: Vec<Sha256Hash>,
    baseline: Option<ManagedFileBaseline>,
}

impl CatalogPathClaim {
    pub(crate) fn active_hashes(&self) -> &[Sha256Hash] {
        &self.active_hashes
    }

    pub(crate) fn baseline(&self) -> Option<&ManagedFileBaseline> {
        self.baseline.as_ref()
    }
}

/// Returns the managed-file bindings of an optional installed-addon record,
/// or an empty slice when no record is present.
pub(crate) fn managed_files_of(record: Option<&InstalledAddon>) -> &[ManagedAddonFile] {
    record.map_or(&[][..], InstalledAddon::managed_files)
}

/// Collects active and immutable-baseline claims for one path. Conflicting
/// catalog rows are rejected instead of being resolved by row order.
pub(crate) fn catalog_path_claim(
    storage: &SqliteStorage,
    game_id: &GameId,
    path: &Path,
) -> AppResult<CatalogPathClaim> {
    let key = crate::paths::normalized_key(path);
    let mut active_hashes = Vec::new();
    let mut baselines = Vec::new();

    for component in storage.list_components_for_game(game_id)? {
        let active_match = component
            .files()
            .iter()
            .find(|file| crate::paths::normalized_key(Path::new(file.path().as_str())) == key);
        let Some(active) = active_match else {
            continue;
        };
        let active_hash = active.sha256().cloned().ok_or_else(|| {
            AppError::invalid_input(format!(
                "active catalog claim has no hash for {}",
                path.display()
            ))
        })?;
        if !active_hashes.contains(&active_hash) {
            active_hashes.push(active_hash);
        }

        if let Some(baseline) =
            load_component_backup_availability(storage, &component)?.into_available()
        {
            let claim = baseline
                .files()
                .iter()
                .find(|file| crate::paths::normalized_key(Path::new(file.path().as_str())) == key)
                .map(|file| {
                    file.sha256()
                        .cloned()
                        .map(|sha256| ManagedFileBaseline::Present { sha256 })
                        .ok_or_else(|| {
                            AppError::invalid_input(format!(
                                "catalog baseline has no hash for {}",
                                path.display()
                            ))
                        })
                })
                .transpose()?
                .unwrap_or(ManagedFileBaseline::Absent);
            if !baselines.contains(&claim) {
                baselines.push(claim);
            }
        }
    }

    if active_hashes.len() > 1 {
        return Err(AppError::invalid_input(format!(
            "conflicting active catalog claims for {}",
            path.display()
        )));
    }
    if baselines.len() > 1 {
        return Err(AppError::invalid_input(format!(
            "conflicting catalog baseline claims for {}",
            path.display()
        )));
    }
    Ok(CatalogPathClaim {
        active_hashes,
        baseline: baselines.pop(),
    })
}
