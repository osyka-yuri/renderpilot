//! Active/absent visibility for retained raw installation registrations.

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::{GameId, GameInstallation, InstallRoot, InstalledAddonHostKind, PathRef};
use rusqlite::{OptionalExtension, Transaction, named_params};

use crate::error::{invalid_row, storage_error};

use super::{
    SqliteStorage,
    observations::{self, AuthorityCas},
};

mod external_projection;

/// Persisted visibility state for one raw game registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallationAvailabilityState {
    /// The installation is present and may appear in active catalog reads.
    Active,
    /// The raw registration remains, but the installation is absent.
    Absent,
}

impl InstallationAvailabilityState {
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "active" => Ok(Self::Active),
            "absent" => Ok(Self::Absent),
            _ => Err(invalid_row(format!(
                "invalid installation availability '{value}'"
            ))),
        }
    }
}

/// Current state and its monotonic publication revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallationAvailability {
    state: InstallationAvailabilityState,
    revision: u64,
}

impl InstallationAvailability {
    /// Returns whether the raw registration may appear in active catalog reads.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self.state, InstallationAvailabilityState::Active)
    }

    /// Returns whether the raw registration is retained as absent.
    #[must_use]
    pub const fn is_absent(self) -> bool {
        matches!(self.state, InstallationAvailabilityState::Absent)
    }

    /// Returns the stored active/absent state.
    #[must_use]
    pub const fn state(self) -> InstallationAvailabilityState {
        self.state
    }

    /// Returns the monotonic state revision used by scan publication CAS.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}

impl SqliteStorage {
    /// Reads the persisted visibility state for one raw game registration.
    pub fn installation_availability(
        &self,
        game_id: &GameId,
    ) -> AppResult<Option<InstallationAvailability>> {
        self.with_connection(|connection| availability_in_connection(connection, game_id))
    }

    /// Returns whether a retained raw registration is currently marked absent.
    pub fn is_installation_absent(&self, game_id: &GameId) -> AppResult<bool> {
        Ok(self
            .installation_availability(game_id)?
            .is_some_and(InstallationAvailability::is_absent))
    }

    /// Durably marks a registered installation absent before local metadata
    /// collection. Repeated calls after the first transition are idempotent.
    /// When an unresolved native operation is present, its scan authority is
    /// preserved exactly; the availability revision still hides the game.
    pub fn mark_installation_absent(
        &self,
        expected_game: &GameInstallation,
        authority: AuthorityCas,
    ) -> AppResult<InstallationAvailability> {
        let (availability, catalog_changed) = self.with_transaction(|transaction| {
            let stored = super::games::find_game_in_connection(transaction, expected_game.id())?
                .ok_or_else(|| {
                    AppError::invalid_input("installation registration no longer exists")
                })?;
            if &stored != expected_game {
                return Err(AppError::storage_failed(
                    "installation registration changed before absence was recorded",
                ));
            }

            let mut availability =
                availability_within_transaction(transaction, expected_game.id())?
                    .ok_or_else(|| storage_error("game has no installation availability row"))?;
            if availability.is_absent() {
                return Ok((availability, false));
            }

            let readiness =
                observations::readiness_within_transaction(transaction, expected_game.id())?;
            if readiness.authority_epoch() != authority.expected_epoch() {
                return Err(storage_error(format!(
                    "scan authority changed for {}; expected epoch {}, found {}",
                    expected_game.id().as_str(),
                    authority.expected_epoch(),
                    readiness.authority_epoch()
                )));
            }

            let has_pending =
                has_native_pending_or_engine_transition(transaction, expected_game.id())?;
            let updated = transaction.execute(
                "UPDATE installation_availability
                    SET state = 'absent', revision = revision + 1,
                        updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
                  WHERE game_id = :game_id AND state = 'active' AND revision = :revision",
                named_params! {
                    ":game_id": expected_game.id().as_str(),
                    ":revision": i64::try_from(availability.revision)
                        .map_err(|_| invalid_row("installation availability revision overflow"))?,
                },
            ).map_err(storage_error)?;
            if updated != 1 {
                return Err(storage_error(
                    "installation availability changed before absence transition",
                ));
            }
            if !has_pending {
                observations::invalidate_game_authority_within_transaction(
                    transaction,
                    expected_game.id(),
                    "installation_absent",
                    None,
                )?;
            }
            availability = availability_within_transaction(transaction, expected_game.id())?
                .ok_or_else(|| storage_error("installation availability row disappeared"))?;
            Ok((availability, true))
        })?;
        if catalog_changed {
            self.invalidate_catalog_projection();
        }
        Ok(availability)
    }

