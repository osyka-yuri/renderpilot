use renderpilot_application::AppResult;
use renderpilot_domain::{AddonKind, GameId};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::error::{invalid_row, storage_error};
use crate::{mapping, sqlite_clock};

use super::SqliteStorage;

impl SqliteStorage {
    /// Reads the durable capability projection in one query.
    pub fn list_profile_addon_capabilities(&self) -> AppResult<Vec<(GameId, AddonKind)>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT game_id, addon_kind FROM profile_addon_capabilities \
                     ORDER BY game_id, addon_kind",
                )
                .map_err(storage_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(storage_error)?;
            rows.map(|row| {
                let (game_id, kind) = row.map_err(storage_error)?;
                let kind = AddonKind::from_stable_str(&kind)
                    .ok_or_else(|| invalid_row(format!("unknown add-on kind `{kind}`")))?;
                Ok((mapping::game_id(game_id)?, kind))
            })
            .collect()
        })
    }

    /// Reads the durable capabilities for one details projection without
    /// materializing capability rows for the rest of the catalog.
    pub fn list_profile_addon_capabilities_for_game(
        &self,
        game_id: &GameId,
    ) -> AppResult<Vec<AddonKind>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT addon_kind FROM profile_addon_capabilities \
                     WHERE game_id = ?1 ORDER BY addon_kind",
                )
                .map_err(storage_error)?;
            let rows = statement
                .query_map([game_id.as_str()], |row| row.get::<_, String>(0))
                .map_err(storage_error)?;
            rows.map(|row| {
                let kind = row.map_err(storage_error)?;
                AddonKind::from_stable_str(&kind)
                    .ok_or_else(|| invalid_row(format!("unknown add-on kind `{kind}`")))
            })
            .collect()
        })
    }

    /// Atomically replaces exactly one manifest-derived capability kind.
    pub fn replace_profile_addon_capabilities(
        &self,
        kind: AddonKind,
        source_revision: &str,
        game_ids: &[GameId],
    ) -> AppResult<bool> {
        self.with_transaction(|transaction| {
            replace_kind(transaction, kind, source_revision, game_ids)
        })
    }

    /// Atomically refreshes only the supplied capability kinds for one game.
    /// Missing kinds are intentionally preserved after a partial probe load.
    pub fn replace_game_profile_addon_capabilities(
        &self,
        game_id: &GameId,
        capabilities: &[(AddonKind, String, bool)],
    ) -> AppResult<bool> {
        self.with_transaction(|transaction| {
            let mut changed = false;
            for (kind, source_revision, available) in capabilities {
                let current_revision = transaction
                    .query_row(
                        "SELECT source_revision FROM profile_addon_capabilities \
                         WHERE game_id = ?1 AND addon_kind = ?2",
                        params![game_id.as_str(), kind.as_str()],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(storage_error)?;
                if (*available && current_revision.as_deref() == Some(source_revision.as_str()))
                    || (!*available && current_revision.is_none())
                {
                    continue;
                }
                changed = true;
                transaction
                    .execute(
                        "DELETE FROM profile_addon_capabilities \
                         WHERE game_id = ?1 AND addon_kind = ?2",
                        params![game_id.as_str(), kind.as_str()],
                    )
                    .map_err(storage_error)?;
                if *available {
                    let now = sqlite_clock::now_ms(transaction)?;
                    transaction
                        .execute(
                            "INSERT INTO profile_addon_capabilities \
                             (game_id, addon_kind, source_revision, updated_at) \
                             VALUES (?1, ?2, ?3, ?4)",
                            params![game_id.as_str(), kind.as_str(), source_revision, now],
                        )
                        .map_err(storage_error)?;
                }
            }
            Ok(changed)
        })
    }
}

