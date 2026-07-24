//! Persisted component baselines projected onto currently usable byte sources.

use std::collections::HashSet;
use std::path::Path;

use renderpilot_application::AppResult;
use renderpilot_domain::{ComponentFile, ComponentRollbackBaseline, GameId, GraphicsComponent};
use renderpilot_storage_sqlite::SqliteStorage;

/// Cheap rollback availability for one persisted component baseline.
///
/// `Available` means every recorded member has a plausible readable byte source.
/// Mutation boundaries still hash those bytes against the recorded digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ComponentBackupAvailability {
    /// No baseline row is persisted for the component.
    NotRecorded,
    /// Every recorded member has a readable sidecar or unchanged live source.
    Available(ComponentRollbackBaseline),
    /// A baseline row exists, but at least one required byte source is unavailable.
    Unavailable(ComponentRollbackBaseline),
}

impl ComponentBackupAvailability {
    /// Consumes this projection and returns the usable recorded baseline.
    #[must_use]
    pub(crate) fn into_available(self) -> Option<ComponentRollbackBaseline> {
        match self {
            Self::Available(baseline) => Some(baseline),
            Self::NotRecorded | Self::Unavailable(_) => None,
        }
    }

    /// Whether rollback can proceed to its mutation-boundary integrity checks.
    #[must_use]
    pub(crate) fn is_available(&self) -> bool {
        matches!(self, Self::Available(_))
    }
}

/// Loads and classifies the persisted baseline for one catalog component.
pub(crate) fn load_component_backup_availability(
    storage: &SqliteStorage,
    component: &GraphicsComponent,
) -> AppResult<ComponentBackupAvailability> {
    let recorded = storage.get_component_backup(component.id())?;
    Ok(classify_component_backup(recorded, component.files()))
}

/// Returns component ids whose persisted rollback sources are currently usable.
///
/// Both component rows and backup rows are loaded once per game; stale backup
/// rows for components no longer present in the catalog are intentionally omitted.
pub(crate) fn available_component_backup_ids(
    storage: &SqliteStorage,
    game_id: &GameId,
    components: &[GraphicsComponent],
) -> AppResult<HashSet<String>> {
    let mut recorded = storage.component_backups_for_game(game_id)?;
    Ok(components
        .iter()
        .filter(|component| {
            classify_component_backup(recorded.remove(component.id()), component.files())
                .is_available()
        })
        .map(|component| component.id().as_str().to_owned())
        .collect())
}

fn classify_component_backup(
    recorded: Option<ComponentRollbackBaseline>,
    current: &[ComponentFile],
) -> ComponentBackupAvailability {
    let Some(recorded) = recorded else {
        return ComponentBackupAvailability::NotRecorded;
    };
    if baseline_sources_appear_available(recorded.files(), current) {
        ComponentBackupAvailability::Available(recorded)
    } else {
        ComponentBackupAvailability::Unavailable(recorded)
    }
}

/// Mirrors recorded-baseline source selection without hashing: prefer a readable
/// classic sidecar, otherwise accept readable live bytes whose catalog identity
/// still equals the recorded baseline.
///
/// An empty baseline is available: rollback removes files created by the overlay.
#[must_use]
pub(super) fn baseline_sources_appear_available(
    recorded: &[ComponentFile],
    current: &[ComponentFile],
) -> bool {
    recorded.iter().all(|file| {
        let Some(expected) = file.sha256() else {
            return false;
        };
        let live = Path::new(file.path().as_str());
        let Ok(sidecar) = crate::fs::backup_path(live) else {
            return false;
        };
        if crate::fs::is_readable_non_empty_file(&sidecar) {
            return true;
        }

        current.iter().any(|active| {
            crate::paths::same_path(Path::new(active.path().as_str()), live)
                && active.sha256() == Some(expected)
                && crate::fs::is_readable_non_empty_file(live)
        })
    })
}