    /// Collects stale local installation state after absence was durably
    /// recorded. Native operations/reservations block the whole transaction;
    /// registrations, external ownership, and downloaded/imported artifacts
    /// remain intact.
    pub fn collect_absent_installation(
        &self,
        expected_game: &GameInstallation,
        authority: AuthorityCas,
    ) -> AppResult<()> {
        let catalog_changed = self.with_transaction(|transaction| {
            let stored = super::games::find_game_in_connection(transaction, expected_game.id())?
                .ok_or_else(|| {
                    AppError::invalid_input("installation registration no longer exists")
                })?;
            if &stored != expected_game {
                return Err(AppError::storage_failed(
                    "installation registration changed before local collection",
                ));
            }
            let availability = availability_in_transaction(transaction, expected_game.id())?;
            if !availability.is_absent() {
                return Err(AppError::invalid_input(
                    "local collection requires an absent installation",
                ));
            }
            let readiness =
                observations::readiness_within_transaction(transaction, expected_game.id())?;
            if readiness.authority_epoch() != authority.expected_epoch() {
                return Err(storage_error(format!(
                    "scan authority changed for {}; expected epoch {}, found {}",
                    expected_game.id().as_str(),
                    authority.expected_epoch(),
                    readiness.authority_epoch()
                )));
            }
            collect_absent_local_state_within_transaction(
                transaction,
                expected_game.id(),
                &InstallRoot::new(expected_game.install_path().clone()),
                true,
            )
        })?;
        if catalog_changed {
            self.invalidate_catalog_projection();
        }
        Ok(())
    }
}

