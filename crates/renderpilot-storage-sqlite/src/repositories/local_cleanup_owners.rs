//! Read-only view of canonical records used to recognize local game leftovers.

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::{
    ComponentId, ComponentRollbackBaseline, GameId, GameInstallation, GameProxyTopology,
    InstalledAddon, LibraryComponent, OptiScalerInstallState,
};
use rusqlite::Transaction;

use crate::{error::storage_error, repositories::installed_addons::EngineConfigJournalOwner};

use super::{
    SqliteStorage, component_backups, components, games, installation_availability,
    installed_addons, observations, optiscaler_states, proxy_topologies,
};

/// A single-transaction copy of the existing records that describe local game
/// ownership. This value is observation data; it carries no mutation authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCleanupOwners {
    game: GameInstallation,
    availability: super::InstallationAvailability,
    authority_epoch: u64,
    components: Vec<LibraryComponent>,
    baselines: Vec<(ComponentId, ComponentRollbackBaseline)>,
    optiscaler_state: Option<OptiScalerInstallState>,
    topology: Option<GameProxyTopology>,
    addon: Option<InstalledAddon>,
    engine_journal: Option<EngineConfigJournalOwner>,
}

impl LocalCleanupOwners {
    /// The registered game record used to anchor this snapshot.
    #[must_use]
    pub fn game(&self) -> &GameInstallation {
        &self.game
    }

    /// The durable active/absent registration state.
    #[must_use]
    pub const fn availability(&self) -> super::InstallationAvailability {
        self.availability
    }

    /// The catalog authority epoch captured in the same SQLite transaction.
    #[must_use]
    pub const fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }

    /// The currently stored detected components.
    #[must_use]
    pub fn components(&self) -> &[LibraryComponent] {
        &self.components
    }

    /// Durable component rollback baselines ordered by component identifier.
    #[must_use]
    pub fn baselines(&self) -> &[(ComponentId, ComponentRollbackBaseline)] {
        &self.baselines
    }

    /// The canonical OptiScaler receipt aggregate, when present.
    #[must_use]
    pub fn optiscaler_state(&self) -> Option<&OptiScalerInstallState> {
        self.optiscaler_state.as_ref()
    }

    /// The canonical proxy topology, when present.
    #[must_use]
    pub fn topology(&self) -> Option<&GameProxyTopology> {
        self.topology.as_ref()
    }

    /// The installed add-on record, when present.
    #[must_use]
    pub fn addon(&self) -> Option<&InstalledAddon> {
        self.addon.as_ref()
    }

    /// The independent Engine.ini owner and exact persisted CAS token.
    #[must_use]
    pub fn engine_journal(&self) -> Option<&EngineConfigJournalOwner> {
        self.engine_journal.as_ref()
    }
}

impl SqliteStorage {
    /// Reads all existing local ownership rows from one SQLite transaction.
    pub fn read_local_cleanup_owners(&self, game_id: &GameId) -> AppResult<LocalCleanupOwners> {
        self.with_transaction(|transaction| read_within_transaction(transaction, game_id))
    }
}

fn read_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<LocalCleanupOwners> {
    let game = games::find_game_in_connection(transaction, game_id)?.ok_or_else(|| {
        AppError::invalid_input(format!(
            "installation registration {} no longer exists",
            game_id.as_str()
        ))
    })?;
    let availability =
        installation_availability::availability_within_transaction(transaction, game_id)?
            .ok_or_else(|| storage_error("game has no installation availability row"))?;
    let authority_epoch =
        observations::readiness_within_transaction(transaction, game_id)?.authority_epoch();
    let components = components::list_components_for_game_within_transaction(transaction, game_id)?;
    let mut baselines =
        component_backups::component_backups_for_game_within_transaction(transaction, game_id)?
            .into_iter()
            .collect::<Vec<_>>();
    baselines.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

    Ok(LocalCleanupOwners {
        game,
        availability,
        authority_epoch,
        components,
        baselines,
        optiscaler_state: optiscaler_states::get_within_aggregate_transaction(
            transaction,
            game_id,
        )?,
        topology: proxy_topologies::get_within_transaction(transaction, game_id)?,
        addon: installed_addons::get_within_transaction(transaction, game_id)?,
        engine_journal: installed_addons::engine_config_journal_owner_within_transaction(
            transaction,
            game_id,
        )?,
    })
}
