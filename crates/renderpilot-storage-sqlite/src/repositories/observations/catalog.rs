//! Catalog readiness, game-owned observations, and invalidation transitions.

use super::*;

impl SqliteStorage {
    /// Reads typed readiness. Missing rows are storage corruption: the games
    /// trigger and v16 migration make authority total for every game.
    pub fn catalog_readiness(&self, game_id: &GameId) -> AppResult<CatalogReadiness> {
        self.with_connection(|connection| readiness_in_connection(connection, game_id))
    }

    /// Replaces one complete game's observation set without changing catalog
    /// generation.
    ///
    /// A ready scan may refresh only its own facts after the complete catalog
    /// projection has been proven unchanged. The epoch check makes a concurrent
    /// component or durable-file invalidation fail closed instead of allowing a
    /// post-scan cache write to outlive that invalidation.
    pub fn replace_complete_game_observations(
        &self,
        game_id: &GameId,
        observations: &[StoredFileObservation],
        authority: AuthorityCas,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            let current = readiness_within_transaction(transaction, game_id)?;
            if current.authority_epoch() != authority.expected_epoch() {
                return Err(storage_error(format!(
                    "scan authority changed for {}; expected epoch {}, found {}",
                    game_id.as_str(),
                    authority.expected_epoch(),
                    current.authority_epoch()
                )));
            }
            if current.ready_projection().is_none() {
                return Err(storage_error(format!(
                    "game {} is not complete; observations cannot be refreshed",
                    game_id.as_str()
                )));
            }
            assert_no_pending_file_mutations_within_transaction(transaction, game_id)?;
            replace_game_observations_within_transaction(transaction, game_id, observations)
        })
    }

    /// Lists only observations owned by one game. Artifact-owned facts are
    /// deliberately excluded so one owner can never supply another owner's
    /// detection cache.
    pub fn list_game_observations(
        &self,
        game_id: &GameId,
    ) -> AppResult<Vec<StoredFileObservation>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "
                    SELECT normalized_path, identity_kind, object_identity, change_token,
                           size, algorithm_revision, sha256,
                           version_observed, version, runtime_observed, runtime_json,
                           pe_observed, pe_json
                      FROM file_observations
                     WHERE owner_kind = 'game' AND game_id = ?1
                     ORDER BY normalized_path
                    ",
                )
                .map_err(storage_error)?;
            let rows = statement
                .query_map([game_id.as_str()], |row| {
                    Ok(observation_from_row(
                        row,
                        ObservationOwner::Game(game_id.clone()),
                    ))
                })
                .map_err(storage_error)?;
            let rows = rows.collect::<Result<Vec<_>, _>>().map_err(storage_error)?;
            rows.into_iter().collect()
        })
    }
}

pub(super) fn readiness_in_connection(
    connection: &Connection,
    game_id: &GameId,
) -> AppResult<CatalogReadiness> {
    let row = connection
        .query_row(
            "SELECT readiness, authority_epoch, invalidation_reason, mutation_token
             FROM catalog_scan_authority WHERE game_id = ?1",
            [game_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(storage_error)?
        .ok_or_else(|| storage_error(format!("game {} has no scan authority", game_id.as_str())))?;
    readiness_from_row(game_id, row)
}

pub(in super::super) fn readiness_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<CatalogReadiness> {
    readiness_in_connection(transaction, game_id)
}

fn readiness_from_row(
    game_id: &GameId,
    (readiness, epoch, reason, token): (String, i64, Option<String>, Option<String>),
) -> AppResult<CatalogReadiness> {
    let authority_epoch =
        u64::try_from(epoch).map_err(|_| invalid_row("negative scan authority epoch"))?;
    match readiness.as_str() {
        "never_completed" => Ok(CatalogReadiness::NeverCompleted { authority_epoch }),
        "complete" => Ok(CatalogReadiness::Complete(CatalogReadyProjection {
            game_id: game_id.clone(),
            authority_epoch,
        })),
        "invalidated" => Ok(CatalogReadiness::Invalidated {
            authority_epoch,
            reason: reason.ok_or_else(|| invalid_row("invalidated authority is missing reason"))?,
            mutation_token: token,
        }),
        _ => Err(invalid_row(format!(
            "invalid catalog scan readiness '{readiness}'"
        ))),
    }
}

pub(in super::super) fn invalidate_game_authority_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    reason: &str,
    mutation_token: Option<&str>,
) -> AppResult<CatalogReadiness> {
    if reason.trim().is_empty() {
        return Err(invalid_row("scan invalidation reason must not be empty"));
    }
    // Generic catalog invalidation always requires existing total authority.
    // The pending-mutation repository owns the one deliberate pre-catalog
    // exception and classifies its game/authority pair before calling here.
    let _current = readiness_within_transaction(transaction, game_id)?;
    delete_game_observations_within_transaction(transaction, game_id)?;
    let now_ms = sqlite_clock::now_ms(transaction)?;
    let updated = transaction
        .execute(
            "UPDATE catalog_scan_authority
             SET readiness = 'invalidated',
                 authority_epoch = authority_epoch + 1,
                 invalidation_reason = :reason,
                 mutation_token = :mutation_token,
                 completed_at = NULL,
                 updated_at = :updated_at
             WHERE game_id = :game_id",
            named_params! {
                ":game_id": game_id.as_str(),
                ":reason": reason,
                ":mutation_token": mutation_token,
                ":updated_at": now_ms,
            },
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(storage_error(format!(
            "could not invalidate scan authority for {}",
            game_id.as_str()
        )));
    }
    readiness_within_transaction(transaction, game_id)
}

pub(in super::super) fn assert_no_pending_file_mutations_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<()> {
    let pending: Option<String> = transaction
        .query_row(
            "SELECT id FROM pending_file_mutations WHERE game_id = ?1 LIMIT 1",
            [game_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;
    if let Some(id) = pending {
        return Err(storage_error(format!(
            "game {} has pending file mutation '{id}'; component replacement is blocked",
            game_id.as_str()
        )));
    }
    Ok(())
}

pub(super) fn delete_game_observations_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<()> {
    transaction
        .execute(
            "DELETE FROM file_observations WHERE owner_kind = 'game' AND game_id = ?1",
            [game_id.as_str()],
        )
        .map_err(storage_error)?;
    Ok(())
}

pub(in super::super) fn replace_game_observations_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    observations: &[StoredFileObservation],
) -> AppResult<()> {
    ensure_only_game_owner(game_id, observations)?;
    delete_game_observations_within_transaction(transaction, game_id)?;
    replace_observations_within_transaction(transaction, observations)
}