fn availability_in_connection(
    connection: &rusqlite::Connection,
    game_id: &GameId,
) -> AppResult<Option<InstallationAvailability>> {
    let row = connection
        .query_row(
            "SELECT state, revision FROM installation_availability WHERE game_id = ?1",
            [game_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(storage_error)?;
    row.map(decode_availability).transpose()
}

pub(super) fn availability_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<Option<InstallationAvailability>> {
    availability_in_connection(transaction, game_id)
}

fn decode_availability((state, revision): (String, i64)) -> AppResult<InstallationAvailability> {
    Ok(InstallationAvailability {
        state: InstallationAvailabilityState::parse(&state)?,
        revision: u64::try_from(revision)
            .map_err(|_| invalid_row("negative availability revision"))?,
    })
}

fn has_native_pending_or_engine_transition(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<bool> {
    transaction
        .query_row(
            "SELECT
                EXISTS(SELECT 1 FROM pending_file_mutations WHERE game_id = :game_id)
                OR EXISTS(SELECT 1 FROM peer_aggregate_reservations WHERE game_id = :game_id)
                OR EXISTS(SELECT 1 FROM pending_shared_vulkan_mutations
                           WHERE scope = 'game_shared' AND game_id = :game_id)
                OR EXISTS(SELECT 1 FROM pending_drs_operations WHERE game_id = :game_id)
                OR EXISTS(SELECT 1 FROM game_engine_config_journals
                           WHERE game_id = :game_id
                             AND json_type(journal_json, '$.pending') = 'object')",
            named_params! { ":game_id": game_id.as_str() },
            |row| row.get(0),
        )
        .map_err(storage_error)
}

fn assert_collection_is_unblocked(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<()> {
    if has_native_pending_or_engine_transition(transaction, game_id)? {
        return Err(storage_error(format!(
            "installation {} has unresolved native or Engine.ini work; local collection is blocked",
            game_id.as_str()
        )));
    }
    Ok(())
}

pub(super) fn collect_absent_local_state_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    install_root: &InstallRoot,
    invalidate_authority: bool,
) -> AppResult<bool> {
    assert_collection_is_unblocked(transaction, game_id)?;
    let mut changed = false;

    let registered_addon = super::installed_addons::get_within_transaction(transaction, game_id)?;
    let projected_addon = registered_addon
        .as_ref()
        .filter(|addon| addon.host_kind() != Some(InstalledAddonHostKind::SharedVulkanLayer))
        .map(|addon| external_projection::project_external_addon_closure(addon, install_root))
        .transpose()?;

    if let Some(owner) = super::installed_addons::engine_config_journal_owner_within_transaction(
        transaction,
        game_id,
    )? {
        if owner.journal.is_pending() {
            return Err(storage_error(
                "pending Engine.ini ownership blocks local collection",
            ));
        }
        if let Some(receipt) = owner.journal.stable.as_ref() {
            let journal_path = PathRef::new(receipt.path.clone()).map_err(invalid_row)?;
            if install_root.contains_path(&journal_path) {
                let deleted = transaction
                    .execute(
                        "DELETE FROM game_engine_config_journals
                          WHERE game_id = ?1 AND addon_kind = ?2 AND journal_json = ?3",
                        rusqlite::params![game_id.as_str(), owner.kind.as_str(), owner.raw_token],
                    )
                    .map_err(storage_error)?;
                if deleted != 1 {
                    return Err(storage_error(
                        "Engine.ini journal changed before contained-owner collection",
                    ));
                }
                changed = true;
            }
        }
    }

    // LocalObserved is source-owned and must disappear while source_game_id
    // still identifies the registration. Downloaded and imported artifacts
    // are shared/user-owned and survive this transition.
    changed |= transaction
        .execute(
            "DELETE FROM library_artifacts
              WHERE source_game_id = ?1 AND trust_level IN ('local_observed', 'LocalObserved')",
            [game_id.as_str()],
        )
        .map_err(storage_error)?
        > 0;
    changed |= transaction
        .execute(
            "DELETE FROM file_observations WHERE owner_kind = 'game' AND game_id = ?1",
            [game_id.as_str()],
        )
        .map_err(storage_error)?
        > 0;

    // OptiScaler state references its proxy topology with ON DELETE RESTRICT.
    changed |= transaction
        .execute(
            "DELETE FROM optiscaler_install_states WHERE game_id = ?1",
            [game_id.as_str()],
        )
        .map_err(storage_error)?
        > 0;
    changed |= transaction
        .execute(
            "DELETE FROM game_proxy_topologies WHERE game_id = ?1",
            [game_id.as_str()],
        )
        .map_err(storage_error)?
        > 0;
    changed |= transaction
        .execute(
            "DELETE FROM component_backups WHERE game_id = ?1",
            [game_id.as_str()],
        )
        .map_err(storage_error)?
        > 0;
    changed |= transaction
        .execute(
            "DELETE FROM components WHERE game_id = ?1",
            [game_id.as_str()],
        )
        .map_err(storage_error)?
        > 0;
    // A committed SharedVulkanLayer record binds the game to its shared host
    // executable. Keep that external ownership receipt; for Proxy receipts,
    // retain only the external path closure after stripping root-local files.
    if let Some(addon) = registered_addon.as_ref()
        && addon.host_kind() != Some(InstalledAddonHostKind::SharedVulkanLayer)
    {
        match projected_addon.expect("non-SharedVulkan owner was projected") {
            Some(projected) if projected != *addon => {
                super::installed_addons::ensure_independent_peer_mutation_allowed(
                    transaction,
                    game_id,
                    addon.kind(),
                )?;
                super::installed_addons::upsert_within_transaction(transaction, &projected)?;
                changed = true;
            }
            Some(_) => {}
            None => {
                changed |= transaction
                    .execute(
                        "DELETE FROM installed_addons WHERE game_id = ?1",
                        [game_id.as_str()],
                    )
                    .map_err(storage_error)?
                    > 0;
            }
        }
    }
    changed |= transaction
        .execute(
            "DELETE FROM profile_addon_capabilities WHERE game_id = ?1",
            [game_id.as_str()],
        )
        .map_err(storage_error)?
        > 0;

    let readiness = observations::readiness_within_transaction(transaction, game_id)?;
    let should_invalidate =
        changed || matches!(readiness, observations::CatalogReadiness::Complete(_));
    if invalidate_authority && should_invalidate {
        observations::invalidate_game_authority_within_transaction(
            transaction,
            game_id,
            "installation_absent_collection",
            None,
        )?;
    }
    Ok(changed || (invalidate_authority && should_invalidate))
}

pub(super) fn availability_in_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<InstallationAvailability> {
    availability_within_transaction(transaction, game_id)?
        .ok_or_else(|| storage_error("game has no installation availability row"))
}

pub(super) fn set_active_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    expected_revision: u64,
) -> AppResult<InstallationAvailability> {
    let current = availability_in_transaction(transaction, game_id)?;
    if current.revision != expected_revision {
        return Err(storage_error(format!(
            "installation availability changed for {}; expected revision {}, found {}",
            game_id.as_str(),
            expected_revision,
            current.revision
        )));
    }
    if current.is_absent() {
        transaction
            .execute(
                "UPDATE installation_availability
                    SET state = 'active', revision = revision + 1,
                        updated_at = CAST(unixepoch('subsec') * 1000 AS INTEGER)
                  WHERE game_id = ?1 AND state = 'absent' AND revision = ?2",
                rusqlite::params![
                    game_id.as_str(),
                    i64::try_from(expected_revision)
                        .map_err(|_| invalid_row("installation availability revision overflow"))?
                ],
            )
            .map_err(storage_error)?;
        return availability_in_transaction(transaction, game_id);
    }
    Ok(current)
}

#[cfg(test)]
mod tests;