fn replace_kind(
    transaction: &Transaction<'_>,
    kind: AddonKind,
    source_revision: &str,
    game_ids: &[GameId],
) -> AppResult<bool> {
    let mut current = transaction
        .prepare_cached(
            "SELECT game_id, source_revision FROM profile_addon_capabilities \
             WHERE addon_kind = ?1 ORDER BY game_id",
        )
        .map_err(storage_error)?
        .query_map([kind.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(storage_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage_error)?;
    let mut expected = game_ids
        .iter()
        .map(|game_id| (game_id.as_str(), source_revision))
        .collect::<Vec<_>>();
    current.sort_unstable();
    expected.sort_unstable();
    if current.len() == expected.len()
        && current
            .iter()
            .zip(&expected)
            .all(|((game_id, revision), expected)| game_id == expected.0 && revision == expected.1)
    {
        return Ok(false);
    }

    transaction
        .execute(
            "DELETE FROM profile_addon_capabilities WHERE addon_kind = ?1",
            [kind.as_str()],
        )
        .map_err(storage_error)?;
    let now = sqlite_clock::now_ms(transaction)?;
    let mut insert = transaction
        .prepare_cached(
            "INSERT INTO profile_addon_capabilities \
             (game_id, addon_kind, source_revision, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(storage_error)?;
    for game_id in game_ids {
        insert
            .execute(params![
                game_id.as_str(),
                kind.as_str(),
                source_revision,
                now
            ])
            .map_err(storage_error)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };

    use super::*;

    fn seed_game(storage: &SqliteStorage, value: &str) -> GameId {
        let game_id = GameId::new(value).expect("game id");
        let identity =
            GameIdentity::new(game_id.clone(), value, Launcher::Manual).expect("game identity");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(format!("C:/Games/{value}")).expect("path"),
        );
        storage.upsert_game(&game).expect("seed game");
        game_id
    }

    #[test]
    fn replacing_one_kind_preserves_other_kinds() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = seed_game(&storage, "game-one");
        storage
            .replace_profile_addon_capabilities(
                AddonKind::RenoDx,
                "reno-1",
                std::slice::from_ref(&game_id),
            )
            .expect("RenoDX capabilities");
        storage
            .replace_profile_addon_capabilities(
                AddonKind::Luma,
                "luma-1",
                std::slice::from_ref(&game_id),
            )
            .expect("Luma capabilities");

        storage
            .replace_profile_addon_capabilities(AddonKind::Luma, "luma-2", &[])
            .expect("replace only Luma");

        assert_eq!(
            storage
                .list_profile_addon_capabilities()
                .expect("capabilities"),
            vec![(game_id, AddonKind::RenoDx)]
        );
    }

    #[test]
    fn failed_kind_replacement_keeps_last_valid_rows() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = seed_game(&storage, "game-two");
        storage
            .replace_profile_addon_capabilities(
                AddonKind::Luma,
                "luma-1",
                std::slice::from_ref(&game_id),
            )
            .expect("initial capabilities");

        let result = storage.replace_profile_addon_capabilities(
            AddonKind::Luma,
            "luma-2",
            &[game_id.clone(), game_id.clone()],
        );

        assert!(result.is_err());
        assert_eq!(
            storage
                .list_profile_addon_capabilities()
                .expect("capabilities"),
            vec![(game_id, AddonKind::Luma)]
        );
    }

    #[test]
    fn identical_kind_replacement_is_a_generation_preserving_noop() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_id = seed_game(&storage, "game-three");
        assert!(
            storage
                .replace_profile_addon_capabilities(
                    AddonKind::Luma,
                    "luma-1",
                    std::slice::from_ref(&game_id),
                )
                .expect("initial capabilities")
        );
        let generation = storage.catalog_generation();

        assert!(
            !storage
                .replace_profile_addon_capabilities(
                    AddonKind::Luma,
                    "luma-1",
                    std::slice::from_ref(&game_id),
                )
                .expect("identical capabilities")
        );
        assert_eq!(storage.catalog_generation(), generation);
    }
}
